//! Summary with concurrent measurements (via batching)

use std::sync::Arc;

use arc_cell::ArcCell;
use parking_lot::RwLock;

use crate::summary::traits::{ConcurrentSummaryProvider, SummaryProvider};

pub const DEFAULT_BATCH_SIZE: usize = 64;

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

    pub fn with_batch_size(self, batch_size: usize) -> Self {
        Self { batch_size, ..self }
    }
}

// TODO: switch to FixedVec
// NOTE: ConcurrentVec doesn't currently implement `Clone` over _all_ possible `P`, but only on the
// default one
type Batch<T> = orx_concurrent_vec::ConcurrentVec<
    T,
    orx_concurrent_vec::SplitVec<
        orx_concurrent_vec::ConcurrentElement<T>,
        orx_concurrent_vec::Doubling,
    >,
>;

/// Wraps over the given [`SummaryProvider`] `P` to batch measurements according to configured batch
/// size
///
/// This is useful to transform a [`SummaryProvider`] into a [`ConcurrentSummaryProvider`], with a
/// simple batching logic for improved lock accesses
#[derive(Debug)]
pub struct BatchedSummary<P> {
    // We use ArcCell to allow more measurements to be recorded while the batch is being committed
    measurements: ArcCell<Batch<f64>>,
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

impl<P: SummaryProvider> BatchedSummary<P> {
    fn new_batch(batch_size: usize) -> Arc<Batch<f64>> {
        // We will always have at most `batch_size` measurements before committing, so let's
        // preallocate enough capacity

        // NOTE: We should also overallocate to have some overhead if
        // some measurements are added before the commit operation takes ownership of the
        // current batch

        // NOTE: `SplitVec` can't be initialized with a requested total capacity directly
        let mut batch = Batch::new();
        batch.reserve_maximum_capacity(batch_size);

        Arc::new(batch)
    }

    /// Wait for the given Arc to have a single owner and obtain the inner value
    pub(crate) fn wait_for_arc<T>(mut arc: Arc<T>) -> T {
        loop {
            match Arc::try_unwrap(arc) {
                Ok(inner) => return inner,
                Err(this) => {
                    arc = this;
                }
            }

            std::hint::spin_loop();
        }
    }

    /// Commits the current measurements batch to the underlying summary
    ///
    /// Will clear current the measurements batch
    pub fn commit(&self) {
        // If [`FixedBatch`] had something like `.take()` the [`ArcCell`] would be unnecessary
        // NOTE: we take the previous batch so new measurements can be added without changing
        // the set that we are currently committing
        let measurements = self.measurements.set(Self::new_batch(self.batch_size));
        let measurements = Self::wait_for_arc(measurements);

        let mut inner = self.inner.write();

        for measure in measurements.into_iter() {
            inner.observe(measure);
        }
    }

    /// Retrieve the inner summary
    ///
    /// Will commit the current batch before returning the summary
    pub fn into_inner(self) -> P {
        self.commit();
        self.inner.into_inner()
    }
}

impl<P: SummaryProvider> SummaryProvider for BatchedSummary<P> {
    type Opts = BatchOpts<P::Opts>;
    type Summary = P::Summary;

    fn new(opts: &Self::Opts) -> Self {
        let inner = RwLock::new(P::new(&opts.inner));
        Self {
            inner,
            measurements: ArcCell::new(Self::new_batch(opts.batch_size)),
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
        let measurements = self.measurements.get();
        measurements.push(val);

        if measurements.len() >= self.batch_size {
            // forcefully drop the guard before committing
            // to avoid deadlocks
            std::mem::drop(measurements);

            // Commit the current batch
            self.commit()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        simple::{SimpleSummary, SimpleSummaryOpts},
        traits::Summary,
    };

    use super::*;

    #[tokio::test]
    async fn concurrent_observe() {
        // TODO: Consider converting into quickcheck test
        // parametrized  by: batch size, number of measurements and concurrent tasks
        let batch_size = DEFAULT_BATCH_SIZE;

        let opts = SimpleSummaryOpts::default();
        let opts = BatchOpts::from_inner(opts).with_batch_size(batch_size);

        let summary = BatchedSummary::<SimpleSummary>::new(&opts);
        let summary = Arc::new(summary);

        let tasks = 8;
        let measurements = 50_000;

        let mut handles = Vec::with_capacity(tasks);
        for _ in 0..tasks {
            let summary = summary.clone();
            let task = tokio::task::spawn_blocking(move || {
                for i in 0..measurements {
                    summary.concurrent_observe(i as f64)
                }
            });
            handles.push(task);
        }

        for h in handles {
            h.await.expect("no task panics");
        }

        let result = summary.snapshot();
        assert_eq!(
            result.sample_count(),
            tasks as u64 * measurements,
            "Should have all measurements present in the collection"
        );
    }
}
