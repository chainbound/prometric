# `prometric`

[![Lints](https://github.com/chainbound/prometric/actions/workflows/lint.yml/badge.svg?branch=main)](https://github.com/chainbound/prometric/actions/workflows/lint.yml)
[![Tests](https://github.com/chainbound/prometric/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/chainbound/prometric/actions/workflows/test.yml)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/chainbound/prometric)

A library for ergonomically generating and using embedded Prometheus metrics in Rust.

Inspired by [metrics-derive](https://github.com/ithacaxyz/metrics-derive), but works directly with [prometheus](https://docs.rs/prometheus/latest/prometheus)
instead of [metrics](https://docs.rs/metrics/latest/metrics), and supports dynamic labels.

| Crate              | crates.io                                                                                                       | docs.rs                                                                                    |
| ------------------ | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `prometric`        | [![crates.io](https://img.shields.io/crates/v/prometric.svg)](https://crates.io/crates/prometric)               | [![docs.rs](https://docs.rs/prometric/badge.svg)](https://docs.rs/prometric)               |
| `prometric-derive` | [![crates.io](https://img.shields.io/crates/v/prometric-derive.svg)](https://crates.io/crates/prometric-derive) | [![docs.rs](https://docs.rs/prometric-derive/badge.svg)](https://docs.rs/prometric-derive) |

## Usage

### Basic Usage

See [`basic_usage`](./prometric-derive/examples/basic_usage.rs) example for usage. Here's a reduced example usage:

``` rust
// The `scope` attribute is used to set the prefix for the metric names in this struct.
#[metrics(scope = "app")]
struct AppMetrics {
    /// The total number of HTTP requests.
    #[metric(rename = "http_requests_total", labels = ["method", "path"])]
    http_requests: Counter,
}

// Build the metrics struct with static labels, which will initialize and register the metrics
// with the default registry. A custom registry can be used by passing it to the builder
// using `with_registry`.
let metrics =
    AppMetrics::builder().with_label("host", "localhost").with_label("port", "8080").build();

// Metric fields each get an accessor method generated, which can be used to interact with the
// metric. The arguments to the accessor method are the labels for the metric.
metrics.http_requests("GET", "/").inc();
```

#### Sample Output

TODO: document how to obtain sample output

```text
# HELP app_account_balance The balance of the account, in dollars. Uses a floating point number.
# TYPE app_account_balance gauge
app_account_balance{account_id="1234567890",host="localhost",port="8080"} -12.2

# HELP app_current_active_users The current number of active users.
# TYPE app_current_active_users gauge
app_current_active_users{host="localhost",port="8080",service="service-1"} 20

# HELP app_errors The total number of errors.
# TYPE app_errors counter
app_errors{host="localhost",port="8080"} 1

# HELP app_http_requests_duration The duration of HTTP requests.
# TYPE app_http_requests_duration histogram
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.005"} 0
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.01"} 0
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.025"} 0
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.05"} 0
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.1"} 0
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.25"} 0
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.5"} 0
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="1"} 1
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="2.5"} 1
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="5"} 1
app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="+Inf"} 1
app_http_requests_duration_sum{host="localhost",method="GET",path="/",port="8080"} 1
app_http_requests_duration_count{host="localhost",method="GET",path="/",port="8080"} 1

# HELP app_http_requests_total The total number of HTTP requests.
# TYPE app_http_requests_total counter
app_http_requests_total{host="localhost",method="GET",path="/",port="8080"} 2
app_http_requests_total{host="localhost",method="POST",path="/",port="8080"} 2
```

### Static Metrics

You can also generate a static `LazyLock` instance by using the `static` attribute. When enabled, the builder methods and `Default` implementation are made private, ensuring the only way to access the metrics is through the static instance:

See [`static_metrics`](./prometric-derive/examples/static_metrics.rs) example for usage.

### Exporting Metrics

An HTTP exporter is provided by [`prometric::exporter::ExporterBuilder`].

See [`exporter`](./prometric-derive/examples/exporter.rs) example for usage.

### Process Metrics

When the `process` feature is enabled, the `ProcessCollector` can be used to collect metrics about the current process.

```rust
use prometric::process::ProcessCollector;

let collector = ProcessCollector::default();
collector.collect();
```

#### Sample Output

```text
# HELP process_collection_duration_seconds The duration of the associated collection routine in seconds.
# TYPE process_collection_duration_seconds gauge
process_collection_duration_seconds 0.016130356
# HELP process_cpu_usage The CPU usage of the process as a percentage.
# TYPE process_cpu_usage gauge
process_cpu_usage 6.2536187171936035
# HELP process_disk_written_bytes_total The total written bytes to disk by the process.
# TYPE process_disk_written_bytes_total gauge
process_disk_written_bytes_total 0
# HELP process_max_fds The maximum number of open file descriptors of the process.
# TYPE process_max_fds gauge
process_max_fds 1048576
# HELP process_open_fds The number of open file descriptors of the process.
# TYPE process_open_fds gauge
process_open_fds 639
# HELP process_resident_memory_bytes The resident memory of the process in bytes. (RSS)
# TYPE process_resident_memory_bytes gauge
process_resident_memory_bytes 4702208
# HELP process_resident_memory_usage The resident memory usage of the process as a percentage of the total memory available.
# TYPE process_resident_memory_usage gauge
process_resident_memory_usage 0.00007072418111501723
# HELP process_start_time_seconds The start time of the process in UNIX seconds.
# TYPE process_start_time_seconds gauge
process_start_time_seconds 1763056609
# HELP process_thread_usage Per-thread CPU usage as a percentage of the process's CPU usage (Linux only).
# TYPE process_thread_usage gauge
process_thread_usage{name="process::tests:",pid="980490"} 0.9259260296821594
process_thread_usage{name="test-thread-1",pid="980491"} 0
process_thread_usage{name="test-thread-2",pid="980492"} 94.44445037841797
# HELP process_threads The number of OS threads used by the process (Linux only).
# TYPE process_threads gauge
process_threads 3
# HELP system_cpu_cores The number of logical CPU cores available in the system.
# TYPE system_cpu_cores gauge
system_cpu_cores 16
# HELP system_cpu_usage System-wide CPU usage percentage.
# TYPE system_cpu_usage gauge
system_cpu_usage 6.7168498039245605
# HELP system_max_cpu_frequency The maximum CPU frequency of all cores in MHz.
# TYPE system_max_cpu_frequency gauge
system_max_cpu_frequency 5339
# HELP system_memory_usage System-wide memory usage percentage.
# TYPE system_memory_usage gauge
system_memory_usage 6.398677876736871
# HELP system_min_cpu_frequency The minimum CPU frequency of all cores in MHz.
# TYPE system_min_cpu_frequency gauge
system_min_cpu_frequency 545
```
