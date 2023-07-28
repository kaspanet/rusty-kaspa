use crate::db::DB;
use rlimit::Resource;
use std::cmp::min;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Copy, Clone)]
pub struct Unspecified;

pub trait StatsPeriod: Clone {}

impl StatsPeriod for Unspecified {}
impl StatsPeriod for u32 {}

#[derive(Debug, Clone)]
pub struct ConnBuilder<Path: Clone, const STATS_ENABLED: bool, StatsPeriod: Clone> {
    db_path: Path,
    create_if_missing: bool,
    parallelism: usize,
    files_limit: i32,
    mem_budget: usize,
    stats_period: StatsPeriod,
}

impl Default for ConnBuilder<Unspecified, false, Unspecified> {
    fn default() -> Self {
        ConnBuilder {
            db_path: Unspecified,
            create_if_missing: true,
            parallelism: 1,
            files_limit: 500,
            mem_budget: 64 * 1024 * 1024,
            stats_period: Unspecified,
        }
    }
}

impl<Path: Clone, const STATS_ENABLED: bool, StatsPeriod: Clone> ConnBuilder<Path, STATS_ENABLED, StatsPeriod> {
    pub fn with_db_path(self, db_path: PathBuf) -> ConnBuilder<PathBuf, STATS_ENABLED, StatsPeriod> {
        ConnBuilder {
            db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
        }
    }
    pub fn with_create_if_missing(self, create_if_missing: bool) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
        }
    }
    pub fn with_parallelism(self, parallelism: impl Into<usize>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: parallelism.into(),
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
        }
    }
    pub fn with_files_limit(self, files_limit: impl Into<i32>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: files_limit.into(),
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
        }
    }
    pub fn with_mem_budget(self, mem_budget: impl Into<usize>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: mem_budget.into(),
            stats_period: self.stats_period,
        }
    }
}

impl<Path: Clone> ConnBuilder<Path, false, Unspecified> {
    pub fn enable_stats(self) -> ConnBuilder<Path, true, Unspecified> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
        }
    }
}

impl<Path: Clone, StatsPeriod: Clone> ConnBuilder<Path, true, StatsPeriod> {
    pub fn disable_stats(self) -> ConnBuilder<Path, false, Unspecified> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: Unspecified,
        }
    }
    pub fn with_stats_period(self, stats_period: impl Into<u32>) -> ConnBuilder<Path, true, u32> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: stats_period.into(),
        }
    }
}

macro_rules! default_opts {
    ($self: expr) => {{
        let mut opts = rocksdb::Options::default();
        if $self.parallelism > 1 {
            opts.increase_parallelism($self.parallelism as i32);
        }
        opts.optimize_level_style_compaction($self.mem_budget);

        #[cfg(target_os = "windows")]
        let files_limit = rlimit::getmaxstdio() as i32;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let files_limit = rlimit::getrlimit(Resource::NOFILE).unwrap().0 as i32;
        // In most linux environments the limit is set to 1024, so we use 500 to give sufficient slack.
        // TODO: fine-tune this parameter and additional parameters related to max file size
        opts.set_max_open_files(min(files_limit, $self.files_limit));
        opts.create_if_missing($self.create_if_missing);
        opts
    }};
}

impl<SP: StatsPeriod> ConnBuilder<PathBuf, false, SP> {
    pub fn build(self) -> Arc<DB> {
        let opts = default_opts!(self);
        let db = Arc::new(DB::open(&opts, self.db_path.to_str().unwrap()).unwrap());
        db
    }
}

impl ConnBuilder<PathBuf, true, Unspecified> {
    pub fn build(self) -> Arc<DB> {
        let mut opts = default_opts!(self);
        opts.enable_statistics();
        let db = Arc::new(DB::open(&opts, self.db_path.to_str().unwrap()).unwrap());
        db
    }
}

impl ConnBuilder<PathBuf, true, u32> {
    pub fn build(self) -> Arc<DB> {
        let mut opts = default_opts!(self);
        opts.enable_statistics();
        opts.set_report_bg_io_stats(true);
        opts.set_stats_dump_period_sec(self.stats_period);
        let db = Arc::new(DB::open(&opts, self.db_path.to_str().unwrap()).unwrap());
        db
    }
}
