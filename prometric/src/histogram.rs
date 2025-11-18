use std::collections::HashMap;

/// A histogram metric.
#[derive(Debug)]
pub struct Histogram {
    inner: prometheus::HistogramVec,
}

impl Clone for Histogram {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl Histogram {
    pub fn new<B: Into<Vec<f64>>>(
        registry: &prometheus::Registry,
        name: &str,
        help: &str,
        labels: &[&str],
        const_labels: HashMap<String, String>,
        buckets: Option<B>,
    ) -> Self {
        let buckets = buckets.map(Into::into).unwrap_or(prometheus::DEFAULT_BUCKETS.to_vec());
        let opts =
            prometheus::HistogramOpts::new(name, help).const_labels(const_labels).buckets(buckets);
        let metric = prometheus::HistogramVec::new(opts, labels).unwrap();

        let boxed = Box::new(metric.clone());
        if let Err(e) = registry.register(boxed.clone()) {
            let id = format!("{}, Labels: {}", name, labels.join(", "),);
            // If the metric is already registered, overwrite it.
            if matches!(e, prometheus::Error::AlreadyReg) {
                registry
                    .unregister(boxed.clone())
                    .unwrap_or_else(|_| panic!("Failed to unregister metric {id}"));

                registry
                    .register(boxed)
                    .unwrap_or_else(|_| panic!("Failed to overwrite metric {id}"));
            } else {
                panic!("Failed to register metric {id}");
            }
        }

        Self { inner: metric }
    }

    pub fn observe(&self, labels: &[&str], value: f64) {
        self.inner.with_label_values(labels).observe(value);
    }
}
