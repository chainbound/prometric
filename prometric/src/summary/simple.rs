//! Simple summary implementation
//!
//! Uses [`metrics_util::storage::Summary`] for the undelying representation

use metrics_util::storage::Summary as Inner;

use crate::summary::traits::{NonConcurrentSummaryProvider, Summary};

/// A simple Summary metric implementation
///
/// This Summary uses [`Inner`] for the underlying computation, which stores the measurements and
/// provides arbitrary quantiles over the observed measurements
#[derive(Debug, Clone)]
pub struct SimpleSummary {
    pub(crate) inner: Inner,
    // We track the sum separately because [`Inner`] doesn't expose it
    pub(crate) sum: f64,
}

/// Configuration for the Summary
///
/// See [`metrics_util::storage::Summary::new`] for documentation on the various options
#[derive(Clone)]
pub struct SimpleSummaryOpts {
    pub alpha: f64,
    pub max_buckets: u32,
    pub min_value: f64,
}

impl Default for SimpleSummaryOpts {
    fn default() -> Self {
        // takes from Inner::with_defaults
        Self { alpha: 0.0001, max_buckets: 32_768, min_value: 1.0e-9 }
    }
}

impl NonConcurrentSummaryProvider for SimpleSummary {
    type Opts = SimpleSummaryOpts;
    type Summary = Self;

    fn new_provider(opts: &Self::Opts) -> Self {
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
