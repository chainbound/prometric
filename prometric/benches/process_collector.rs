//! Benchmarks for [`ProcessCollector`] refresh cost.
//!
//! # Purpose
//!
//! `sysinfo` refreshes can take tens of milliseconds depending on which
//! subsystems are polled. These benchmarks isolate the cost of each
//! [`RefreshKind`] component so we can document collection overhead and
//! justify any changes to the default configuration.
//!
//! # Structure
//!
//! Each benchmark function represents one [`RefreshKind`] configuration:
//! - [`cpu_only`]         — floor cost, no freq polling, no tasks, no disk
//! - [`cpu_with_freq`]    — isolates the overhead of CPU frequency reads
//! - [`with_disk_usage`]  — isolates `/proc/<pid>/io` (Linux) read cost
//! - [`with_tasks`]       — isolates `/proc/<pid>/task/*` enumeration cost
//! - [`current_default`]  — exact config used by [`ProcessCollector`] today
//! - [`proposed_slim`]    — candidate slim config, drops freq polling
//!
//! # Running
//!
//! ```bash
//! # All benchmarks, bencher output (used by CI)
//! cargo bench --bench process_collector -- --output-format bencher --noplot
//!
//! # Single benchmark with full Criterion HTML report
//! cargo bench --bench process_collector -- with_tasks
//!
//! # Quick smoke-run (1 sample, no statistical analysis)
//! cargo bench --bench process_collector -- --sample-size 10
//! ```

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, RefreshKind, System};

/// Runs a single [`RefreshKind`] configuration under the `process_refresh` group.
///
/// # Warm-up
///
/// A single [`System::refresh_specifics`] call is made before the benchmark
/// loop starts. This mirrors [`ProcessCollector::new`], which also does one
/// eager refresh so the first [`collect`] call has a valid prior sample to
/// diff against (required for accurate CPU % calculation).
///
/// Without the warm-up, the first iteration would measure cold-cache kernel
/// data structures rather than steady-state refresh cost.
fn bench_refresh(c: &mut Criterion, label: &str, specifics: RefreshKind) {
    let mut group = c.benchmark_group("process_refresh");

    let mut sys = System::new_with_specifics(specifics);
    sys.refresh_specifics(specifics); // warm-up — mirrors ProcessCollector::new()

    group.bench_with_input(BenchmarkId::new("refresh_specifics", label), &specifics, |b, &spec| {
        b.iter(|| sys.refresh_specifics(spec))
    });

    group.finish();
}

/// Cheapest meaningful config: CPU usage + RAM + per-process CPU/memory.
///
/// No frequency reads, no thread enumeration, no disk I/O stats.
/// This is the floor — the minimum needed to populate all non-optional metrics.
/// Use this as the baseline when reading results; every other config adds
/// cost on top of this number.
fn cpu_only(c: &mut Criterion) {
    bench_refresh(
        c,
        "cpu_usage_only",
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
            .with_memory(MemoryRefreshKind::nothing().with_ram())
            .with_processes(ProcessRefreshKind::nothing().with_cpu().with_memory()),
    );
}

/// Adds CPU frequency reads on top of [`cpu_only`].
///
/// On Linux this reads `/sys/bus/cpu/devices/cpu*/cpufreq/scaling_cur_freq`
/// (one sysfs read per logical core). On macOS it goes through `sysctl`.
/// The delta between this and [`cpu_only`] is the pure frequency-polling cost.
///
/// Frequency is used by [`ProcessCollector`] for `system_min/max_cpu_frequency`
/// gauges. If those metrics are not needed, dropping [`CpuRefreshKind::everything`]
/// for [`CpuRefreshKind::nothing().with_cpu_usage()`] is the first easy saving.
fn cpu_with_freq(c: &mut Criterion) {
    bench_refresh(
        c,
        "cpu_with_frequency",
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything()) // everything() = usage + frequency
            .with_memory(MemoryRefreshKind::nothing().with_ram())
            .with_processes(ProcessRefreshKind::nothing().with_cpu().with_memory()),
    );
}

/// Adds disk I/O accounting on top of [`cpu_only`].
///
/// On Linux this reads `/proc/<pid>/io` (one syscall per tracked process).
/// On macOS it uses `proc_pidinfo` with `PROC_PIDTASKINFO`.
/// The delta between this and [`cpu_only`] is the pure disk-stat cost.
///
/// Used by [`ProcessCollector`] for the `process_disk_written_bytes_total` counter.
fn with_disk_usage(c: &mut Criterion) {
    bench_refresh(
        c,
        "with_disk_usage",
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
            .with_memory(MemoryRefreshKind::nothing().with_ram())
            .with_processes(
                ProcessRefreshKind::nothing().with_cpu().with_memory().with_disk_usage(),
            ),
    );
}

/// Adds thread/task enumeration on top of [`cpu_only`].
///
/// On Linux this walks `/proc/<pid>/task/*` — cost scales with thread count.
/// On macOS `with_tasks()` is a no-op; sysinfo does not expose per-thread
/// data there, so this bench will read identically to [`cpu_only`] on macOS.
///
/// Expected to be the most expensive component on Linux for processes with
/// many threads. The delta between this and [`cpu_only`] is the task-walk cost.
///
/// Used by [`ProcessCollector`] for `process_threads` and the
/// `process_thread_usage` gauge vec.
fn with_tasks(c: &mut Criterion) {
    bench_refresh(
        c,
        "with_tasks",
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
            .with_memory(MemoryRefreshKind::nothing().with_ram())
            .with_processes(ProcessRefreshKind::nothing().with_cpu().with_memory().with_tasks()),
    );
}

/// The exact [`RefreshKind`] configuration used by [`ProcessCollector::new`] today.
///
/// This is the reference point for any proposed changes. All other configs
/// should be compared against this number — not against each other — so the
/// PR can state a concrete "X% reduction in collection cost".
fn current_default(c: &mut Criterion) {
    bench_refresh(
        c,
        "current_default",
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::nothing().with_ram())
            .with_processes(
                ProcessRefreshKind::nothing()
                    .with_cpu()
                    .with_memory()
                    .with_disk_usage()
                    .with_tasks(),
            ),
    );
}

/// Proposed slim config: identical to [`current_default`] but drops frequency polling.
///
/// Rationale: `system_min/max_cpu_frequency` gauges are rarely actionable in
/// a process-health dashboard. Dropping [`CpuRefreshKind::everything`] in favour
/// of [`CpuRefreshKind::nothing().with_cpu_usage()`] removes the per-core sysfs
/// reads while keeping all other metrics intact.
///
/// If the delta vs [`current_default`] is meaningful (>~20% on either platform),
/// this config should become the new default and the frequency gauges should be
/// moved behind an opt-in flag.
fn proposed_slim(c: &mut Criterion) {
    bench_refresh(
        c,
        "proposed_slim",
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage()) // freq dropped
            .with_memory(MemoryRefreshKind::nothing().with_ram())
            .with_processes(
                ProcessRefreshKind::nothing()
                    .with_cpu()
                    .with_memory()
                    .with_disk_usage()
                    .with_tasks(),
            ),
    );
}

criterion_group!(
    benches,
    cpu_only,
    cpu_with_freq,
    with_disk_usage,
    with_tasks,
    current_default,
    proposed_slim
);
criterion_main!(benches);
