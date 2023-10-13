use std::time::{Duration, Instant};

use kaspa_core::info;

/// Minimum number of items to report
const REPORT_BATCH_GRANULARITY: usize = 500;
/// Maximum time to go without report
const REPORT_TIME_GRANULARITY: Duration = Duration::from_secs(2);

pub struct ProgressReporter<F: Fn(usize, i32)> {
    low_daa_score: u64,
    high_daa_score: u64,
    object_name: &'static str,
    last_reported_percent: i32,
    last_log_time: Instant,
    current_batch: usize,
    processed: usize,

    report_callback: Option<F>,
}

impl<F: Fn(usize, i32)> ProgressReporter<F> {
    pub fn new(low_daa_score: u64, mut high_daa_score: u64, object_name: &'static str, cb: Option<F>) -> Self {
        if high_daa_score <= low_daa_score {
            // Avoid a zero or negative diff
            high_daa_score = low_daa_score + 1;
        }
        Self {
            low_daa_score,
            high_daa_score,
            object_name,
            last_reported_percent: 0,
            last_log_time: Instant::now(),
            current_batch: 0,
            processed: 0,
            report_callback: cb,
        }
    }

    pub fn report(&mut self, processed_delta: usize, current_daa_score: u64) {
        self.current_batch += processed_delta;
        let now = Instant::now();
        if now - self.last_log_time < REPORT_TIME_GRANULARITY && self.current_batch < REPORT_BATCH_GRANULARITY && self.processed > 0 {
            return;
        }
        self.processed += self.current_batch;
        self.current_batch = 0;
        if current_daa_score > self.high_daa_score {
            self.high_daa_score = current_daa_score + 1; // + 1 for keeping it at 99%
        }
        let relative_daa_score = if current_daa_score > self.low_daa_score { current_daa_score - self.low_daa_score } else { 0 };
        let percent = ((relative_daa_score as f64 / (self.high_daa_score - self.low_daa_score) as f64) * 100.0) as i32;
        if percent > self.last_reported_percent {
            info!("IBD: Processed {} {} ({}%)", self.processed, self.object_name, percent);
            if let Some(cb) = &self.report_callback {
                cb(self.processed, percent)
            }
            self.last_reported_percent = percent;
        }
        self.last_log_time = now;
    }

    pub fn report_completion(mut self, processed_delta: usize) {
        self.processed += self.current_batch + processed_delta;
        info!("IBD: Processed {} {} (100%)", self.processed, self.object_name);
        if let Some(cb) = &self.report_callback {
            cb(self.processed, 100)
        }
    }
}
