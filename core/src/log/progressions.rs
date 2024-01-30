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
use console ::measure_text_width;
// Multi progress bar statics
pub static MULTI_PROGRESS_BAR_ACTIVE: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));
pub static MULTI_PROGRESS_BAR: Lazy<Option<MultiProgress>> =
    Lazy::new(|| if MULTI_PROGRESS_BAR_ACTIVE.load(Ordering::SeqCst) { Some(MultiProgress::new()) } else { None });

const DEFAULT_SPINNER_CHARS: &[&str] = &["|", "/", "-", "\\"];
const DEFAULT_PROGRESS_CHARS: &str = "=> ";
const DEFAULT_INIT_INDENT: usize = 48;

/// Init Lazy `MULTI_PROGRESS_BAR_ACTIVE` and `MULTI_PROGRESS_BAR` bar globally and activate depending on `activate`..
///
/// Note: `MULTI_PROGRESS_BAR_ACTIVE` and `MULTI_PROGRESS_BAR` are both Lazy statics,
/// so they will only be initialized once, and only if they are used.
/// As such, this function should be ensured to always
/// be called before any other function accessing these statics!!!
///
/// furthermore: as MULTI_PROGRESS_BAR interacts with the logging system,
/// this function should be called before any other function interacting with the logging system.
/// albeit, this should be gurtanteed by the fact that this function is paired with and called in the `init_logger` function internally.
//#[cfg(not(target_arch = "wasm32"))]
pub fn init_multi_progress_bar(activate: bool) {
    if activate {
        //info!("[{0}] Initializing active...", IDENT);
        MULTI_PROGRESS_BAR_ACTIVE.deref().store(true, Ordering::SeqCst);
    }
    let _ = MULTI_PROGRESS_BAR.deref(); // This is to force the Lazy static to initialize, we cannot log before this happens.
                                        //println!("[{0}] Initialized: {1}", IDENT, MULTI_PROGRESS_BAR_ACTIVE.deref().load(Ordering::SeqCst));
}

/// Suspend all progress bars globally, if multi progress bars are globally enabled, else just perform func.
//#[cfg(not(target_arch = "wasm32"))]
pub fn maybe_suspend<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    if let Some(mpb) = MULTI_PROGRESS_BAR.deref() {
        return mpb.suspend(f);
    }
    f()
}

/// Returns an [`Option<ProgrossBar>`], with a loading bar, depending on the state of the [`MULTI_PROGRESS_BAR`]
/// i.e. if progressions are globally enabled Some(ProgressBar) is returned, else None is returned.
///
/// Note: if end position is not known or infinite, perhaps use [`maybe_init_spinner`] instead.
pub fn maybe_init_progress_bar(
    prefix: Cow<'static, str>,
    msg: Cow<'static, str>,
    len: u64,
    with_pos: bool,
    with_processed: bool,
    with_per_sec: bool,
    with_eta: bool,
    with_percent: bool,
) -> Option<ProgressBar> {
    println!("{0}", prefix.to_string());
    println!("{0}", msg.to_string());
    println!("{0} : {1} / {2} : {3}", prefix, measure_text_width(&prefix), msg, measure_text_width(&msg));
    if let Some(mpb) = MULTI_PROGRESS_BAR.deref() {
        let heading = format!(
            "[{{spinner:.cyan/blue}}] {{prefix:.green.bold}}: {{msg:.yellow}} {:>indent$}| Time: {{elapsed_precise:8}} |",
            " ",
            indent = DEFAULT_INIT_INDENT - (measure_text_width(&prefix) + measure_text_width(&msg))
        );
        let bar = format!(" [{{wide_bar:.cyan/blue}}] |");
        let processed = if with_processed { format!(" Total: {{human_pos:>14}}/{{len:14}} |") } else { "".to_string() };
        let per_sec = if with_per_sec { format!(" Speed: {{per_sec:14}} |") } else { "".to_string() };
        let eta = if with_eta { format!(" ETA: {{eta:8}} |") } else { "".to_string() };
        let percent = if with_percent { format!(" {{percent:5}}% |") } else { "".to_string() };

        let style = ProgressStyle::default_bar()
            .template(&(heading + &bar + &processed + &per_sec + &eta + &percent))
            .expect("expected default bar template to be valid")
            .progress_chars(DEFAULT_PROGRESS_CHARS)
            .tick_strings(DEFAULT_SPINNER_CHARS);

        let pb = ProgressBar::new(len).with_prefix(prefix).with_message(msg).with_style(style);

        // bars only go to the bottom level
        mpb.insert(0, pb.clone());

        return Some(pb);
    };
    None
}

