use prometric::{Counter, Gauge, Histogram, Summary};
use prometric_derive::metrics;

// The `scope` attribute is used to set the prefix for the metric names in this struct.
#[metrics(scope = "app")]
struct AppMetrics {
    /// The total number of HTTP requests.
    #[metric(rename = "http_requests_total", labels = ["method", "path"])]
    http_requests: Counter,

    // For histograms, the `buckets` attribute is optional. It will default to [prometheus::DEFAULT_BUCKETS] if not provided.
    // `buckets` can also be an expression that evaluates into a `Vec<f64>`.
    /// The duration of HTTP requests.
    #[metric(labels = ["method", "path"], buckets = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])]
    http_requests_duration: Histogram,

    #[metric(labels = ["method", "path"], quantiles = [0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0])]
    http_request_sizes: Summary,

    /// This doc comment will be overwritten by the `help` attribute.
    #[metric(rename = "current_active_users", labels = ["service"], help = "The current number of active users.")]
    current_users: Gauge,

    /// The balance of the account, in dollars. Uses a floating point number.
    #[metric(rename = "account_balance", labels = ["account_id"])]
    account_balance: Gauge<f64>,

    /// The total number of errors.
    #[metric]
    errors: Counter,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Build the metrics struct with static labels, which will initialize and register the metrics with the default registry.
    // A custom registry can be used by passing it to the builder using `with_registry`.
    let metrics =
        AppMetrics::builder().with_label("host", "localhost").with_label("port", "8080").build();

    // Metric fields each get an accessor method generated, which can be used to interact with the metric.
    // The arguments to the accessor method are the labels for the metric.
    metrics.http_requests("GET", "/").inc();
    metrics.http_requests_duration("GET", "/").observe(1.0);
    metrics.http_request_sizes("GET", "/").observe(12345);
    metrics.current_users("service-1").set(10);
    metrics.account_balance("1234567890").set(-12.2);
    metrics.errors().inc();
}
