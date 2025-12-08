use super::rocksdb_preset::RocksDbPreset;
use crate::db::DB;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::{path::PathBuf, sync::Arc};

#[derive(Debug)]
pub struct Unspecified;

#[derive(Debug)]
pub struct ConnBuilder<Path, const STATS_ENABLED: bool, StatsPeriod, FDLimit> {
    db_path: Path,
    create_if_missing: bool,
    parallelism: usize,
    files_limit: FDLimit,
    mem_budget: usize,
    stats_period: StatsPeriod,
    preset: RocksDbPreset,
    wal_dir: Option<PathBuf>,
    cache_budget: Option<usize>,
}

impl Default for ConnBuilder<Unspecified, false, Unspecified, Unspecified> {
    fn default() -> Self {
        ConnBuilder {
            db_path: Unspecified,
            create_if_missing: true,
            parallelism: 1,
            mem_budget: 64 * 1024 * 1024,
            stats_period: Unspecified,
            files_limit: Unspecified,
            preset: RocksDbPreset::Default,
            wal_dir: None,
            cache_budget: None,
        }
    }
}

impl<Path, const STATS_ENABLED: bool, StatsPeriod, FDLimit> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit> {
    pub fn with_db_path(self, db_path: PathBuf) -> ConnBuilder<PathBuf, STATS_ENABLED, StatsPeriod, FDLimit> {
        ConnBuilder {
            db_path,
            files_limit: self.files_limit,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
            preset: self.preset,
            wal_dir: self.wal_dir,
            cache_budget: self.cache_budget,
        }
    }
    pub fn with_create_if_missing(self, create_if_missing: bool) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit> {
        ConnBuilder { create_if_missing, ..self }
    }
    pub fn with_parallelism(self, parallelism: impl Into<usize>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit> {
        ConnBuilder { parallelism: parallelism.into(), ..self }
    }
    pub fn with_mem_budget(self, mem_budget: impl Into<usize>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit> {
        ConnBuilder { mem_budget: mem_budget.into(), ..self }
    }
    pub fn with_files_limit(self, files_limit: impl Into<i32>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, i32> {
        ConnBuilder {
            db_path: self.db_path,
            files_limit: files_limit.into(),
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
            preset: self.preset,
            wal_dir: self.wal_dir,
            cache_budget: self.cache_budget,
        }
    }
    pub fn with_preset(self, preset: RocksDbPreset) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit> {
        ConnBuilder { preset, ..self }
    }
    pub fn with_wal_dir(self, wal_dir: Option<PathBuf>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit> {
        ConnBuilder { wal_dir, ..self }
    }
    pub fn with_cache_budget(self, cache_budget: Option<usize>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit> {
        ConnBuilder { cache_budget, ..self }
    }
}

impl<Path, FDLimit> ConnBuilder<Path, false, Unspecified, FDLimit> {
    pub fn enable_stats(self) -> ConnBuilder<Path, true, Unspecified, FDLimit> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
            preset: self.preset,
            wal_dir: self.wal_dir,
            cache_budget: self.cache_budget,
        }
    }
}

impl<Path, StatsPeriod, FDLimit> ConnBuilder<Path, true, StatsPeriod, FDLimit> {
    pub fn disable_stats(self) -> ConnBuilder<Path, false, Unspecified, FDLimit> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: Unspecified,
            preset: self.preset,
            wal_dir: self.wal_dir,
            cache_budget: self.cache_budget,
        }
    }
    pub fn with_stats_period(self, stats_period: impl Into<u32>) -> ConnBuilder<Path, true, u32, FDLimit> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: stats_period.into(),
            preset: self.preset,
            wal_dir: self.wal_dir,
            cache_budget: self.cache_budget,
        }
    }
}

macro_rules! default_opts {
    ($self: expr) => {{
        let mut opts = rocksdb::Options::default();

        // Apply the preset configuration (includes parallelism and compaction settings)
        $self.preset.apply_to_options(&mut opts, $self.parallelism, $self.mem_budget, $self.cache_budget);

        // Configure WAL directory if specified (for RAM cache / tmpfs)
        // Auto-generate unique subdirectory from database path to avoid conflicts
        if let Some(ref wal_base) = $self.wal_dir {
            let db_name = $self
                .db_path
                .file_name()
                .and_then(|n| n.to_str())
                .expect(&format!("Invalid database path: {}", $self.db_path.display()));
            let wal_subdir = wal_base.join(db_name);

            // Create subdirectory if needed (each DB gets its own WAL space)
            std::fs::create_dir_all(&wal_subdir).expect(&format!(
                "Failed to create WAL subdirectory {}: {}",
                wal_subdir.display(),
                "error"
            ));

            opts.set_wal_dir(&wal_subdir);
        }

        let guard = kaspa_utils::fd_budget::acquire_guard($self.files_limit)?;
        opts.set_max_open_files($self.files_limit);
        opts.create_if_missing($self.create_if_missing);
        Ok((opts, guard))
    }};
}

impl ConnBuilder<PathBuf, false, Unspecified, i32> {
    pub fn build(self) -> Result<Arc<DB>, kaspa_utils::fd_budget::Error> {
        let (opts, guard) = default_opts!(self)?;
        let db = Arc::new(DB::new(<DBWithThreadMode<MultiThreaded>>::open(&opts, self.db_path.to_str().unwrap()).unwrap(), guard));
        Ok(db)
    }
}

impl ConnBuilder<PathBuf, true, Unspecified, i32> {
    pub fn build(self) -> Result<Arc<DB>, kaspa_utils::fd_budget::Error> {
        let (mut opts, guard) = default_opts!(self)?;
        opts.enable_statistics();
        let db = Arc::new(DB::new(<DBWithThreadMode<MultiThreaded>>::open(&opts, self.db_path.to_str().unwrap()).unwrap(), guard));
        Ok(db)
    }
}

impl ConnBuilder<PathBuf, true, u32, i32> {
    pub fn build(self) -> Result<Arc<DB>, kaspa_utils::fd_budget::Error> {
        let (mut opts, guard) = default_opts!(self)?;
        opts.enable_statistics();
        opts.set_report_bg_io_stats(true);
        opts.set_stats_dump_period_sec(self.stats_period);
        let db = Arc::new(DB::new(<DBWithThreadMode<MultiThreaded>>::open(&opts, self.db_path.to_str().unwrap()).unwrap(), guard));
        Ok(db)
    }
}
