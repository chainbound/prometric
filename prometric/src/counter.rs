use std::collections::HashMap;

use crate::private::Sealed;

/// The default number type for counters.
pub type CounterDefault = u64;

/// A marker trait for numbers that can be used as counter values.
/// Supported types: `u64`, `f64`
pub trait CounterNumber: Sized + 'static + Sealed {
    /// The atomic type associated with this number type.
    type Atomic: prometheus::core::Atomic;
}

impl CounterNumber for u64 {
    type Atomic = prometheus::core::AtomicU64;
}

impl CounterNumber for f64 {
    type Atomic = prometheus::core::AtomicF64;
}

/// A counter metric with a generic number type. Default is `u64`, which provides better performance
/// for natural numbers.
#[derive(Debug)]
pub struct Counter<N: CounterNumber = CounterDefault> {
    inner: prometheus::core::GenericCounterVec<N::Atomic>,
}

impl<N: CounterNumber> Clone for Counter<N> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<N: CounterNumber> Counter<N> {
    /// Create a new counter metric with the given registry, name, help, labels, and const labels.
    pub fn new(
        registry: &prometheus::Registry,
        name: &str,
        help: &str,
        labels: &[&str],
        const_labels: HashMap<String, String>,
    ) -> Self {
        let opts = prometheus::Opts::new(name, help).const_labels(const_labels);
        let metric = prometheus::core::GenericCounterVec::<N::Atomic>::new(opts, labels).unwrap();

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

    pub fn inc_by(&self, labels: &[&str], value: <N::Atomic as prometheus::core::Atomic>::T) {
        self.inner.with_label_values(labels).inc_by(value);
    }

    pub fn reset(&self, labels: &[&str]) {
        self.inner.with_label_values(labels).reset();
    }
}
