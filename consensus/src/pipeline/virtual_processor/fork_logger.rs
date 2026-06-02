use kaspa_core::{info, log::FORK_KEYWORD};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[derive(Clone)]
pub(crate) struct ForkLogger {
    steps: Arc<AtomicU8>,
    fork_name: &'static str,
    banner_label: &'static str,
    module_name: &'static str,
    show_ascii_art: bool,
}

impl ForkLogger {
    pub fn new(module_name: &'static str, show_ascii_art: bool) -> Self {
        Self::new_with_fork("Toccata", "TOCCATA", module_name, show_ascii_art)
    }

    pub fn new_with_fork(
        fork_name: &'static str,
        banner_label: &'static str,
        module_name: &'static str,
        show_ascii_art: bool,
    ) -> Self {
        Self { steps: Arc::new(AtomicU8::new(Self::ACTIVATE)), fork_name, banner_label, module_name, show_ascii_art }
    }

    const ACTIVATE: u8 = 0;

    pub fn report_activation(&self) -> bool {
        if self.steps.compare_exchange(Self::ACTIVATE, Self::ACTIVATE + 1, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            if self.show_ascii_art {
                info!(target: FORK_KEYWORD,
                    r#"
 _____                  ___
|_   _|__   ___ ___ __ _| |_ __ _
  | |/ _ \ / __/ __/ _` | __/ _` |
  | | (_) | (_| (_| (_| | || (_| |
  |_|\___/ \___\___\__,_|\__\__,_|
                    {}
"#,
                    self.banner_label
                );
            }
            info!(target: FORK_KEYWORD, "[{}] Activated for {}", self.fork_name, self.module_name);
            true
        } else {
            false
        }
    }
}
