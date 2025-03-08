use crate::db::DB;
use rocksdb::{BlockBasedOptions, Cache, DBCompressionType, DBWithThreadMode, MultiThreaded};
use std::{path::PathBuf, sync::Arc, thread::available_parallelism};

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const GB: usize = 1024 * MB;

#[derive(Debug)]
pub struct Unspecified;

#[derive(Debug)]
pub struct ConnBuilder<Path, const STATS_ENABLED: bool, StatsPeriod, FDLimit, CacheSize> {
    db_path: Path,
    create_if_missing: bool,
    parallelism: usize,
    files_limit: FDLimit,
    mem_budget: usize,
    stats_period: StatsPeriod,
    cache_size: CacheSize,
}

impl Default for ConnBuilder<Unspecified, true, Unspecified, Unspecified, Unspecified> {
    fn default() -> Self {
        ConnBuilder {
            db_path: Unspecified,
            create_if_missing: true,
            parallelism: 1,
            mem_budget: 64 * 1024 * 1024,
            stats_period: Unspecified,
            files_limit: Unspecified,
            cache_size: Unspecified,
        }
    }
}

impl<Path, const STATS_ENABLED: bool, StatsPeriod, FDLimit, CacheSize>
    ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit, CacheSize>
{
    pub fn with_db_path(self, db_path: PathBuf) -> ConnBuilder<PathBuf, STATS_ENABLED, StatsPeriod, FDLimit, CacheSize> {
        ConnBuilder {
            db_path,
            files_limit: self.files_limit,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
            cache_size: self.cache_size,
        }
    }
    pub fn with_create_if_missing(self, create_if_missing: bool) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit, CacheSize> {
        ConnBuilder { create_if_missing, ..self }
    }
    pub fn with_parallelism(self, parallelism: impl Into<usize>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit, CacheSize> {
        ConnBuilder { parallelism: parallelism.into(), ..self }
    }
    pub fn with_mem_budget(self, mem_budget: impl Into<usize>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, FDLimit, CacheSize> {
        ConnBuilder { mem_budget: mem_budget.into(), ..self }
    }
    pub fn with_files_limit(self, files_limit: impl Into<i32>) -> ConnBuilder<Path, STATS_ENABLED, StatsPeriod, i32, CacheSize> {
        ConnBuilder {
            db_path: self.db_path,
            files_limit: files_limit.into(),
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
            cache_size: self.cache_size,
        }
    }
    pub fn disable_stats(self) -> ConnBuilder<Path, false, Unspecified, FDLimit, CacheSize> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: Unspecified,
            cache_size: self.cache_size,
        }
    }
}

impl<Path, const STATS_ENABLED: bool, FDLimit, CacheSize> ConnBuilder<Path, STATS_ENABLED, Unspecified, FDLimit, CacheSize> {
    pub fn enable_stats(self) -> ConnBuilder<Path, true, Unspecified, FDLimit, CacheSize> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
            cache_size: self.cache_size,
        }
    }
}

impl<Path, StatsPeriod, FDLimit, CacheSize> ConnBuilder<Path, true, StatsPeriod, FDLimit, CacheSize> {
    pub fn with_stats_period(self, stats_period: impl Into<u32>) -> ConnBuilder<Path, true, u32, FDLimit, CacheSize> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: stats_period.into(),
            cache_size: self.cache_size,
        }
    }
}

impl<Path, StatsPeriod, FDLimit, CacheSize> ConnBuilder<Path, true, StatsPeriod, FDLimit, CacheSize> {
    pub fn with_cache_size(self, cache_size: usize) -> ConnBuilder<Path, true, StatsPeriod, FDLimit, usize> {
        ConnBuilder {
            db_path: self.db_path,
            create_if_missing: self.create_if_missing,
            parallelism: self.parallelism,
            files_limit: self.files_limit,
            mem_budget: self.mem_budget,
            stats_period: self.stats_period,
            cache_size,
        }
    }
}

