pub mod impls {
    use console::style;
    use std::fmt;

    pub fn log_success(source: &str, args: &fmt::Arguments<'_>) {
        println!("{:>12} {}", style(source).green().bold(), args);
    }

    pub fn log_warn(source: &str, args: &fmt::Arguments<'_>) {
        println!("{:>12} {}", style(source).yellow().bold(), args);
    }

    pub fn log_error(source: &str, args: &fmt::Arguments<'_>) {
        println!("{:>12} {}", style(source).red().bold(), args);
    }
}

#[macro_export]
macro_rules! log_success {
    ($target:expr, $($t:tt)*) => (
        $crate::log::impls::log_success($target, &format_args!($($t)*))
    )
}

pub use log_success;

#[macro_export]
macro_rules! log_warn {

    ($target:expr, $($t:tt)*) => (
        $crate::log::impls::log_warn($target, &format_args!($($t)*))
    )
}

pub use log_warn;

#[macro_export]
macro_rules! log_error {
    ($target:expr, $($t:tt)*) => (
        $crate::log::impls::log_error($target, &format_args!($($t)*))
    )
}

pub use log_error;