/// Returns an [`Option<ProgrossBar>`], with a loading bar, depending on the state of the [`MULTI_PROGRESS_BAR`]
/// i.e. if progressions are globally enabled Some(ProgressBar) is returned, else None is returned.
///
/// Note: if end position is known and finite, perhaps use [`maybe_init_progress_bar`] instead.
pub fn maybe_init_spinner(
    prefix: Cow<'static, str>,
    msg: Cow<'static, str>,
    with_pos: bool,
    with_per_sec: bool,
) -> Option<ProgressBar> {

    if let Some(mpb) = MULTI_PROGRESS_BAR.deref() {
        //info!("[{0}] Adding Spinner with prefix: {1}; message: {2}", IDENT, prefix, msg); //TODO: change to debug / trace after reveiw.
        //println!("{} : {} / {} : {}", prefix, prefix.chars().count(), msg, msg.chars().count());
        let per_sec = if with_per_sec { format!(" Speed: {{per_sec:14}} |") } else { "".to_string() };
        let pos = if with_pos { format!(" Total: {{human_pos:14}} |") } else { "".to_string() };
        let heading = format!(
            "[{{spinner:.cyan/blue}}] {{prefix:.green.bold}}: {{msg:.yellow}} {:>indent$}| Time: {{elapsed_precise:14}} |",
            " ",
            indent = DEFAULT_INIT_INDENT - (measure_text_width(&prefix) + measure_text_width(&msg))
        );
        let style = ProgressStyle::default_spinner()
            .template(&(heading + &pos + &per_sec))
            //.template(&format!("[{{spinner}}]{{prefix:.blue)}}: {{msg}}{:>50}|{:>indent$}{{elapsed_precise}}{:>indent$}|{:>indent$}{{human_pos}} total{:>indent$}|{:>indent$}{{per_sec}}{:>indent$}|", "","","","","","","", indent=DEFAULT_INDENT))
            .expect("expected default spinner template to be valid")
            .tick_strings(DEFAULT_SPINNER_CHARS);

        let pb = ProgressBar::new_spinner().with_prefix(prefix).with_message(msg).with_style(style);

        // spinners only go at the top level
        mpb.insert_from_back(0, pb.clone());

        return Some(pb);
    };
    None
}

mod test {
    use crate::log::progressions::*;
    use indicatif::MultiProgress;
    use once_cell::sync::Lazy;
    use std::{borrow::Cow, sync::atomic::Ordering};

    #[test]
    fn test_inits() {
        init_multi_progress_bar(false);

        assert!(MULTI_PROGRESS_BAR_ACTIVE.load(Ordering::SeqCst));
        assert!(MULTI_PROGRESS_BAR.is_some());
        assert!(maybe_init_progress_bar(Cow::Borrowed("test"), Cow::Borrowed("test"), 0u64, true, true, true, true, true).is_some());
        assert!(maybe_init_spinner(Cow::Borrowed("test"), Cow::Borrowed("test"), true, true).is_some());
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
        assert!(maybe_init_progress_bar(Cow::Borrowed("test"), Cow::Borrowed("test"), 0u64, true, true, true, true, true).is_none());
        assert!(maybe_init_spinner(Cow::Borrowed("test"), Cow::Borrowed("test"), true, true).is_none());
    }
}
