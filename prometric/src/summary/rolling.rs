//! Rolling Summary implementation
//!
//! Uses [`metrics_exporter_prometheus::Distribution`] for the underlying representation

use std::{num::NonZeroU32, time::Duration};

use metrics_util::Quantile;
use quanta::Instant;

use crate::summary::{DEFAULT_QUANTILES, simple::SimpleSummary, traits::SummaryProvider};

// from metrics_exporter_prometheus::Distribution
pub const DEFAULT_SUMMARY_BUCKET_DURATION: Duration = Duration::from_secs(20);
pub const DEFAULT_SUMMARY_BUCKET_COUNT: NonZeroU32 = NonZeroU32::new(3).unwrap();

pub type RollingSummary = metrics_exporter_prometheus::Distribution;

/// Configuration for the Summary
///
/// See [`RollingSummary::new`] for documentation on the various options
#[derive(Clone)]
pub struct RollingSummaryOpts {
    pub quantiles: Vec<Quantile>,
    pub duration: Duration,
    pub max_buckets_count: NonZeroU32,
}

impl RollingSummaryOpts {
    pub fn with_quantiles(self, quantiles: &[f64]) -> Self {
        Self {
            quantiles: quantiles.iter().map(|quantile| Quantile::new(*quantile)).collect(),
            ..self
        }
    }
}

impl Default for RollingSummaryOpts {
    fn default() -> Self {
        Self {
            quantiles: DEFAULT_QUANTILES.iter().map(|quantile| Quantile::new(*quantile)).collect(),
            duration: DEFAULT_SUMMARY_BUCKET_DURATION,
            max_buckets_count: DEFAULT_SUMMARY_BUCKET_COUNT,
        }
    }
}

impl SummaryProvider for RollingSummary {
    type Opts = RollingSummaryOpts;
    type Summary = SimpleSummary;

    fn new(opts: &Self::Opts) -> Self {
        let distribution = metrics_exporter_prometheus::DistributionBuilder::new(
            opts.quantiles.clone(),
            Some(opts.duration),
            None,
            Some(opts.max_buckets_count),
            None,
        )
        .get_distribution("name not relevant");

        assert!(
            matches!(distribution, RollingSummary::Summary(..)),
            "DistributionBuilder didn't build a Summary!"
        );

        distribution
    }

    fn observe(&mut self, sample: f64) {
        // TODO: Determine if we want to also receive the measurement instant
        let now = Instant::now();
        self.record_samples(&[(sample, now)]);
    }

    fn snapshot(&self) -> <Self as SummaryProvider>::Summary {
        match self {
            RollingSummary::Summary(summary, _, sum) => {
                let summary = summary.snapshot(Instant::now());

                // NOTE: Technically this is the _total_ sum, not the rolling one
                // but metrics_util doesn't otherwise expose it at all
                // We could reproduce a rolling sum but then we would basically
                // reimplement the entire rolling summary
                SimpleSummary { inner: summary, sum: *sum }
            }
            _ => unreachable!("Distribution forced to be a Summary"),
        }
    }
}
