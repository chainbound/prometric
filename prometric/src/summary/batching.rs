//! Summary with concurrent measurements (via batching)

use orx_concurrent_vec::ConcurrentVec;
use parking_lot::RwLock;

use crate::{
    arc_swap_cell::ArcCell,
    summary::traits::{ConcurrentSummaryProvider, SummaryProvider},
};

pub const DEFAULT_BATCH_SIZE: usize = 64;

/// Wraps over the given [`SummaryProvider`] `P` to batch measurements according to configured batch
/// size
///
/// This is useful to transform a [`SummaryProvider`] into a [`ConcurrentSummaryProvider`], with a
/// simple batching logic for improved lock accesses
#[derive(Debug)]
pub struct BatchedSummary<P> {
    // We use ArcCell to allow more measurements to be recorded while the batch is being committed
    measurements: ArcCell<ConcurrentVec<f64>>,
    batch_size: usize,

    inner: RwLock<P>,
}

impl<P: Clone> Clone for BatchedSummary<P> {
    fn clone(&self) -> Self {
        let measurements = self.measurements.clone();

        Self {
            measurements,
            batch_size: self.batch_size,
            inner: RwLock::new(self.inner.read().clone()),
        }
    }
}

/// The configuration for the [`BatchedSummary`]
#[derive(Clone)]
pub struct BatchOpts<O> {
    /// The number of measurements to batch before committing to the inner Summary
    pub batch_size: usize,
    pub inner: O,
}

impl<O> BatchOpts<O> {
    pub fn from_inner(inner: O) -> Self {
        Self { batch_size: DEFAULT_BATCH_SIZE, inner }
    }
}

impl<P: SummaryProvider> BatchedSummary<P> {
    /// Commits the current measurements batch to the underlying summary
    ///
    /// Will clear the measurements batch
    pub fn commit(&self) {
        // If [`ConcurrentVec`] had something like `.take()` the [`ArcCell`] would be unnecessary
        let measurements = self.measurements.swap(ConcurrentVec::new());

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
            measurements: ArcCell::new(ConcurrentVec::new()),
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
