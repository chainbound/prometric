//! This library contains the core supported metric types. They are all wrappers around the
//! Prometheus core types. These types are primarily used for *defining* metrics, and not for
//! *using* them. The actual usage of metrics is done through the generated structs from the
//! `prometric-derive` crate.
//! - [`counter::Counter`]: A counter metric.
//! - [`guage::Gauge`]: A gauge metric.
//! - [`histogram::Histogram`]: A histogram metric.

#[cfg(feature = "exporter")]
pub mod exporter;

#[cfg(feature = "process")]
pub mod process;

pub mod counter;
pub use counter::*;

pub mod gauge;
pub use gauge::*;

pub mod histogram;
pub use histogram::*;

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
