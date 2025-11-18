use prometric::{Counter, Gauge};
use prometric_derive::metrics;

#[metrics(scope = "app", static)]
struct AppMetrics {
    /// The total number of requests.
    #[metric(labels = ["method"])]
    requests: Counter,

    /// The current number of active connections.
    #[metric]
    active_connections: Gauge,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Use the static directly (the name is APP_METRICS in SCREAMING_SNAKE_CASE)
    APP_METRICS.requests("GET").inc();
    APP_METRICS.active_connections().set(10);

    // The following would not compile:
    // let metrics = AppMetrics::builder();  // Error: builder() is private
    // let metrics = AppMetrics::default();   // Error: Default is not implemented
}
