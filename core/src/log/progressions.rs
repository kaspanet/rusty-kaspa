use console::measure_text_width;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use once_cell::sync::Lazy;
use std::{
    borrow::Cow,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

//TODO: formatting! i.e. align and indent, and maybe some things for default colors for default text...
//Note: Trace and debug tracking is commented out for now, but can be commented in if needed, mostly because ux may be too much.

// Multi progress bar statics

/// This is the main atomic which will activate the multi progress bars.
static MULTI_PROGRESS_BAR_ACTIVE_PROXY: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));

// These are initialized depending on the state of the above  [`MULTI_PROGRESS_BAR_ACTIVE_PROXY`] atomic, and are only initialized if the above atomic is true.

/// This is a simple bool to tell us if progress bars are active. This should be referenced if this info is required.
/// we only need the [`MULTI_PROGRESS_BAR_ACTIVE_PROXY`] [`AtomicBool`] proxy for activation, to circumvent unsafe code.
/// but this should be more efficient for runtime queries.
pub static MULTI_PROGRESS_BAR_ACTIVE: Lazy<bool> = Lazy::new(|| MULTI_PROGRESS_BAR_ACTIVE_PROXY.load(Ordering::SeqCst));

/// Potentially our global [`MultiProgress`], if progressions are globally enabled, else [`None`].
pub static MULTI_PROGRESS_BAR: Lazy<Option<MultiProgress>> =
    Lazy::new(|| if *MULTI_PROGRESS_BAR_ACTIVE { Some(MultiProgress::new()) } else { None });

// TOP level progrsss bars i.e. reporter that get their own globals and are always active, if progressions are active.

static RUNNING_REPORTER: Lazy<Option<ProgressBar>> = Lazy::new(|| {
    if *MULTI_PROGRESS_BAR_ACTIVE {
        let heading = "{prefix:!.116.bold}: {msg:!.249} For {elapsed_precise:!.249} \n\n";
        let style = ProgressStyle::default_spinner().template(heading).expect("expected default spinner template to be valid");
        let pb = ProgressBar::new_spinner().with_prefix("Kaspad").with_style(style).with_tab_width(4);
        MULTI_PROGRESS_BAR.deref().clone().unwrap().add(pb.clone());
        pb.set_message("Running");
        pb.tick();
        pb.enable_steady_tick(Duration::from_millis(200)); // this is to move the spinner / update elapsed time.
        Some(pb)
    } else {
        None
    }
});

// Commented out Trace and Debug reporters for now, but can be uncommented if needed.
/*
pub static TRACE_REPORTER: Lazy<Option<ProgressBar>> = Lazy::new(|| if *MULTI_PROGRESS_BAR_ACTIVE {
    let heading = "{prefix:!.116.bold}: {msg:!.249} \n\n";
    let style = ProgressStyle::default_spinner()
        .template(heading)
        .expect("expected default spinner template to be valid");
    let pb = ProgressBar::new_spinner().with_prefix("Trace Reporter").with_style(style).with_tab_width(4);
    MULTI_PROGRESS_BAR.deref().clone().unwrap().add(pb.clone());
    pb.set_message("");
    pb.tick();
    Some(pb)
} else { None });

pub static DEBUG_REPORTER: Lazy<Option<ProgressBar>> = Lazy::new(|| if *MULTI_PROGRESS_BAR_ACTIVE {
    let heading = "{prefix:!.116.bold}: {msg:!.249}";
    let style = ProgressStyle::default_spinner()
        .template(heading)
        .expect("expected default spinner template to be valid");
    let pb = ProgressBar::new_spinner().with_prefix("Debug Reporter").with_style(style).with_tab_width(4);
    MULTI_PROGRESS_BAR.deref().clone().unwrap().add(pb.clone());
    pb.set_message("");
    pb.tick();
    Some(pb)
} else { None });
*/

pub static INFO_REPORTER: Lazy<Option<ProgressBar>> = Lazy::new(|| {
    if *MULTI_PROGRESS_BAR_ACTIVE {
        let heading = "{prefix:!.116.bold}:  {msg:!.249} \n\n";
        let style = ProgressStyle::default_spinner().template(heading).expect("expected default spinner template to be valid");
        let pb = ProgressBar::new_spinner().with_prefix("Info Reporter").with_style(style).with_tab_width(4);
        MULTI_PROGRESS_BAR.deref().clone().unwrap().add(pb.clone());
        pb.set_message("");
        pb.tick();
        Some(pb)
    } else {
        None
    }
});

