//!
//! Assertion macros based on the standard nightly version
//! <https://doc.rust-lang.org/src/core/macros/mod.rs.html#144-169>
//!

use std::fmt;

/// Asserts that an expression matches any of the given patterns.
///
/// Like in a `match` expression, the pattern can be optionally followed by `if`
/// and a guard expression that has access to names bound by the pattern.
///
/// On panic, this macro will print the value of the expression with its
/// debug representation.
///
/// Like [`assert!`], this macro has a second form, where a custom
/// panic message can be provided.
///
/// # Examples
///
/// ```ignore
/// use crate::assert_match;
///
/// let a = 1u32.checked_add(2);
/// let b = 1u32.checked_sub(2);
/// assert_match!(a, Some(_));
/// assert_match!(b, None);
///
/// let c = Ok("abc".to_string());
/// assert_match!(c, Ok(x) | Err(x) if x.len() < 100);
/// ```
#[macro_export]
macro_rules! assert_match {
    ($left:expr, $(|)? $( $pattern:pat_param )|+ $( if $guard: expr )? $(,)?) => {
        match $left {
            $( $pattern )|+ $( if $guard )? => {}
            ref left_val => {
                $crate::assert::assert_matches_failed(
                    left_val,
                    stringify!($($pattern)|+ $(if $guard)?),
                    core::option::Option::None
                );
            }
        }
    };
    ($left:expr, $(|)? $( $pattern:pat_param )|+ $( if $guard: expr )?, $($arg:tt)+) => {
        match $left {
            $( $pattern )|+ $( if $guard )? => {}
            ref left_val => {
                $crate::assert::assert_matches_failed(
                    left_val,
                    stringify!($($pattern)|+ $(if $guard)?),
                    std::option::Option::Some(std::format_args!($($arg)+))
                );
            }
        }
    };
}

#[derive(Debug)]
#[doc(hidden)]
pub enum AssertKind {
    Eq,
    Ne,
    Match,
}

/// Internal function for `assert_match!`
#[cold]
#[track_caller]
#[doc(hidden)]
pub fn assert_matches_failed<T: fmt::Debug + ?Sized>(left: &T, right: &str, args: Option<fmt::Arguments<'_>>) -> ! {
    // Use the Display implementation to display the pattern.
    struct Pattern<'a>(&'a str);
    impl fmt::Debug for Pattern<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(self.0, f)
        }
    }
    assert_failed_inner(AssertKind::Match, &left, &Pattern(right), args);
}

/// Non-generic version of the above functions, to avoid code bloat.
#[track_caller]
fn assert_failed_inner(kind: AssertKind, left: &dyn fmt::Debug, right: &dyn fmt::Debug, args: Option<fmt::Arguments<'_>>) -> ! {
    let op = match kind {
        AssertKind::Eq => "==",
        AssertKind::Ne => "!=",
        AssertKind::Match => "matches",
    };

    match args {
        Some(args) => panic!(
            r#"assertion failed: `(left {op} right)`
  left: `{left:?}`,
 right: `{right:?}`: {args}"#
        ),
        None => panic!(
            r#"assertion failed: `(left {op} right)`
  left: `{left:?}`,
 right: `{right:?}`"#,
        ),
    }
}
