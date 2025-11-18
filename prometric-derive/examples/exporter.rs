use prometric::{Counter, Gauge, exporter::ExporterBuilder};
use prometric_derive::metrics;

#[metrics(scope = "example")]
struct ExampleMetrics {
    /// A simple counter
    #[metric]
    counter: Counter,

    /// A simple gauge
    #[metric]
    gauge: Gauge,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let metrics = ExampleMetrics::default();

    metrics.counter().inc();
    metrics.gauge().set(10);

    // Export the metrics on an HTTP endpoint in the background:
    ExporterBuilder::new()
        // Specify the address to listen on
        .with_address("127.0.0.1:9090")
        // Set the global namespace for the metrics (usually the name of the application)
        .with_namespace("exporter")
        // Install the exporter. This will start an HTTP server and serve metrics on the specified
        // address.
        .install()
        .expect("Failed to install exporter");
}