pub static WARN_REPORTER: Lazy<Option<ProgressBar>> = Lazy::new(|| {
    if *MULTI_PROGRESS_BAR_ACTIVE {
        let heading = "{prefix:!.116.bold}:  {msg:.!.249}";
        let style = ProgressStyle::default_spinner().template(heading).expect("expected default spinner template to be valid");
        let pb = ProgressBar::new_spinner().with_prefix("Warn Reporter").with_style(style).with_tab_width(4);
        MULTI_PROGRESS_BAR.deref().clone().unwrap().add(pb.clone());
        pb.set_message("");
        pb.tick();
        Some(pb)
    } else {
        None
    }
});

pub static ERROR_REPORTER: Lazy<Option<ProgressBar>> = Lazy::new(|| {
    if *MULTI_PROGRESS_BAR_ACTIVE {
        let heading = "{prefix:!.116.bold}: {msg:!.249}";
        let style = ProgressStyle::default_spinner().template(heading).expect("expected default spinner template to be valid");
        let pb = ProgressBar::new_spinner().with_prefix("Error Reporter").with_style(style).with_tab_width(4);
        MULTI_PROGRESS_BAR.deref().clone().unwrap().add(pb.clone());
        pb.set_message("");
        pb.tick();
        Some(pb)
    } else {
        None
    }
});

const DEFAULT_PROGRESS_CHARS: &str = "=> ";
const DEFAULT_INIT_INDENT: usize = 36;

