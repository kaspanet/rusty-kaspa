use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use once_cell::sync::Lazy;
use std::{
    borrow::Cow,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub const IDENT: &str = "MultiProgressBar";

pub const DEFAULT_BAR_TEMPLATE: &str = "[{spinner:.cyan/blue}] {prefix:.green.>10.<10.bold}: {msg:.yellow.>10.<10} | {elapsed_precise:.>10.<10} | [{wide_bar:.cyan/blue}] | {human_pos:.<10}/{human_len:.<10} | {per_sec:.<10.>10} | {ET!: {eta}.>10.<10} | {{percent}%:.>10.<10} |";
pub const DEFAULT_SPINNER_TEMPLATE: &str = "[{spinner:.cyan/blue}] {prefix:.green.>10.<10.bold}: {msg:.yellow.>10.<10} | {elapsed_precise:.>10.<10} | {human_pos:.<10} total {:.<10} | {per_sec.>10.<10} |";
pub const DEFAULT_SPINNER_CHARS: &[&str] = &["|", "/", "-", "\\"];
pub const DEFAULT_PROGRESS_CHARS: &str = "=> ";
pub static MULTI_PROGRESS_BAR_ACTIVE: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));

pub static MULTI_PROGRESS_BAR: Lazy<Option<MultiProgress>> =
    Lazy::new(|| if MULTI_PROGRESS_BAR_ACTIVE.load(Ordering::SeqCst) { Some(MultiProgress::new()) } else { None });

pub fn init_multi_progress_bar(activate: bool) {
    if activate {
        //info!("[{0}] Initializing active...", IDENT);
        MULTI_PROGRESS_BAR_ACTIVE.deref().store(true, Ordering::SeqCst);
    }
    let _ = MULTI_PROGRESS_BAR.deref(); // This is to force the Lazy static to initialize, we cannot log before this happens.
                                        //println!("[{0}] Initialized: {1}", IDENT, MULTI_PROGRESS_BAR_ACTIVE.deref().load(Ordering::SeqCst));
}

pub fn maybe_suspend<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    if let Some(mpb) = MULTI_PROGRESS_BAR.deref() {
        return mpb.suspend(f);
    }
    f()
}

/// Returns an `Option<ProgrossBar>` depending on the state of the `MULTI_PROGRESS_BAR` i.e. if progressions are globally enabled.
pub fn maybe_init_progress_bar(prefix: Cow<'static, str>, msg: Cow<'static, str>, len: u64) -> Option<ProgressBar> {
    if let Some(mpb) = MULTI_PROGRESS_BAR.deref() {
        //info!("[{0}] Adding Progress bar with prefix: {1}; message: {2}; and length: {3}", IDENT, prefix, msg, len); //TODO: change to debug / trace after reveiw.
        let style = ProgressStyle::default_bar()
            .template(DEFAULT_BAR_TEMPLATE)
            .expect("expected default bar template to be valid")
            .progress_chars(DEFAULT_PROGRESS_CHARS);

        let pb = ProgressBar::new(len).with_prefix(prefix).with_message(msg).with_style(style);

        mpb.add(pb.clone());

        return Some(pb);
    };
    None
}

pub fn maybe_init_spinner(prefix: Cow<'static, str>, msg: Cow<'static, str>) -> Option<ProgressBar> {
    if let Some(mpb) = MULTI_PROGRESS_BAR.deref() {
        //info!("[{0}] Adding Spinner with prefix: {1}; message: {2}", IDENT, prefix, msg); //TODO: change to debug / trace after reveiw.
        let style = ProgressStyle::default_spinner()
            .template(DEFAULT_SPINNER_TEMPLATE)
            .expect("expected default spinner template to be valid")
            .tick_strings(DEFAULT_SPINNER_CHARS);

        let pb = ProgressBar::new_spinner().with_prefix(prefix).with_message(msg).with_style(style);

        mpb.add(pb.clone());

        return Some(pb);
    };
    None
}

pub fn build_template_from(updates: Vec<(String, String, String)>) -> String {
    let mut inner_message = String::new();
    for (object, op, count) in updates {
        inner_message.push_str(&format!(" {0} {1}: {2} |", object, op, count));
    }
    let prependage = "[{spinner:.cyan/blue}][{prefix:.green}]: {message} | {elapsed_precise} |".to_string();
    format!("{0}{1}", prependage, inner_message).to_string()
}

#[cfg(test)]
mod test {
    use crate::log::progressions::*;
    use std::{borrow::Cow, sync::atomic::Ordering};

    #[test]
    fn test_inits() {
        init_multi_progress_bar(false);

        assert!(MULTI_PROGRESS_BAR_ACTIVE.load(Ordering::SeqCst));
        assert!(MULTI_PROGRESS_BAR.is_some());
        assert!(maybe_init_progress_bar(Cow::Borrowed("test"), Cow::Borrowed("test"), 0u64).is_some());
        assert!(maybe_init_spinner(Cow::Borrowed("test"), Cow::Borrowed("test")).is_some());
        MULTI_PROGRESS_BAR_ACTIVE.store(false, Ordering::SeqCst);

        // Required to reset the static variable.
        // Note: This is safe as long as no other tests are running in parallel,
        // which are also using the the MULTI_PROGRESS_BAR_ACTIVE global static - We do not expect this to be the case.
        unsafe {
            unsafe fn reset_immutable_global<T>(reference: &T) -> &mut T {
                let const_ptr = reference as *const T;
                let mut_ptr = const_ptr as *mut T;
                &mut *mut_ptr
            }
            let val = reset_immutable_global(&MULTI_PROGRESS_BAR);
            *val = Lazy::new(|| if MULTI_PROGRESS_BAR_ACTIVE.load(Ordering::SeqCst) { Some(MultiProgress::new()) } else { None });
        };

        init_multi_progress_bar(false);

        assert!(!MULTI_PROGRESS_BAR_ACTIVE.load(Ordering::SeqCst));
        assert!(MULTI_PROGRESS_BAR.is_none());
        assert!(maybe_init_progress_bar(Cow::Borrowed("test"), Cow::Borrowed("test"), 0u64).is_none());
        assert!(maybe_init_spinner(Cow::Borrowed("test"), Cow::Borrowed("test")).is_none());
    }
}
