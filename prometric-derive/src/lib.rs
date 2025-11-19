//! This crate contains the attribute macro for generating Prometheus metrics.
//! Refer to the [metrics] attribute documentation for more information.
use proc_macro::TokenStream;
use syn::{ItemStruct, parse_macro_input};

use crate::expand::MetricsAttr;

mod expand;
mod utils;

/// This attribute macro instruments all of the struct fields with Prometheus metrics according to
/// the attributes on the fields. It also generates an ergonomic accessor API for each of the
/// defined metrics.
///
/// # Attributes
///
/// - `scope`: Sets the prefix for metric names (required)
/// - `static`: If enabled, generates a static `LazyLock` with a SCREAMING_SNAKE_CASE name.
///
/// # Example
/// ```rust
/// use prometric_derive::metrics;
/// use prometric::{Counter, Gauge, Histogram};
///
/// // The `scope` attribute is used to set the prefix for the metric names in this struct.
/// #[metrics(scope = "app")]
/// struct AppMetrics {
///     /// The total number of HTTP requests.
///     #[metric(rename = "http_requests_total", labels = ["method", "path"])]
///     http_requests: Counter,
///
///     // For histograms, the `buckets` attribute is optional. It will default to [prometheus::DEFAULT_BUCKETS] if not provided.
///     /// The duration of HTTP requests.
///     #[metric(labels = ["method", "path"], buckets = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])]
///     http_requests_duration: Histogram,
///
///     /// This doc comment will be overwritten by the `help` attribute.
///     #[metric(rename = "current_active_users", labels = ["service"], help = "The current number of active users.")]
///     current_users: Gauge,
///
///     /// The balance of the account, in dollars. Uses a floating point number.
///     #[metric(rename = "account_balance", labels = ["account_id"])]
///     account_balance: Gauge<f64>,
///
///     /// The total number of errors.
///     #[metric]
///     errors: Counter,
/// }
///
/// // Build the metrics struct with static labels, which will initialize and register the metrics with the default registry.
/// // A custom registry can be used by passing it to the builder using `with_registry`.
/// let metrics = AppMetrics::builder().with_label("host", "localhost").with_label("port", "8080").build();
///
/// // Metric fields each get an accessor method generated, which can be used to interact with the metric.
/// // The arguments to the accessor method are the labels for the metric.
/// metrics.http_requests("GET", "/").inc();
/// metrics.http_requests_duration("GET", "/").observe(1.0);
/// metrics.current_users("service-1").set(10);
/// metrics.account_balance("1234567890").set(-12.2);
/// metrics.errors().inc();
/// ```
///
/// # Sample Output
/// ```text
/// # HELP app_account_balance The balance of the account, in dollars. Uses a floating point number.
/// # TYPE app_account_balance gauge
/// app_account_balance{account_id="1234567890",host="localhost",port="8080"} -12.2
///
/// # HELP app_current_active_users The current number of active users.
/// # TYPE app_current_active_users gauge
/// app_current_active_users{host="localhost",port="8080",service="service-1"} 20
///
/// # HELP app_errors The total number of errors.
/// # TYPE app_errors counter
/// app_errors{host="localhost",port="8080"} 1
///
/// # HELP app_http_requests_duration The duration of HTTP requests.
/// # TYPE app_http_requests_duration histogram
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.005"} 0
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.01"} 0
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.025" } 0
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.05"} 0
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.1"} 0
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.25"} 0
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="0.5"} 0
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="1"} 1
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="2.5"} 1
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="5"} 1
/// app_http_requests_duration_bucket{host="localhost",method="GET",path="/",port="8080",le="+Inf"} 1
/// app_http_requests_duration_sum{host="localhost",method="GET",path="/",port="8080"} 1
/// app_http_requests_duration_count{host="localhost",method="GET",path="/",port="8080"} 1
///
/// # HELP app_http_requests_total The total number of HTTP requests.
/// # TYPE app_http_requests_total counter
/// app_http_requests_total{host="localhost",method="GET",path="/",port="8080"} 2
/// app_http_requests_total{host="localhost",method="POST",path="/",port="8080"} 2
/// ```
/// # Static Metrics Example
///
/// When the `static` attribute is enabled, a static `LazyLock` is generated with a
/// SCREAMING_SNAKE_CASE name. The builder methods and `Default` implementation are made private,
/// ensuring the only way to access the metrics is through the static instance.
///
/// If `static` is enabled, `prometheus::default_registry()` is used.
///
/// ```rust
/// use prometric::{Counter, Gauge};
/// use prometric_derive::metrics;
///
/// #[metrics(scope = "app", static)]
/// struct AppMetrics {
///     /// The total number of requests.
///     #[metric(labels = ["method"])]
///     requests: Counter,
///
///     /// The current number of active connections.
///     #[metric]
///     active_connections: Gauge,
/// }
///
/// // Use the static directly anywhere
/// APP_METRICS.requests("GET").inc();
/// APP_METRICS.active_connections().set(10);
///
/// // The following would not compile:
/// // let metrics = AppMetrics::builder();  // Error: builder() is private
/// // let metrics = AppMetrics::default();   // Error: Default is not implemented
/// ```
///
/// # Exporting Metrics
/// An HTTP exporter is provided by [`prometric::exporter::ExporterBuilder`]. Usage:
///
/// ```rust
/// use prometric::exporter::ExporterBuilder;
///
/// // Metric definitions...
///
/// // Export the metrics on an HTTP endpoint in the background:
/// ExporterBuilder::new()
///     // Specify the address to listen on
///     .with_address("127.0.0.1:9090")
///     // Set the global namespace for the metrics (usually the name of the application)
///     .with_namespace("exporter")
///     // Install the exporter. This will start an HTTP server and serve metrics on the specified
///     // address.
///     .install()
///     .expect("Failed to install exporter");
/// ```
///
/// # Process Metrics Example
///
/// When the `process` feature is enabled, the `ProcessCollector` is used to collect metrics about
/// the current process.
///
/// ```rust
/// # #[cfg(feature = "process")] {
/// use prometric::process::ProcessCollector;
/// use prometric_derive::metrics;
///
/// let mut collector = ProcessCollector::default();
/// collector.collect();
/// # }
/// ```
///
/// #### Output
/// ```text
/// # HELP process_collection_duration_seconds The duration of the associated collection routine in seconds.
/// # TYPE process_collection_duration_seconds gauge
/// process_collection_duration_seconds 0.016130356
/// # HELP process_cpu_usage The CPU usage of the process as a percentage.
/// # TYPE process_cpu_usage gauge
/// process_cpu_usage 6.2536187171936035
/// # HELP process_disk_written_bytes_total The total written bytes to disk by the process.
/// # TYPE process_disk_written_bytes_total gauge
/// process_disk_written_bytes_total 0
/// # HELP process_max_fds The maximum number of open file descriptors of the process.
/// # TYPE process_max_fds gauge
/// process_max_fds 1048576
/// # HELP process_open_fds The number of open file descriptors of the process.
/// # TYPE process_open_fds gauge
/// process_open_fds 639
/// # HELP process_resident_memory_bytes The resident memory of the process in bytes. (RSS)
/// # TYPE process_resident_memory_bytes gauge
/// process_resident_memory_bytes 4702208
/// # HELP process_resident_memory_usage The resident memory usage of the process as a percentage of the total memory available.
/// # TYPE process_resident_memory_usage gauge
/// process_resident_memory_usage 0.00007072418111501723
/// # HELP process_start_time_seconds The start time of the process in UNIX seconds.
/// # TYPE process_start_time_seconds gauge
/// process_start_time_seconds 1763056609
/// # HELP process_thread_usage Per-thread CPU usage as a percentage of the process's CPU usage (Linux only).
/// # TYPE process_thread_usage gauge
/// process_thread_usage{name="process::tests:",pid="980490"} 0.9259260296821594
/// process_thread_usage{name="test-thread-1",pid="980491"} 0
/// process_thread_usage{name="test-thread-2",pid="980492"} 94.44445037841797
/// # HELP process_threads The number of OS threads used by the process (Linux only).
/// # TYPE process_threads gauge
/// process_threads 3
/// # HELP system_cpu_cores The number of logical CPU cores available in the system.
/// # TYPE system_cpu_cores gauge
/// system_cpu_cores 16
/// # HELP system_cpu_usage System-wide CPU usage percentage.
/// # TYPE system_cpu_usage gauge
/// system_cpu_usage 6.7168498039245605
/// # HELP system_max_cpu_frequency The maximum CPU frequency of all cores in MHz.
/// # TYPE system_max_cpu_frequency gauge
/// system_max_cpu_frequency 5339
/// # HELP system_memory_usage System-wide memory usage percentage.
/// # TYPE system_memory_usage gauge
/// system_memory_usage 6.398677876736871
/// # HELP system_min_cpu_frequency The minimum CPU frequency of all cores in MHz.
/// # TYPE system_min_cpu_frequency gauge
/// system_min_cpu_frequency 545
/// ```
#[proc_macro_attribute]
pub fn metrics(attr: TokenStream, item: TokenStream) -> TokenStream {
    // NOTE: We use `proc_macro_attribute` here because we're actually rewriting the struct. Derive
    // macros are additive.
    let mut input = parse_macro_input!(item as ItemStruct);

    let attributes: MetricsAttr = match syn::parse(attr) {
        Ok(v) => v,
        Err(e) => {
            return e.to_compile_error().into();
        }
    };

    expand::expand(attributes, &mut input).unwrap_or_else(|err| err.into_compile_error()).into()
}
