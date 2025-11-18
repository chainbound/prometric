use std::collections::HashMap;

use prometheus::core::MetricVec;

mod core;
use core::{DefaultSummaryProvider, SummaryProvider, SummaryVecBuilder};

pub use core::SummaryOpts;

type SummaryVec<S = DefaultSummaryProvider> = MetricVec<SummaryVecBuilder<S>>;

/// A summary metric.
#[derive(Clone, Debug)]
pub struct Summary<S: SummaryProvider + Send + Sync + Clone = DefaultSummaryProvider> {
    inner: SummaryVec<S>,
}

impl<S: SummaryProvider + Clone + Send + Sync> Summary<S> {
    // Unlike other items like `HistogramVec`, this can't exist on `MetricVec` directly
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
    pub fn new<B: Into<Vec<f64>>>(
        registry: &prometheus::Registry,
        name: &str,
        help: &str,
        labels: &[&str],
        const_labels: HashMap<String, String>,
        quantiles: Option<B>,
    ) -> Self {
        let quantiles = quantiles.map(Into::into).unwrap_or(core::DEFAULT_QUANTILES.to_vec());
        let opts = SummaryOpts::new(name, help, ()).const_labels(const_labels).quantiles(quantiles);
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

    pub fn observe(&self, labels: &[&str], value: f64) {
        self.inner.with_label_values(labels).observe(value);
    }
}
