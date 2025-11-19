use metrics_util::storage::Summary as Inner;

use crate::summary::traits::{Summary, SummaryProvider};

#[derive(Debug, Clone)]
pub struct SimpleSummary {
    pub(crate) inner: Inner,
    // We track the sum separately because [`Inner`] doesn't expose it
    pub(crate) sum: f64,
}

#[derive(Clone, Default)]
pub struct SimpleSummaryOpts {}

impl SummaryProvider for SimpleSummary {
    type Opts = SimpleSummaryOpts;
    type Summary = Self;

    fn new(_: &Self::Opts) -> Self {
        Self { inner: Inner::with_defaults(), sum: 0. }
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
