/// Abstracts over the metric summary logic user to compute the given quantile results
pub trait SummaryProvider {
    type Opts: Clone + Send + Sync;
    type Summary: Summary;

    /// Create a new instance of the given provider
    fn new(opts: &Self::Opts) -> Self;

    /// Add a new data point to the summary's collection
    fn observe(&mut self, _: f64);

    /// Return the current summary computed over the observations
    fn snapshot(&self) -> Self::Summary;
}

/// Abstracts over the representation of the Summary data
pub trait Summary {
    /// Computes the sum of all the samples in the summary
    fn sample_sum(&self) -> f64;

    /// Returns the number of samples in the summary
    fn sample_count(&self) -> u64;

    /// Attempt to compute the value for the given quantile
    fn quantile(&self, _: f64) -> Option<f64>;
}
