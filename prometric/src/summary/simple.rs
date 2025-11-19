//! Simple summary implementation
//!
//! Uses [`metrics_util::storage::Summary`] for the undelying representation

use metrics_util::storage::Summary as Inner;

use crate::summary::traits::{Summary, SummaryProvider};

/// A simple Summary metric implementation
#[derive(Debug, Clone)]
pub struct SimpleSummary {
    pub(crate) inner: Inner,
    // We track the sum separately because [`Inner`] doesn't expose it
    pub(crate) sum: f64,
}

/// Configuration for the Summary
///
/// See [`metrics_util::storage::Summary::new`] for documentation on the various options
#[derive(Clone, Default)]
pub struct SimpleSummaryOpts {
    pub alpha: f64,
    pub max_buckets: u32,
    pub min_value: f64,
}

impl SummaryProvider for SimpleSummary {
    type Opts = SimpleSummaryOpts;
    type Summary = Self;

    fn new(opts: &Self::Opts) -> Self {
        Self { inner: Inner::new(opts.alpha, opts.max_buckets, opts.min_value), sum: 0. }
    }

    fn observe(&mut self, val: f64) {
        self.inner.add(val);
    }

    fn snapshot(&self) -> Self::Summary {
        self.clone()
    }
}

impl Summary for SimpleSummary {
    fn sample_sum(&self) -> f64 {
        self.sum
    }

    fn sample_count(&self) -> u64 {
        self.inner.count() as u64
    }

    fn quantile(&self, quantile: f64) -> Option<f64> {
        self.inner.quantile(quantile)
    }
}
