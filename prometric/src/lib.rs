//! This library contains the core supported metric types. They are all wrappers around the
//! Prometheus core types. These types are primarily used for *defining* metrics, and not for
//! *using* them. The actual usage of metrics is done through the generated structs from the
//! `prometric-derive` crate.
//! - [`Counter`]: A counter metric.
//! - [`Gauge`]: A gauge metric.
//! - [`Histogram`]: A histogram metric.

use std::collections::HashMap;

#[cfg(feature = "exporter")]
pub mod exporter;

#[cfg(feature = "process")]
pub mod process;

/// Sealed trait to prevent outside code from implementing the metric types.
mod private {
    pub trait Sealed {}

    impl Sealed for u64 {}
    impl Sealed for i64 {}
    impl Sealed for f64 {}
    impl Sealed for i32 {}
    impl Sealed for u32 {}
    impl Sealed for usize {}
    impl Sealed for f32 {}
}

/// Internal conversion trait to allow ergonomic value passing (e.g., `u32`, `usize`).
/// This enables library users to call methods like `.set(queue.len())` without manual casts.
pub trait IntoAtomic<T>: private::Sealed {
    fn into_atomic(self) -> T;
}

impl<T: private::Sealed> IntoAtomic<T> for T {
    #[inline]
    fn into_atomic(self) -> T {
        self
    }
}

/// Macro to implement `IntoAtomic<Out>` for a type `In`.
macro_rules! impl_into_atomic {
    ($in_ty:ty => $out_ty:ty) => {
        impl $crate::IntoAtomic<$out_ty> for $in_ty {
            #[inline]
            fn into_atomic(self) -> $out_ty {
                self as $out_ty
            }
        }
    };
}

// auto casts to u64
impl_into_atomic!(i32 => u64);
impl_into_atomic!(u32 => u64);
impl_into_atomic!(usize => u64);

// auto casts to i64
impl_into_atomic!(i32 => i64);
impl_into_atomic!(u32 => i64);
impl_into_atomic!(usize => i64);

// auto casts to f64
impl_into_atomic!(i32 => f64);
impl_into_atomic!(u32 => f64);
impl_into_atomic!(usize => f64);
impl_into_atomic!(f32 => f64);

/// The default number type for counters.
pub type CounterDefault = u64;

/// The default number type for gauges.
pub type GaugeDefault = u64;

/// A marker trait for numbers that can be used as counter values.
/// Supported types: `u64`, `f64`
pub trait CounterNumber: Sized + 'static + private::Sealed {
    /// The atomic type associated with this number type.
    type Atomic: prometheus::core::Atomic;
}

impl CounterNumber for u64 {
    type Atomic = prometheus::core::AtomicU64;
}

impl CounterNumber for f64 {
    type Atomic = prometheus::core::AtomicF64;
}

/// A marker trait for numbers that can be used as gauge values.
/// Supported types: `i64`, `f64`, `u64`
pub trait GaugeNumber: Sized + 'static + private::Sealed {
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
    pub fn new(
        registry: &prometheus::Registry,
        name: &str,
        help: &str,
        labels: &[&str],
        const_labels: HashMap<String, String>,
        buckets: Option<Vec<f64>>,
    ) -> Self {
        let buckets = buckets.unwrap_or(prometheus::DEFAULT_BUCKETS.to_vec());
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
