use std::collections::HashMap;

use prometheus::core::MetricVec;

pub mod traits;
use traits::{NonConcurrentSummaryProvider, SummaryMetric, SummaryProvider};

mod generic;
use generic::SummaryVecBuilder;
pub use generic::{DEFAULT_QUANTILES, SummaryOpts};

pub mod simple;

pub mod rolling;
use rolling::{RollingSummary, RollingSummaryOpts};

pub mod batching;
use batching::{BatchOpts, BatchedSummary};

pub type DefaultSummaryProvider = BatchedSummary<RollingSummary>;

type SummaryVec<S = DefaultSummaryProvider> = MetricVec<SummaryVecBuilder<S>>;

/// A Summary metric.
#[derive(Clone, Debug)]
pub struct Summary<S: SummaryMetric = DefaultSummaryProvider> {
    inner: SummaryVec<S>,
}

impl<S: SummaryMetric> Summary<S> {
    // NOTE: Unlike other items like `HistogramVec`, this can't exist on `MetricVec` directly
    // as we are not allowed to have inherent impls on foreign types
    fn new_summary_vec(
        opts: SummaryOpts<S::Opts>,
        label_names: &[&str],
    ) -> prometheus::Result<SummaryVec<S>> {
        let variable_names = label_names.iter().map(|s| (*s).to_owned()).collect();
        let opts = opts.variable_labels(variable_names);
        let metric_vec = MetricVec::create(
            prometheus::proto::MetricType::SUMMARY,
            SummaryVecBuilder::<S>::new(),
            opts,
        )?;

        Ok(metric_vec as SummaryVec<S>)
    }
}

impl Summary<DefaultSummaryProvider> {
    pub fn new(
        registry: &prometheus::Registry,
        name: &str,
        help: &str,
        labels: &[&str],
        const_labels: HashMap<String, String>,
        quantiles: Option<Vec<f64>>,
    ) -> Self {
        let quantiles = quantiles.unwrap_or(generic::DEFAULT_QUANTILES.to_vec());

        let opts = RollingSummaryOpts::default().with_quantiles(&quantiles);
        let opts = BatchOpts::from_inner(opts);
        let opts =
            SummaryOpts::new(name, help, opts).const_labels(const_labels).quantiles(quantiles);

        let metric = Self::new_summary_vec(opts, labels).unwrap();

        let boxed = Box::new(metric.clone());
        if let Err(e) = registry.register(boxed.clone()) {
            let id = format!("{}, Labels: {}", name, labels.join(", "),);
            // If the metric is already registered, overwrite it.
            if matches!(e, prometheus::Error::AlreadyReg) {
                registry
                    .unregister(boxed.clone())
                    .unwrap_or_else(|_| panic!("Failed to unregister metric {id}"));

                registry
                    .register(boxed)
                    .unwrap_or_else(|_| panic!("Failed to overwrite metric {id}"));
            } else {
                panic!("Failed to register metric {id}");
            }
        }

        Self { inner: metric }
    }
}

impl<S> Summary<S>
where
    S: SummaryProvider<Summary = <S as NonConcurrentSummaryProvider>::Summary> + SummaryMetric,
{
    pub fn observe(&self, labels: &[&str], value: f64) {
        SummaryProvider::observe(&**self.inner.with_label_values(labels), value);
    }

    pub fn snapshot(&self, labels: &[&str]) -> <S as NonConcurrentSummaryProvider>::Summary {
        NonConcurrentSummaryProvider::snapshot(&**self.inner.with_label_values(labels))
    }
}

#[cfg(test)]
mod tests {
    use crate::traits::Summary as _;

    use super::*;

    const MEASUREMENTS: usize = 50_000;
    const PRINT_EVERY: usize = 100;

    fn measure<S>(summary: Summary<S>)
    where
        S: SummaryProvider<Summary = <S as NonConcurrentSummaryProvider>::Summary> + SummaryMetric,
    {
        for i in 0..MEASUREMENTS {
            let start = std::time::Instant::now();
            summary.observe(&[], i as f64);
            if i % PRINT_EVERY == 0 {
                println!("Time taken: {:?}", start.elapsed());
            }
        }

        let result = summary.snapshot(&[]);
        assert_eq!(
            result.sample_count(),
            MEASUREMENTS as u64,
            "Should have all measurements present in the collection"
        );
    }

    #[test]
    fn smoke() {
        let registry = prometheus::default_registry();
        let summary =
            Summary::new(&registry, "smoke", "Smoke test summary", &[], Default::default(), None);

        measure(summary)
    }
}
