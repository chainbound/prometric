use std::collections::HashMap;

use prometheus::core::MetricVec;

pub mod traits;
use traits::{ConcurrentSummaryProvider, SummaryMetric};

mod generic;
use generic::SummaryVecBuilder;
pub use generic::{DEFAULT_QUANTILES, SummaryOpts};

pub mod simple;

pub mod rolling;
use rolling::{RollingSummary, RollingSummaryOpts};

pub mod batching;
use batching::{BatchOps, BatchedSummary};

pub type DefaultSummaryProvider = BatchedSummary<RollingSummary>;

type SummaryVec<S = DefaultSummaryProvider> = MetricVec<SummaryVecBuilder<S>>;

/// A Summary metric.
#[derive(Clone, Debug)]
pub struct Summary<S: ConcurrentSummaryProvider + SummaryMetric = DefaultSummaryProvider> {
    inner: SummaryVec<S>,
}

impl<S: ConcurrentSummaryProvider + SummaryMetric> Summary<S> {
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
        let opts = BatchOps::from_inner(opts);
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

impl<S: ConcurrentSummaryProvider + SummaryMetric> Summary<S> {
    pub fn observe(&self, labels: &[&str], value: f64) {
        self.inner.with_label_values(labels).observe(value);
    }
}
