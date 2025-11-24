//! Rolling Summary implementation
//!
//! Uses [`metrics_exporter_prometheus::Distribution`] for the underlying representation

use std::{num::NonZeroU32, time::Duration};

use metrics_util::Quantile;
use quanta::Instant;

use crate::summary::{
    DEFAULT_QUANTILES,
    simple::SimpleSummary,
    traits::{NonConcurrentSummaryProvider, Summary},
};

// from metrics_exporter_prometheus::Distribution
pub const DEFAULT_SUMMARY_BUCKET_DURATION: Duration = Duration::from_secs(20);
pub const DEFAULT_SUMMARY_BUCKET_COUNT: NonZeroU32 = NonZeroU32::new(3).unwrap();

/// A Rolling summary implementation, backed by [`metrics_exporter_prometheus::Distribution`]
///
/// This is a summry which includes a "rolling" algorithm, to exclude measurements past the
/// configured `duration` (in [`RollingSummaryOpts`]). As the RollingSummary stores measurements in
/// buckets, the measurement expiry is on a per-bucket basis, meaning that old values might still be
/// used if the bucket they belong in hasn't expired yet.
///
/// Quantiles are computed using [`SimpleSummary`], which will contain the non-expired measurements
pub type RollingSummary = metrics_exporter_prometheus::Distribution;

/// A [`crate::summary::traits::Summary`] snapshot implementation for [`RollingSummary`]
///
/// Will return the total count and total sum, but use the resulting [`SimpleSummary`] from
/// [`RollingSummary`] for the quantile computation, which only uses non-expired values
///
/// # References
/// [`RollingSummary`] is usually rendered with the total sum and count, but using the active values for quantile computation,
/// as seen in [`metrics_exporter_prometheus`](https://github.com/metrics-rs/metrics/blob/main/metrics-exporter-prometheus/src/recorder.rs#L183).
pub struct RollingSummarySnapshot {
    count: usize,
    inner: SimpleSummary,
}

impl Summary for RollingSummarySnapshot {
    fn sample_sum(&self) -> f64 {
        self.inner.sample_sum()
    }

    fn sample_count(&self) -> u64 {
        self.count as u64
    }

    fn quantile(&self, val: f64) -> Option<f64> {
        self.inner.quantile(val)
    }
}

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

impl NonConcurrentSummaryProvider for RollingSummary {
    type Opts = RollingSummaryOpts;
    type Summary = RollingSummarySnapshot;

    fn new_provider(opts: &Self::Opts) -> Self {
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

    fn snapshot(&self) -> RollingSummarySnapshot {
        match self {
            RollingSummary::Summary(summary, _, sum) => {
                let count = summary.count();
                let snapshot = summary.snapshot(Instant::now());
                let inner = SimpleSummary { inner: snapshot, sum: *sum };

                RollingSummarySnapshot { inner, count }
            }
            _ => unreachable!("Distribution forced to be a Summary"),
        }
    }
}
