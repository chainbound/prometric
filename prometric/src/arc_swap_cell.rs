//! An [`arc_swap::ArcSwap`]-based cell
//!
//! The purpose of [`ArcCell`] is to allow an [`std::sync::Arc`] to be used like cell, reading
//! what's held inside atomically but also allowing atomic swap operations. The [`ArcCell`] is NOT a
//! way to have multiple references to the same data, as [`arc_swap::ArcSwap`] paired with a normal
//! [`std::sync::Arc`] is already sufficient for that purpose.

use std::{ops::Deref, sync::Arc};

use arc_swap::{
    ArcSwapAny, Guard as InnerGuard,
    strategy::{DefaultStrategy, Strategy},
};

/// An utility to use an [ `Arc` ] similarly to a cell
///
/// # Invariants
/// The inner [`Arc`] is inaccessible outside this cell, therefore the strong count shall ALWAYS be
/// equal or less than 1.
pub struct ArcCell<T, S: Strategy<Arc<T>> = DefaultStrategy>(ArcSwapAny<Arc<T>, S>);

impl<T: std::fmt::Debug, S: Strategy<Arc<T>>> std::fmt::Debug for ArcCell<T, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ArcCell").field(&self.0).finish()
    }
}

impl<T, S: Strategy<Arc<T>> + Default> ArcCell<T, S> {
    /// Construct a new [`ArcCell`]
    pub fn new(data: T) -> Self {
        Self::with_strategy(data, Default::default())
    }
}

impl<T, S: Strategy<Arc<T>>> ArcCell<T, S> {
    /// Construct a new [`ArcCell`] with the given protection strategy
    pub fn with_strategy(data: T, strategy: S) -> Self {
        Self(ArcSwapAny::with_strategy(Arc::new(data), strategy))
    }

    /// Provides a temporary borrow of the object inside.
    ///
    /// Behaves the same as [`InnerGuard`], except it doesn't expose the inner [`Arc`] to avoid
    /// cheap clones
    pub fn load(&self) -> Guard<T, S> {
        Guard(self.0.load())
    }

    /// Exchanges the value inside this instance.
    pub fn swap(&self, new: T) -> T {
        let arc = self.0.swap(Arc::new(new));

        Arc::into_inner(arc).expect("ArcCell's inner Arc shouldn't have been cloned anywhere")
    }
}

impl<T: Clone, S: Strategy<Arc<T>> + Default> Clone for ArcCell<T, S> {
    /// This is the core of the [`ArcCell`] invariant: no clones of the underlying Arc
    fn clone(&self) -> Self {
        let inner = self.0.load();
        // we enforce that T is being cloned and not the Arc holding it
        let cloned = T::clone(&inner);

        Self::new(cloned)
    }
}

/// Guard for [`ArcCell`]
///
/// Enforces the [`ArcCell`] invariants by ensuring the inner [`Arc`] is not accessible
pub struct Guard<T, S: Strategy<Arc<T>>>(InnerGuard<Arc<T>, S>);

impl<T, S: Strategy<Arc<T>>> Deref for Guard<T, S> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.deref().deref()
    }
}
