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
