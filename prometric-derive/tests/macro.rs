use std::time::Duration;

use prometheus::Encoder as _;
use prometric::{Counter, Gauge, Histogram};

/// This is a struct that contains the metrics for the application.
///
/// # Explanation
///
/// - Deriving `PrometheusMetrics` will generate the metrics for the struct.
/// - #[metrics(prefix = "app", static_labels = [("host", "localhost"), ("port", "8080")])]
/// is used to set the prefix and static labels for the metrics.
///
/// - Doc comments on the fields are used to generate the documentation for the metric.
/// - #[metric] attribute defines the metric name, and labels, and potentially other options for
///   that metric type (like buckets)
/// - The type of the field is used to determine the metric type.
/// - Deriving `Default` will generate a default instance of the struct with the metrics initialized
///   and described. Counters and gauges
/// will be initialized to 0.
#[prometric_derive::metrics(scope = "app")]
struct AppMetrics {
    /// The total number of HTTP requests.
    #[metric(rename = "http_requests_total", labels = ["method", "path"])]
    http_requests: prometric::Counter,

    /// The duration of HTTP requests.
    #[metric(labels = ["method", "path"], buckets = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])]
    http_requests_duration: prometric::Histogram,

    /// This doc comment will be overwritten by the `help` attribute.
    #[metric(rename = "current_active_users", labels = ["service"], help = "The current number of active users.")]
    current_users: prometric::Gauge<u64>,

    #[metric(rename = "account_balance", labels = ["account_id"])]
    /// The balance of the account, in dollars. Uses a floating point number.
    account_balance: prometric::Gauge<f64>,

    /// The total number of errors.
    #[metric]
    errors: prometric::Counter,
}

#[test]
fn test_macro() {
    // Register with default registry, no static labels
    // let app_metrics = AppMetrics::default();

    // OR use a custom registry, static labels with builder-style API
    let registry = prometheus::default_registry();
    let app_metrics = AppMetrics::builder()
        .with_registry(registry)
        .with_label("host", "localhost") // These define the static labels for these metrics
        .with_label("port", "8080")
        .build(); // Build the metrics instance

    app_metrics.http_requests("GET", "/").inc();

    // Increment all GET requests by 1
    app_metrics.http_requests("GET", "/").inc();

    // Increment all POST requests by 2
    app_metrics.http_requests("POST", "/").inc_by(2);

    // Set the current number of active users for service-1 to 10
    app_metrics.current_users("service-1").set(10);
    // Set the current number of active users to 20
    app_metrics.current_users("service-1").set(20);

    let duration = Duration::from_secs(1);
    app_metrics.http_requests_duration("GET", "/").observe(duration.as_secs_f64());

    app_metrics.account_balance("1234567890").set(-12.2);
    app_metrics.errors().inc();

    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather(); // Wait, need to expose registry

    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    let output = String::from_utf8(buffer).unwrap();
    println!("\n=== Prometheus Metrics Output ===\n{output}");

    assert!(output.contains("app_current_active_users"));
    assert!(output.contains("app_http_requests_duration"));
    assert!(output.contains("app_http_requests_total"));
    assert!(output.contains("app_errors"));
    assert!(output.contains("app_account_balance"));
    assert!(output.contains("The current number of active users."));
}

#[test]
fn test_autocasts() {
    let registry = prometheus::Registry::new();
    let app_metrics =
        AppMetrics::builder().with_registry(&registry).with_label("host", "localhost").build();

    // counter
    app_metrics.http_requests("GET", "/").inc_by(3); // auto: i32
    app_metrics.http_requests("GET", "/").inc_by(3u32);
    app_metrics.http_requests("GET", "/").inc_by(3i32);
    app_metrics.http_requests("GET", "/").inc_by(3u64);
    app_metrics.http_requests("GET", "/").inc_by(3usize);

    // gauge
    app_metrics.current_users("service-1").set(3); // auto: i32
    app_metrics.current_users("service-1").set(3u32);
    app_metrics.current_users("service-1").set(3i32);
    app_metrics.current_users("service-1").set(3usize);

    // hist
    app_metrics.http_requests_duration("GET", "/").observe(3); // auto: i32 
    app_metrics.http_requests_duration("GET", "/").observe(3u32);
    app_metrics.http_requests_duration("GET", "/").observe(3i32);
    app_metrics.http_requests_duration("GET", "/").observe(3f32);
    app_metrics.http_requests_duration("GET", "/").observe(3f64);
    app_metrics.http_requests_duration("GET", "/").observe(3usize);
}

#[test]
fn test_double_registration_success() {
    let registry = prometheus::Registry::new();
    AppMetrics::builder().with_registry(&registry).with_label("host", "localhost").build();

    AppMetrics::builder().with_registry(&registry).with_label("host", "0.0.0.0").build();
}

#[prometric_derive::metrics(scope = "test", static)]
struct TestMetrics {
    /// Test counter metric.
    #[metric(labels = ["label1"])]
    test_counter: prometric::Counter,

    /// Test gauge metric.
    #[metric]
    test_gauge: prometric::Gauge,
}

#[test]
fn test_static() {
    // Verify that the static TEST_METRICS is generated and accessible
    // The static name should be TEST_METRICS (SCREAMING_SNAKE_CASE)

    // Use the static directly (statics are module-level, not associated items)
    TEST_METRICS.test_counter("value1").inc();
    TEST_METRICS.test_gauge().set(42);

    // Verify it works by checking the registry
    let registry = prometheus::default_registry();
    let metric_families = registry.gather();

    let encoder = prometheus::TextEncoder::new();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    let output = String::from_utf8(buffer).unwrap();

    // The static should register metrics with the default registry
    assert!(output.contains("test_test_counter"));
    assert!(output.contains("test_test_gauge"));

    // Verify we can increment again
    TEST_METRICS.test_counter("value1").inc();
    TEST_METRICS.test_gauge().inc();
}

#[test]
fn bucket_expressions_work() {
    const BUCKETS: &[f64] = &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0];
    fn buckets() -> &'static [f64] {
        BUCKETS
    }

    #[prometric_derive::metrics(scope = "test")]
    struct BucketMetrics {
        /// Test histogram metric with bucket expression.
        #[metric(buckets = buckets())]
        hist: prometric::Histogram,
    }

    let registry = prometheus::default_registry();
    let app_metrics = BucketMetrics::builder().with_registry(registry).build();

    let duration = Duration::from_secs(1);
    app_metrics.hist().observe(duration.as_secs_f64());

    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather(); // Wait, need to expose registry

    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    let output = String::from_utf8(buffer).unwrap();

    assert!(output.contains("test_hist"));
}

#[test]
fn bucket_defaults_work() {
    #[prometric_derive::metrics(scope = "test")]
    struct BucketMetrics {
        /// Test histogram metric with bucket expression.
        #[metric]
        hist: prometric::Histogram,
    }

    let registry = prometheus::default_registry();
    let app_metrics = BucketMetrics::builder().with_registry(registry).build();

    let duration = Duration::from_secs(1);
    app_metrics.hist().observe(duration.as_secs_f64());

    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather(); // Wait, need to expose registry

    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    let output = String::from_utf8(buffer).unwrap();

    assert!(output.contains("test_hist"));
}
