use std::{collections::HashMap, marker::PhantomData};

use metrics_util::storage::Summary as SummaryImpl;
use prometheus::{
    Opts,
    core::{Desc, Describer, Metric, MetricVecBuilder},
    proto as pp,
};

pub type DefaultSummaryProvider = SummaryImpl;
pub const DEFAULT_QUANTILES: &[f64] = &[0.0, 0.5, 0.9, 0.95, 0.99, 0.999, 1.0];

/// Configuration options for [`GenericSummary`]
#[derive(Clone)]
pub struct SummaryOpts<O> {
    pub common_opts: Opts,

    /// Used to initialize the specific [`SummaryProvider`]
    pub summary_opts: O,

    /// Which quantiles to export
    pub quantiles: Vec<f64>,
}

impl<O> Describer for SummaryOpts<O> {
    fn describe(&self) -> prometheus::Result<Desc> {
        self.common_opts.describe()
    }
}

impl<O> SummaryOpts<O> {
    pub fn new<S1: Into<String>, S2: Into<String>>(name: S1, help: S2, summary: O) -> Self {
        Self {
            common_opts: Opts::new(name, help),
            summary_opts: summary,
            quantiles: Vec::from(DEFAULT_QUANTILES),
        }
    }

    /// See [`Opts::const_labels`]
    pub fn const_labels(mut self, const_labels: HashMap<String, String>) -> Self {
        self.common_opts = self.common_opts.const_labels(const_labels);
        self
    }

    /// See [`Opts::variable_labels`]
    pub fn variable_labels(mut self, variable_labels: Vec<String>) -> Self {
        self.common_opts = self.common_opts.variable_labels(variable_labels);
        self
    }

    /// Configure the quantiles to use when creating a prometheus protobuf summary
    pub fn quantiles<B: Into<Vec<f64>>>(self, quantiles: B) -> Self {
        Self { quantiles: quantiles.into(), ..self }
    }
}

/// Uses the configured [`SummaryProvider`] `S` to collect observations and compute quantiles
///
/// Main purpose is to wrap over the summary to convert it into a [`prometheus::proto::Summary`]
#[derive(Debug, Clone)]
pub struct GenericSummary<S = DefaultSummaryProvider> {
    label_pairs: Vec<pp::LabelPair>,
    summary: S,

    quantiles: Vec<f64>,
}

impl<S: SummaryProvider> GenericSummary<S> {
    pub fn new<V: AsRef<str>>(
        opts: &SummaryOpts<S::Opts>,
        label_values: &[V],
    ) -> prometheus::Result<Self> {
        let desc = opts.common_opts.describe()?;
        let label_pairs = make_label_pairs(&desc, label_values)?;

        Ok(Self {
            label_pairs,
            summary: S::new(&opts.summary_opts),
            quantiles: opts.quantiles.clone(),
        })
    }

    /// Record a given observation in the summary.
    pub fn observe(&self, v: f64) {
        self.summary.observe(v);
    }

    /// Make a snapshot of the current sumarry state exposed as a Protobuf struct
    pub fn proto(&self) -> pp::Summary {
        let mut summary = pp::Summary::default();

        summary.set_sample_sum(self.summary.sample_sum());
        summary.set_sample_count(self.summary.sample_count());

        let mut quantiles = Vec::with_capacity(self.quantiles.len());
        for quantile in self.quantiles.iter().cloned() {
            let mut q = pp::Quantile::default();
            q.set_quantile(quantile);

            let Some(val) = self.summary.quantile(quantile) else { continue };
            q.set_value(val);
            quantiles.push(q);
        }

        summary.set_quantile(quantiles);

        summary
    }
}

/// Abstracts over the metric summary logic user to compute the given quantile results
// TODO: decouple further from `SummaryImpl` or remove entirely and use the underlying type directly
pub trait SummaryProvider {
    type Opts: Clone + Send + Sync;

    /// Create a new instance of the given provider
    fn new(opts: &Self::Opts) -> Self;

    /// Computes the sum of all the samples in the summary
    fn sample_sum(&self) -> f64;

    /// Returns the number of samples in the summary
    fn sample_count(&self) -> u64;

    /// Add a new data point to the summary's collection
    fn observe(&self, _: f64);

    /// Attempt to comput the value for the given quantile
    fn quantile(&self, _: f64) -> Option<f64>;
}

impl SummaryProvider for SummaryImpl {
    type Opts = ();

    fn new(_: &Self::Opts) -> Self {
        Self::with_defaults()
    }

    fn sample_sum(&self) -> f64 {
        todo!("self.summary doesn't expose sum")
    }

    fn sample_count(&self) -> u64 {
        self.count() as u64
    }

    fn observe(&self, _val: f64) {
        todo!("self.summary.observe requires mut")
    }

    fn quantile(&self, quantile: f64) -> Option<f64> {
        self.quantile(quantile)
    }
}

impl<S: SummaryProvider + Send + Sync + Clone> Metric for GenericSummary<S> {
    fn metric(&self) -> pp::Metric {
        let mut m = pp::Metric::from_label(self.label_pairs.clone());
        m.set_summary(self.proto());
        m
    }
}

/// Similarly to [`prometheus::HistogramVec`], but for Summaries.
pub struct SummaryVecBuilder<S = DefaultSummaryProvider> {
    _p: PhantomData<S>,
}

impl<S> Clone for SummaryVecBuilder<S> {
    fn clone(&self) -> Self {
        Self { _p: self._p.clone() }
    }
}

impl<P> SummaryVecBuilder<P> {
    pub fn new() -> Self {
        Self { _p: PhantomData }
    }
}

impl<S: SummaryProvider + Send + Sync + Clone> MetricVecBuilder for SummaryVecBuilder<S> {
    type M = GenericSummary<S>;
    type P = SummaryOpts<S::Opts>;

    fn build<V: AsRef<str>>(&self, opts: &Self::P, vals: &[V]) -> prometheus::Result<Self::M> {
        Self::M::new(opts, vals)
    }
}

// from prometheus::value::make_label_pairs
fn make_label_pairs<V: AsRef<str>>(
    desc: &Desc,
    label_values: &[V],
) -> prometheus::Result<Vec<pp::LabelPair>> {
    if desc.variable_labels.len() != label_values.len() {
        return Err(prometheus::Error::InconsistentCardinality {
            expect: desc.variable_labels.len(),
            got: label_values.len(),
        });
    }

    let total_len = desc.variable_labels.len() + desc.const_label_pairs.len();
    if total_len == 0 {
        return Ok(vec![]);
    }

    if desc.variable_labels.is_empty() {
        return Ok(desc.const_label_pairs.clone());
    }

    let mut label_pairs = Vec::with_capacity(total_len);
    for (i, n) in desc.variable_labels.iter().enumerate() {
        let mut label_pair = pp::LabelPair::default();
        label_pair.set_name(n.clone());
        label_pair.set_value(label_values[i].as_ref().to_owned());
        label_pairs.push(label_pair);
    }

    for label_pair in &desc.const_label_pairs {
        label_pairs.push(label_pair.clone());
    }
    label_pairs.sort();
    Ok(label_pairs)
}