macro_rules! default_opts {
    ($self: expr) => {{
        let mut opts = rocksdb::Options::default();
        if $self.parallelism > 1 {
            opts.increase_parallelism($self.parallelism as i32);
        }

        {
            // based on optimized by default
            opts.set_max_background_jobs((available_parallelism().unwrap().get() / 2) as i32);
            opts.set_compaction_readahead_size(2 * MB);
            opts.set_level_zero_stop_writes_trigger(36);
            opts.set_level_zero_slowdown_writes_trigger(20);
            opts.set_max_compaction_bytes(2 * MB as u64 * 100);
        }
        {
            let buffer_size = 32usize * MB;
            let min_to_merge = 4i32;
            let max_buffers = 16;
            let trigger = 12i32;
            let max_level_base = min_to_merge as u64 * trigger as u64 * buffer_size as u64;

            opts.set_target_file_size_base(32 * MB as u64);
            opts.set_max_bytes_for_level_multiplier(4.0);

            opts.set_write_buffer_size(buffer_size);
            opts.set_max_write_buffer_number(max_buffers);
            opts.set_min_write_buffer_number_to_merge(min_to_merge);
            opts.set_level_zero_file_num_compaction_trigger(trigger);
            opts.set_max_bytes_for_level_base(max_level_base);

            opts.set_wal_bytes_per_sync(1000 * KB as u64); // suggested by advisor
            opts.set_use_direct_io_for_flush_and_compaction(true); // should decrease write amp
            opts.set_keep_log_file_num(1); // good for analytics
            opts.set_bytes_per_sync(MB as u64);
            opts.set_max_total_wal_size(GB as u64);
            opts.set_compression_per_level(&[
                DBCompressionType::None,
                DBCompressionType::Lz4,
                DBCompressionType::Lz4,
                DBCompressionType::Lz4,
                DBCompressionType::Lz4,
                DBCompressionType::Lz4,
                DBCompressionType::Lz4,
            ]);
            opts.set_level_compaction_dynamic_level_bytes(true); // default option since 8.4 https://github.com/facebook/rocksdb/wiki/Leveled-Compaction#level_compaction_dynamic_level_bytes-is-true-recommended-default-since-version-84

            // Create BlockBasedOptions and set block size
            let mut block_opts = rocksdb::BlockBasedOptions::default();
            block_opts.set_bloom_filter(4.9, true); // increases ram, trade-off, needs to be tested
            block_opts.set_block_size(128 * KB);
            $self.cache_size.apply_cache_size(&mut block_opts);
            opts.set_block_based_table_factory(&block_opts);
        }
        let guard = kaspa_utils::fd_budget::acquire_guard($self.files_limit)?;
        opts.set_max_open_files($self.files_limit);
        opts.create_if_missing($self.create_if_missing);
        Ok((opts, guard))
    }};
}

impl<T: ApplyCacheSize> ConnBuilder<PathBuf, false, Unspecified, i32, T> {
    pub fn build(self) -> Result<Arc<DB>, kaspa_utils::fd_budget::Error> {
        let (opts, guard) = default_opts!(self)?;
        let db = Arc::new(DB::new(<DBWithThreadMode<MultiThreaded>>::open(&opts, self.db_path.to_str().unwrap()).unwrap(), guard));
        Ok(db)
    }
}

impl<T: ApplyCacheSize> ConnBuilder<PathBuf, true, Unspecified, i32, T> {
    pub fn build(self) -> Result<Arc<DB>, kaspa_utils::fd_budget::Error> {
        let (mut opts, guard) = default_opts!(self)?;
        opts.enable_statistics();
        opts.set_report_bg_io_stats(true);
        let db = Arc::new(DB::new(<DBWithThreadMode<MultiThreaded>>::open(&opts, self.db_path.to_str().unwrap()).unwrap(), guard));
        Ok(db)
    }
}

impl<T: ApplyCacheSize> ConnBuilder<PathBuf, true, u32, i32, T> {
    pub fn build(self) -> Result<Arc<DB>, kaspa_utils::fd_budget::Error> {
        let (mut opts, guard) = default_opts!(self)?;
        opts.enable_statistics();
        opts.set_report_bg_io_stats(true);
        opts.set_stats_dump_period_sec(self.stats_period);
        let db = Arc::new(DB::new(<DBWithThreadMode<MultiThreaded>>::open(&opts, self.db_path.to_str().unwrap()).unwrap(), guard));
        Ok(db)
    }
}

pub trait ApplyCacheSize {
    fn apply_cache_size(&self, options: &mut BlockBasedOptions);
}

impl ApplyCacheSize for usize {
    fn apply_cache_size(&self, options: &mut BlockBasedOptions) {
        let cache = Cache::new_lru_cache(*self);
        options.set_block_cache(&cache);
    }
}

impl ApplyCacheSize for Unspecified {
    fn apply_cache_size(&self, _options: &mut BlockBasedOptions) {}
}
