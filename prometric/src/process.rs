use prometheus::{
    Gauge, GaugeVec, Opts, Registry,
    core::{AtomicU64, GenericGauge},
};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, RefreshKind, System};

type UintGauge = GenericGauge<AtomicU64>;

type UintCounter = GenericGauge<AtomicU64>;

/// A collector for process (and some system) metrics.
///
/// # Metrics
/// See the documentation for the [`ProcessMetrics`] struct for the list of metrics.
///
/// # Example
/// ```rust
/// use prometheus::Registry;
/// use prometric::process::ProcessCollector;
///
/// let registry = Registry::new();
/// let mut collector = ProcessCollector::new(&registry);
///
/// // OR run with the default registry
/// let mut collector = ProcessCollector::default();
///
/// // Collect the metrics
/// collector.collect();
/// ```
pub struct ProcessCollector {
    specifics: RefreshKind,
    sys: System,
    cores: u64,

    metrics: ProcessMetrics,
}

impl Default for ProcessCollector {
    fn default() -> Self {
        Self::new(prometheus::default_registry())
    }
}

impl ProcessCollector {
    /// Create a new `ProcessCollector` with the given registry.
    pub fn new(registry: &Registry) -> Self {
        // Create the stats that will be refreshed
        let specifics = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::nothing().with_ram())
            .with_processes(
                ProcessRefreshKind::nothing()
                    .with_cpu()
                    .with_memory()
                    .with_disk_usage()
                    .with_tasks(),
            );

        let mut sys = sysinfo::System::new_with_specifics(specifics);

        // Refresh system information immediately for our first data point.
        sys.refresh_specifics(specifics);

        let cores = sys.cpus().len() as u64;
        let metrics = ProcessMetrics::new(registry);

        Self { specifics, sys, cores, metrics }
    }

    /// Get the PID of the current process.
    pub fn pid(&self) -> u32 {
        Pid::from_u32(std::process::id()).as_u32()
    }

    /// Collect system and process metrics.
    pub fn collect(&mut self) {
        let start = std::time::Instant::now();
        self.sys.refresh_specifics(self.specifics);

        let cpus = self.sys.cpus();
        let min_cpu_freq = cpus.iter().map(|cpu| cpu.frequency()).min().unwrap();
        let max_cpu_freq = cpus.iter().map(|cpu| cpu.frequency()).max().unwrap();
        let system_cpu_usage = self.sys.global_cpu_usage();
        let system_memory_usage =
            self.sys.used_memory() as f64 / self.sys.total_memory() as f64 * 100.0;

        let Some(process) = self.sys.process(Pid::from_u32(self.pid())) else {
            return;
        };

        let cpu_usage = process.cpu_usage() / self.cores as f32;

        // Collect thread stats
        if let Some(tasks) = process.tasks() {
            tasks.iter().for_each(|pid| {
                let Some(thread) = self.sys.process(*pid) else {
                    return;
                };

                let pid = pid.to_string();
                let name = thread.name().to_str().unwrap_or(pid.as_str());

                self.metrics
                    .thread_usage
                    .with_label_values(&[pid.as_str(), name])
                    .set(thread.cpu_usage() as f64);
            });
        }

        let threads = process.tasks().map(|tasks| tasks.len()).unwrap_or(0);
        let open_fds = process.open_files().unwrap_or(0);
        let max_fds = process.open_files_limit().unwrap_or(0);
        let resident_memory = process.memory();
        let resident_memory_usage = resident_memory as f64 / self.sys.total_memory() as f64;
        let disk_usage = process.disk_usage().total_written_bytes;

        self.metrics.system_cores.set(self.cores);
        self.metrics.system_max_cpu_freq.set(max_cpu_freq);
        self.metrics.system_min_cpu_freq.set(min_cpu_freq);
        self.metrics.system_cpu_usage.set(system_cpu_usage as f64);
        self.metrics.system_memory_usage.set(system_memory_usage);

        self.metrics.threads.set(threads as u64);
        self.metrics.cpu_usage.set(cpu_usage as f64);
        self.metrics.resident_memory.set(resident_memory);
        self.metrics.resident_memory_usage.set(resident_memory_usage);
        self.metrics.start_time.set(process.start_time());
        self.metrics.open_fds.set(open_fds as u64);
        self.metrics.max_fds.set(max_fds as u64);
        self.metrics.disk_written_bytes.set(disk_usage);

        // Record the duration of the collection routine
        self.metrics.collection_duration.set(start.elapsed().as_secs_f64());
    }
}

/// A collection of metrics for a process, with some useful system metrics.
pub struct ProcessMetrics {
    // System metrics
    /// The number of logical CPU cores available in the system.
    system_cores: UintGauge,
    /// The maximum CPU frequency of all cores in MHz.
    system_max_cpu_freq: UintGauge,
    /// The minimum CPU frequency of all cores in MHz.
    system_min_cpu_freq: UintGauge,
    /// The system-wide CPU usage percentage.
    system_cpu_usage: Gauge,
    /// The system-wide memory usage percentage.
    system_memory_usage: Gauge,

    // Process metrics
    /// The number of OS threads used by the process (Linux only).
    threads: UintGauge,
    /// The CPU usage of the process as a percentage.
    cpu_usage: Gauge,
    /// The resident memory of the process in bytes. (RSS)
    resident_memory: UintGauge,
    /// The resident memory usage of the process as a percentage of the total memory available.
    resident_memory_usage: Gauge,
    /// The start time of the process in UNIX seconds.
    start_time: UintGauge,
    /// The number of open file descriptors of the process.
    open_fds: UintGauge,
    /// The maximum number of open file descriptors of the process.
    max_fds: UintGauge,
    /// The total written bytes to disk by the process.
    disk_written_bytes: UintCounter,
    /// The statistics of the threads used by the process (Linux only).
    thread_usage: GaugeVec,

