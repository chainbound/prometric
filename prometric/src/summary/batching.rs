//! Summary with concurrent measurements (via batching)

use std::sync::Arc;

use arc_swap::ArcSwap;
use orx_concurrent_vec::ConcurrentVec;
use parking_lot::RwLock;

use crate::summary::traits::{ConcurrentSummaryProvider, SummaryProvider};

pub const DEFAULT_BATCH_SIZE: usize = 64;

/// Wraps over the given [`SummaryProvider`] `P` to batch measurements according to configured batch
/// size
///
/// This is useful to transform a [`SummaryProvider`] into a [`ConcurrentSummaryProvider`], with a simple batching logic
/// for improved lock accesses
#[derive(Debug)]
pub struct BatchedSummary<P> {
    // This in an ArcSwap so the underlying storage can be atomically retrieved
    // allowing more measurements to be recorded while the batch is being commited
    measurements: ArcSwap<ConcurrentVec<f64>>,
    batch_size: usize,

    inner: RwLock<P>,
}

impl<P: Clone> Clone for BatchedSummary<P> {
    fn clone(&self) -> Self {
        // NOTE: MUST not do a cheap clone of the Arc
        // To avoid polluting measurements from other references
        let measurements = ArcSwap::new(self.measurements.load().clone());

        Self {
            measurements,
            inner: RwLock::new(self.inner.read().clone()),
            batch_size: self.batch_size,
        }
    }
}

/// The configuration for the [`BatchedSummary`]
#[derive(Clone)]
pub struct BatchOps<O> {
    /// The number of measurements to batch before committing to the inner Summary
    pub batch_size: usize,
    pub inner: O,
}

impl<O> BatchOps<O> {
    pub fn from_inner(inner: O) -> Self {
        Self { batch_size: DEFAULT_BATCH_SIZE, inner }
    }
}

impl<P: SummaryProvider> BatchedSummary<P> {
    /// Commits the current measurements batch to the underlying summary
    ///
    /// Will clear the measurements batch
    pub fn commit(&self) {
        // If [`ConcurrentVec`] had something like `.take()` the ArcSwap would be unnecessary
        let measurements = self.measurements.swap(Arc::new(ConcurrentVec::new()));
        let measurements = Arc::into_inner(measurements)
            // NOTE: see `Clone` impl above
            .expect("Measurements shouldn't have been cloned anywhere");

        let mut inner = self.inner.write();

        for measure in measurements.into_iter() {
            inner.observe(measure);
        }
    }
}

impl<P: SummaryProvider> SummaryProvider for BatchedSummary<P> {
    type Opts = BatchOps<P::Opts>;
    type Summary = P::Summary;

    fn new(opts: &Self::Opts) -> Self {
        let inner = RwLock::new(P::new(&opts.inner));
        Self {
            inner,
            measurements: ArcSwap::from_pointee(ConcurrentVec::new()),
            batch_size: opts.batch_size,
        }
    }

    fn observe(&mut self, val: f64) {
        self.concurrent_observe(val);
    }

    fn snapshot(&self) -> Self::Summary {
        // Forcefully commit the current batch before snapshotting
        self.commit();
        self.inner.read().snapshot()
    }
}

impl<P: SummaryProvider> ConcurrentSummaryProvider for BatchedSummary<P> {
    fn concurrent_observe(&self, val: f64) {
        let measurements = self.measurements.load();
        measurements.push(val);

        if measurements.len() >= self.batch_size {
            // Commit the current batch
            self.commit()
        }
    }
}
