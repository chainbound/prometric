use std::{net::SocketAddr, thread, time::Duration};

use hyper::{
    Request, Response, body::Incoming, header::CONTENT_TYPE, server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use prometheus::{Encoder, TextEncoder};

/// A builder for the Prometheus HTTP exporter.
pub struct ExporterBuilder {
    registry: Option<prometheus::Registry>,
    address: String,
    path: String,
    global_prefix: Option<String>,
    process_metrics_poll_interval: Option<Duration>,
}

impl Default for ExporterBuilder {
    fn default() -> Self {
        Self {
            registry: None,
            address: "0.0.0.0:9090".to_owned(),
            path: "/metrics".to_owned(),
            global_prefix: None,
            process_metrics_poll_interval: None,
        }
    }
}

impl ExporterBuilder {
    /// Create a new exporter with the default registry and socket address.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the socket address for the exporter.
    ///
    /// # Panics
    /// Panics if the socket address is malformed, i.e. if [`str::parse`] into a [`SocketAddr`]
    /// returns an error.
    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        let address = address.into();
        self.address = address;
        self
    }

    /// Set the path for the exporter.
    ///
    /// If no path is provided, the default path is `/`.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        self.path = path;
        self
    }

    /// Set the global namespace for the metrics in the associated registry. This will be prepended
    /// to all metric names.
    pub fn with_namespace(mut self, global_prefix: impl Into<String>) -> Self {
        let global_prefix = global_prefix.into();
        self.global_prefix = Some(global_prefix);
        self
    }

    /// Set the registry for the exporter.
    pub fn with_registry(mut self, registry: prometheus::Registry) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Also collect process metrics, polling at the given interval in the background.
    ///
    /// A 10 second interval is a good default for most applications.
    #[cfg(feature = "process")]
    pub fn with_process_metrics(mut self, poll_interval: Duration) -> Self {
        self.process_metrics_poll_interval = Some(poll_interval);
        self
    }

    fn path(&self) -> Result<String, ExporterError> {
        if self.path.is_empty() {
            return Err(ExporterError::InvalidPath(self.path.clone()));
        }

        if !self.path.starts_with('/') {
            return Err(ExporterError::InvalidPath(self.path.clone()));
        }

        // Remove trailing slash from path
        let path = if self.path.eq("/") {
            "/".to_owned()
        } else {
            self.path.trim_end_matches('/').to_owned()
        };

        Ok(path)
    }

    fn address(&self) -> Result<SocketAddr, ExporterError> {
        self.address.parse().map_err(|e| ExporterError::InvalidAddress(self.address.clone(), e))
    }

    /// Install the HTTP exporter with the given configuration and start serving metrics.
    /// Uses [hyper] for the HTTP server and [tokio] for the runtime.
    ///
    /// # Behavior
    /// - If a Tokio runtime is available, use it to spawn the listener.
    /// - Otherwise, spawn a new single-threaded Tokio runtime on a thread, and spawn the listener
    ///   there.
    pub fn install(self) -> Result<(), ExporterError> {
        let path = self.path()?;
        let address = self.address()?;
        let registry = self.registry.unwrap_or_else(|| prometheus::default_registry().clone());

        // Build the serve and process collection futures.
        let serve = serve(address, registry, path, self.global_prefix);
        let collect = collect_process_metrics(self.process_metrics_poll_interval);
        let fut = async { tokio::try_join!(serve, collect) };

        // If a Tokio runtime is available, use it to spawn the listener. Otherwise,
        // create a new single-threaded runtime and spawn the listener there.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(fut);
        } else {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;

            thread::spawn(move || {
                runtime.block_on(fut).unwrap_or_else(|e| panic!("server error: {e:?}"));
            });
        }

        Ok(())
    }
}

async fn serve(
    addr: SocketAddr,
    registry: prometheus::Registry,
    path: String,
    global_prefix: Option<String>,
) -> Result<(), ExporterError> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        let registry = registry.clone();
        let path = path.clone();
        let global_prefix = global_prefix.clone();

        let service = service_fn(move |req| {
            serve_req(req, registry.clone(), path.clone(), global_prefix.clone())
        });

        tokio::spawn(async move {
            let _ = http1::Builder::new().serve_connection(io, service).await;
        });
    }
}

async fn serve_req(
    req: Request<Incoming>,
    registry: prometheus::Registry,
    path: String,
    global_prefix: Option<String>,
) -> Result<Response<String>, Box<dyn std::error::Error + Send + Sync>> {
    let encoder = TextEncoder::new();
    let mut metrics = registry.gather();

    if req.uri().path() != path {
        return Ok(Response::builder().status(404).body("Not Found".to_string())?);
    }

    // Set the global prefix for the metrics
    if let Some(prefix) = global_prefix {
        metrics.iter_mut().for_each(|metric| {
            if let Some(name) = metric.name.as_mut() {
                name.insert(0, '_');
                name.insert_str(0, &prefix);
            };
        });
    }

    let body = encoder.encode_to_string(&metrics)?;

    let response =
        Response::builder().status(200).header(CONTENT_TYPE, encoder.format_type()).body(body)?;

    Ok(response)
}

/// If the "process" feature is enabled AND the poll interval is provided, collect
/// process metrics at the given interval. Otherwise, no-op.
///
/// NOTE: the return type is Result to use [`tokio::try_join!`] with [`serve`].
async fn collect_process_metrics(_poll_interval: Option<Duration>) -> Result<(), ExporterError> {
    #[cfg(feature = "process")]
    if let Some(interval) = _poll_interval {
        let mut collector = crate::process::ProcessCollector::default();
        loop {
            collector.collect();
            tokio::time::sleep(interval).await;
        }
    }

    Ok(())
}

/// An error that can occur when building or installing the Prometheus HTTP exporter.
pub enum ExporterError {
    BindError(std::io::Error),
    ServeError(hyper::Error),
    InvalidPath(String),
    InvalidAddress(String, std::net::AddrParseError),
}

impl std::error::Error for ExporterError {}

impl std::fmt::Display for ExporterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BindError(e) => write!(f, "Failed to bind to address: {e:?}"),
            Self::ServeError(e) => write!(f, "HTTP server failed: {e:?}"),
            Self::InvalidPath(path) => write!(f, "Invalid path: {path}"),
            Self::InvalidAddress(address, e) => write!(f, "Invalid address: {address}: {e:?}"),
        }
    }
}

impl From<std::io::Error> for ExporterError {
    fn from(e: std::io::Error) -> Self {
        Self::BindError(e)
    }
}

impl std::fmt::Debug for ExporterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}
