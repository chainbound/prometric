use std::collections::HashMap;

use crate::private::Sealed;

/// The default number type for gauges.
pub type GaugeDefault = u64;

/// A marker trait for numbers that can be used as gauge values.
/// Supported types: `i64`, `f64`, `u64`
pub trait GaugeNumber: Sized + 'static + Sealed {
    /// The atomic type associated with this number type.
    type Atomic: prometheus::core::Atomic;
}

impl GaugeNumber for i64 {
    type Atomic = prometheus::core::AtomicI64;
}

impl GaugeNumber for f64 {
    type Atomic = prometheus::core::AtomicF64;
}

impl GaugeNumber for u64 {
    type Atomic = prometheus::core::AtomicU64;
}

/// A gauge metric with a generic number type. Default is `i64`, which provides better performance
/// for integers.
#[derive(Debug)]
pub struct Gauge<N: GaugeNumber = GaugeDefault> {
    inner: prometheus::core::GenericGaugeVec<N::Atomic>,
}

impl<N: GaugeNumber> Clone for Gauge<N> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<N: GaugeNumber> Gauge<N> {
    /// Create a new gauge metric with the given registry, name, help, labels, and const labels.
    pub fn new(
        registry: &prometheus::Registry,
        name: &str,
        help: &str,
        labels: &[&str],
        const_labels: HashMap<String, String>,
    ) -> Self {
        let opts = prometheus::Opts::new(name, help).const_labels(const_labels);
        let metric = prometheus::core::GenericGaugeVec::<N::Atomic>::new(opts, labels).unwrap();

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

    pub fn inc(&self, labels: &[&str]) {
        self.inner.with_label_values(labels).inc();
    }

    pub fn dec(&self, labels: &[&str]) {
        self.inner.with_label_values(labels).dec();
    }

    pub fn add(&self, labels: &[&str], value: <N::Atomic as prometheus::core::Atomic>::T) {
        self.inner.with_label_values(labels).add(value);
    }

    pub fn sub(&self, labels: &[&str], value: <N::Atomic as prometheus::core::Atomic>::T) {
        self.inner.with_label_values(labels).sub(value);
    }

    pub fn set(&self, labels: &[&str], value: <N::Atomic as prometheus::core::Atomic>::T) {
        self.inner.with_label_values(labels).set(value);
    }
}