/// Init Lazy [`MULTI_PROGRESS_BAR_ACTIVE`], [`MULTI_PROGRESS_BAR_ACTIVE_PROXY`] and [`MULTI_PROGRESS_BAR`] bar globally, and activate depending on `activate`..
///
/// Note: `MULTI_PROGRESS_BAR_ACTIVE` and `MULTI_PROGRESS_BAR` are both Lazy statics,
/// so they will only be initialized once, and only if they are used.
/// As such, this function should be ensured to always
/// be called before any other function accessing these statics!!!
///
/// furthermore: as MULTI_PROGRESS_BAR interacts with the logging system,
/// this function should be called before any other function interacting with the logging system.
/// albeit, this should be gurtanteed by the fact that this function is paired with and called in the `init_logger` function in the code-flow of things.
//#[cfg(not(target_arch = "wasm32"))]
pub fn init_multi_progress_bar(activate: bool) {
    if activate {
        MULTI_PROGRESS_BAR_ACTIVE_PROXY.deref().store(true, Ordering::SeqCst);
        let _ = MULTI_PROGRESS_BAR_ACTIVE.deref();
        let mpb: &Option<MultiProgress> = Lazy::force(&MULTI_PROGRESS_BAR); // This is to force the Lazy static to initialize, we cannot log before this happens.
        let mpd = mpb.clone().unwrap();
        mpd.set_move_cursor(true); // https://github.com/console-rs/indicatif/issues/143
    }
    // We force all intializations here.
    let _ = Lazy::force(&MULTI_PROGRESS_BAR);
    let _ = Lazy::force(&RUNNING_REPORTER);
    let _ = Lazy::force(&ERROR_REPORTER);
    let _ = Lazy::force(&WARN_REPORTER);
    let _ = Lazy::force(&INFO_REPORTER);
    // Commented out Trace and Debug reporters for now, but can be uncommented if needed.
    //let _ = Lazy::force(&DEBUG_REPORTER);
    //let _ = Lazy::force(&TRACE_REPORTER);
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

pub fn maybe_exit_multi_progress() {
    if *MULTI_PROGRESS_BAR_ACTIVE {
        // Commented out Trace and Debug reporters for now, but can be uncommented if needed.
        //TRACE_REPORTER.clone().unwrap().finish();
        //DEBUG_REPORTER.clone().unwrap().finish();
        INFO_REPORTER.clone().unwrap().finish();
        WARN_REPORTER.clone().unwrap().finish();
        ERROR_REPORTER.clone().unwrap().finish();
        RUNNING_REPORTER.clone().unwrap().finish();
        //MULTI_PROGRESS_BAR.deref().clone().unwrap().clear().unwrap();
    }
}

/// Returns an [`Option<ProgrossBar>`], with a loading bar, depending on the state of the [`MULTI_PROGRESS_BAR`]
/// i.e. if progressions are globally enabled Some(ProgressBar) is returned, else None is returned.
///
/// Note: if end position is not known or infinite, perhaps use [`maybe_init_spinner`] instead.
pub fn maybe_init_progress_bar_spinner_pair(
    prefix: Cow<'static, str>,
    msg: Cow<'static, str>,
    len: u64,
    with_elapsed: bool,
    with_pos_full: bool,
    with_per_sec: bool,
    with_eta: bool,
    with_percent: bool,
) -> Option<(ProgressBar, ProgressBar)> {
    if let Some(mpb) = MULTI_PROGRESS_BAR.deref() {
        let bar = "[{wide_bar:.86}]";
        let style = ProgressStyle::default_bar()
            .template(bar)
            .expect("expected default bar template to be valid")
            .progress_chars(DEFAULT_PROGRESS_CHARS);
        let pbs =
            create_spinner(prefix.clone(), msg.clone(), len, with_elapsed, false, with_pos_full, with_per_sec, with_percent, with_eta);
        let pbb = ProgressBar::new(len).with_prefix(prefix).with_message(msg).with_style(style).with_tab_width(4);

        // bars only go to the bottom level
        mpb.insert_from_back(0, pbs.clone());
        mpb.insert_from_back(0, pbb.clone());

        return Some((pbb, pbs));
    };
    None
}

pub trait ProgressBarSpinnerPair {
    fn set_position(&self, pos: u64);

    fn set_length(&self, len: u64);
}

impl ProgressBarSpinnerPair for (ProgressBar, ProgressBar) {
    fn set_position(&self, pos: u64) {
        self.0.set_position(pos);
        self.1.set_position(pos);
    }

    fn set_length(&self, len: u64) {
        self.0.set_length(len);
        self.1.set_length(len);
    }
}

fn create_spinner(
    prefix: Cow<'static, str>,
    msg: Cow<'static, str>,
    len: u64,
    with_elapsed: bool,
    with_pos: bool,
    with_pos_full: bool,
    with_per_sec: bool,
    with_percent: bool,
    with_eta: bool,
) -> ProgressBar {
    let heading = format!(
        "{{prefix:!.116.bold}}: {{msg:!.249}} {0:>indent$} => ",
        " ",
        indent = DEFAULT_INIT_INDENT - (measure_text_width(&prefix) + measure_text_width(&msg))
    );
    let elapsed = if with_elapsed { "Elapsed: {elapsed_precise:!.249}\t" } else { "" };
    let pos = if with_pos { "Progress: {human_pos:!.249}\t" } else { "" };
    let pos_full = if with_pos_full { "Progress: {human_pos:!.249}/{human_len:!.249}\t" } else { "" };
    let per_sec = if with_per_sec { "Speed: {per_sec:!.249}\t" } else { "" };
    let percent = if with_percent { "Percent: {percent:!.249}%\t" } else { "" };
    let eta = if with_eta { "ETA: {eta:!.249}\t" } else { "" };

    let message = heading + elapsed + pos + pos_full + per_sec + percent + eta;
    let style = ProgressStyle::default_spinner().template(message.as_str()).expect("expected default spinner template to be valid");

    ProgressBar::new(len).with_prefix(prefix).with_message(msg).with_style(style).with_tab_width(4)
}

/// Returns an [`Option<ProgrossBar>`], with a loading bar, depending on the state of the [`MULTI_PROGRESS_BAR`]
/// i.e. if progressions are globally enabled Some(ProgressBar) is returned, else None is returned.
///
/// Note: if end position is known and finite, perhaps use [`maybe_init_progress_bar`] instead.
pub fn maybe_init_spinner(
    prefix: Cow<'static, str>,
    msg: Cow<'static, str>,
    with_elapsed: bool,
    with_pos: bool,
    with_per_sec: bool,
) -> Option<ProgressBar> {
    if let Some(mpb) = MULTI_PROGRESS_BAR.deref() {
        let pb = create_spinner(prefix, msg, 0u64, with_elapsed, with_pos, false, with_per_sec, false, false);
        // spinners only go at the top level
        mpb.insert(6, pb.clone()); // plus one for each log reporter... (error, warn, info, debug, trace)
        return Some(pb);
    };
    None
}

#[cfg(test)]
mod test {
    use crate::log::progressions::*;
    use indicatif::MultiProgress;
    use once_cell::sync::Lazy;
    use std::{borrow::Cow, sync::atomic::Ordering};

    #[test]
    fn test_inits() {
        init_multi_progress_bar(true);

        assert!(MULTI_PROGRESS_BAR_ACTIVE_PROXY.load(Ordering::SeqCst));
        assert!(*MULTI_PROGRESS_BAR_ACTIVE);
        assert!(MULTI_PROGRESS_BAR.is_some());
        assert!(maybe_init_progress_bar_spinner_pair(
            Cow::Borrowed("test"),
            Cow::Borrowed("test"),
            0u64,
            true,
            true,
            true,
            true,
            true
        )
        .is_some());
        assert!(maybe_init_spinner(Cow::Borrowed("test"), Cow::Borrowed("test"), true, true, true).is_some());
        // Commented out Trace and Debug reporters for now, but can be uncommented if needed.
        //assert!(TRACE_REPORTER.is_some());
        //assert!(DEBUG_REPORTER.is_some());
        assert!(INFO_REPORTER.is_some());
        assert!(WARN_REPORTER.is_some());
        assert!(ERROR_REPORTER.is_some());
        assert!(RUNNING_REPORTER.is_some());
        MULTI_PROGRESS_BAR_ACTIVE_PROXY.store(false, Ordering::SeqCst);

        // Required to reset the static variable.
        // Note: This is safe as long as no other tests are running in parallel,
        // which are also using the the MULTI_PROGRESS_BAR_ACTIVE global static - We do not expect this to be the case.
        unsafe {
            #[allow(clippy::mut_from_ref)] // yes clippy, we know what we are doing here, this is marked unsafe code.
            unsafe fn reset_immutable_global<T>(reference: &T) -> &mut T {
                let const_ptr = reference as *const T;
                let mut_ptr = const_ptr as *mut T;
                &mut *mut_ptr
            }

            // Commented out Trace and Debug reporters for now, but can be uncommented if needed.
            // let val = reset_immutable_global(&TRACE_REPORTER);
            // *val = Lazy::new(|| if *MULTI_PROGRESS_BAR_ACTIVE { Some(ProgressBar::new_spinner()) } else { None });
            // let val = reset_immutable_global(&DEBUG_REPORTER);
            // *val = Lazy::new(|| if *MULTI_PROGRESS_BAR_ACTIVE{ Some(ProgressBar::new_spinner()) } else { None });
            let val = reset_immutable_global(&MULTI_PROGRESS_BAR_ACTIVE);
            *val = Lazy::new(|| MULTI_PROGRESS_BAR_ACTIVE_PROXY.load(Ordering::SeqCst));
            let val = reset_immutable_global(&MULTI_PROGRESS_BAR);
            *val = Lazy::new(|| if *MULTI_PROGRESS_BAR_ACTIVE { Some(MultiProgress::new()) } else { None });
            let val = reset_immutable_global(&INFO_REPORTER);
            *val = Lazy::new(|| if MULTI_PROGRESS_BAR.is_some() { Some(ProgressBar::new_spinner()) } else { None });
            let val = reset_immutable_global(&WARN_REPORTER);
            *val = Lazy::new(|| {
                if MULTI_PROGRESS_BAR_ACTIVE_PROXY.load(Ordering::SeqCst) {
                    Some(ProgressBar::new_spinner())
                } else {
                    None
                }
            });
            let val = reset_immutable_global(&ERROR_REPORTER);
            *val = Lazy::new(|| if *MULTI_PROGRESS_BAR_ACTIVE { Some(ProgressBar::new_spinner()) } else { None });
            let val = reset_immutable_global(&RUNNING_REPORTER);
            *val = Lazy::new(|| if MULTI_PROGRESS_BAR.is_some() { Some(ProgressBar::new_spinner()) } else { None });
        };

        init_multi_progress_bar(false);

        assert!(!MULTI_PROGRESS_BAR_ACTIVE_PROXY.load(Ordering::SeqCst));
        assert!(!*MULTI_PROGRESS_BAR_ACTIVE);
        assert!(MULTI_PROGRESS_BAR.is_none());
        assert!(maybe_init_progress_bar_spinner_pair(
            Cow::Borrowed("test"),
            Cow::Borrowed("test"),
            0u64,
            true,
            true,
            true,
            true,
            true
        )
        .is_none());
        assert!(maybe_init_spinner(Cow::Borrowed("test"), Cow::Borrowed("test"), true, true, true).is_none());
        // Commented out Trace and Debug reporters for now, but can be uncommented if needed.
        //assert!(TRACE_REPORTER.is_none());
        //assert!(DEBUG_REPORTER.is_none());
        assert!(INFO_REPORTER.is_none());
        assert!(WARN_REPORTER.is_none());
        assert!(ERROR_REPORTER.is_none());
        assert!(RUNNING_REPORTER.is_none());
    }
}
