use itertools::Itertools;

/// Format all iterator elements, separated by `sep`.
///
/// Unlike the underlying `itertools::format`, **does not panic** if `fmt` is called more than once.
/// Should be used for logging purposes since `itertools::format` will panic when used by multiple loggers.
pub trait IterExtensions: Iterator {
    fn reusable_format(self, sep: &str) -> ReusableIterFormat<'_, Self>
    where
        Self: Sized,
    {
        ReusableIterFormat::new(self.format(sep))
    }

    /// Provides a run-length-encoding iterator that yields the cumulative count
    /// of elements seen so far, along with the value of the element. Useful for creating
    /// compressed representations of sequences with repeating elements
    fn rle_cumulative(self) -> impl Iterator<Item = (usize, Self::Item)>
    where
        Self: Sized,
        Self::Item: PartialEq,
    {
        let mut cumulative: usize = 0;
        self.dedup_with_count().map(move |(count, value)| {
            cumulative += count;
            (cumulative, value)
        })
    }
}

pub trait IterExtensionsRle<T>: Iterator<Item = (usize, T)>
where
    T: Clone,
{
    /// Expands a run-length encoded iterator back into its original sequence of elements.
    /// It takes an iterator of (cumulative_count, item) tuples and yields the repeated items
    fn expand_rle(self) -> impl Iterator<Item = T>
    where
        Self: Sized,
    {
        self.scan(0usize, |prev, (cum, item)| {
            let count = cum.checked_sub(*prev).filter(|&c| c > 0).expect("cumulative counts must be strictly increasing");
            *prev = cum;
            Some((count, item))
        })
        .flat_map(|(count, item)| std::iter::repeat_n(item, count))
    }
}

impl<I, T> IterExtensionsRle<T> for I
where
    I: Iterator<Item = (usize, T)>,
    T: Clone,
{
}

impl<T: ?Sized> IterExtensions for T where T: Iterator {}

pub struct ReusableIterFormat<'a, I> {
    inner: itertools::Format<'a, I>,
}

impl<'a, I> ReusableIterFormat<'a, I> {
    pub fn new(inner: itertools::Format<'a, I>) -> Self {
        Self { inner }
    }
}

impl<I> std::fmt::Display for ReusableIterFormat<'_, I>
where
    I: std::clone::Clone,
    I: Iterator,
    I::Item: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Clone the inner format to workaround the `Format: was already formatted once` internal error
        self.inner.clone().fmt(f)
    }
}

impl<I> std::fmt::Debug for ReusableIterFormat<'_, I>
where
    I: std::clone::Clone,
    I: Iterator,
    I::Item: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Clone the inner format to workaround the `Format: was already formatted once` internal error
        self.inner.clone().fmt(f)
    }
}

/// Returns an iterator over powers of two up to (the rounded up) available parallelism: `2, 4, 8, ..., 2^(available_parallelism.log2().ceil())`,
/// i.e., for `std::thread::available_parallelism = 15` the function will return `2, 4, 8, 16`
pub fn parallelism_in_power_steps() -> impl Iterator<Item = usize> {
    (1..=(std::thread::available_parallelism().unwrap().get() as f64).log2().ceil() as u32).map(|x| 2usize.pow(x))
}