    /// The duration of the associated collection routine in seconds.
    collection_duration: Gauge,
}

impl ProcessMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        let system_cores = UintGauge::new(
            "system_cpu_cores",
            "The number of logical CPU cores available in the system.",
        )
        .unwrap();
        let system_max_cpu_freq = UintGauge::new(
            "system_max_cpu_frequency",
            "The maximum CPU frequency of all cores in MHz.",
        )
        .unwrap();
        let system_min_cpu_freq = UintGauge::new(
            "system_min_cpu_frequency",
            "The minimum CPU frequency of all cores in MHz.",
        )
        .unwrap();
        let system_cpu_usage =
            Gauge::new("system_cpu_usage", "System-wide CPU usage percentage.").unwrap();
        let system_memory_usage =
            Gauge::new("system_memory_usage", "System-wide memory usage percentage.").unwrap();

        let threads = UintGauge::new(
            "process_threads",
            "The number of OS threads used by the process (Linux only).",
        )
        .unwrap();
        let cpu_usage =
            Gauge::new("process_cpu_usage", "The CPU usage of the process as a percentage.")
                .unwrap();
        let resident_memory = UintGauge::new(
            "process_resident_memory_bytes",
            "The resident memory of the process in bytes. (RSS)",
        )
        .unwrap();
        let resident_memory_usage = Gauge::new(
            "process_resident_memory_usage",
            "The resident memory usage of the process as a percentage of the total memory available.",
        )
        .unwrap();
        let start_time = UintGauge::new(
            "process_start_time_seconds",
            "The start time of the process in UNIX seconds.",
        )
        .unwrap();
        let open_fds = UintGauge::new(
            "process_open_fds",
            "The number of open file descriptors of the process.",
        )
        .unwrap();
        let max_fds = UintGauge::new(
            "process_max_fds",
            "The maximum number of open file descriptors of the process.",
        )
        .unwrap();
        let disk_written_bytes = UintCounter::new(
            "process_disk_written_bytes_total",
            "The total written bytes to disk by the process.",
        )
        .unwrap();
        let thread_usage: GaugeVec = GaugeVec::new(
            Opts::new(
                "process_thread_usage",
                "Per-thread CPU usage as a percentage of the process's CPU usage (Linux only).",
            ),
            &["pid", "name"],
        )
        .unwrap();

        let collection_duration = Gauge::new(
            "process_collection_duration_seconds",
            "The duration of the associated collection routine in seconds.",
        )
        .unwrap();

        // Register all metrics with the registry
        registry.register(Box::new(system_cores.clone())).unwrap();
        registry.register(Box::new(system_max_cpu_freq.clone())).unwrap();
        registry.register(Box::new(system_min_cpu_freq.clone())).unwrap();
        registry.register(Box::new(system_cpu_usage.clone())).unwrap();
        registry.register(Box::new(system_memory_usage.clone())).unwrap();

        registry.register(Box::new(threads.clone())).unwrap();
        registry.register(Box::new(cpu_usage.clone())).unwrap();
        registry.register(Box::new(resident_memory.clone())).unwrap();
        registry.register(Box::new(resident_memory_usage.clone())).unwrap();
        registry.register(Box::new(start_time.clone())).unwrap();
        registry.register(Box::new(open_fds.clone())).unwrap();
        registry.register(Box::new(max_fds.clone())).unwrap();
        registry.register(Box::new(disk_written_bytes.clone())).unwrap();
        registry.register(Box::new(thread_usage.clone())).unwrap();

        registry.register(Box::new(collection_duration.clone())).unwrap();

        Self {
            system_cores,
            system_max_cpu_freq,
            system_min_cpu_freq,
            system_cpu_usage,
            system_memory_usage,
            threads,
            cpu_usage,
            resident_memory,
            resident_memory_usage,
            start_time,
            open_fds,
            max_fds,
            disk_written_bytes,
            thread_usage,
            collection_duration,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{hash::Hasher as _, thread, time::Instant};

    use super::*;

    #[test]
    fn test_process_collector() {
        let handle = thread::Builder::new()
            .name("test-thread-1".to_string())
            .spawn(|| {
                let mut hasher = std::hash::DefaultHasher::new();
                let end = Instant::now() + std::time::Duration::from_secs(3);

                // Busy loop with small sleep
                while Instant::now() < end {
                    for i in 0..10000 {
                        hasher.write_u64(i);
                    }
                }

                println!("test-thread-1: {}", hasher.finish());
            })
            .unwrap();

        let handle2 = thread::Builder::new()
            .name("test-thread-2".to_string())
            .spawn(|| {
                let end = Instant::now() + std::time::Duration::from_secs(3);
                let mut sum = 0u64;

                // Busy loop
                while Instant::now() < end {
                    for i in 0..10000 {
                        sum = sum.wrapping_add(i * i);
                    }
                }

                println!("test-thread-2: {}", sum);
            })
            .unwrap();

        let registry = Registry::new();
        let mut collector = ProcessCollector::new(&registry);
        collector.collect();

        std::thread::sleep(std::time::Duration::from_secs(1));
        let start = Instant::now();
        collector.collect();

        let duration = start.elapsed();
        println!("Time taken for collection: {:?}", duration);

        let metrics = registry.gather();
        let encoder = prometheus::TextEncoder::new();
        let body = encoder.encode_to_string(&metrics).unwrap();
        println!("{}", body);

        handle.join().unwrap();
        handle2.join().unwrap();
    }
}
