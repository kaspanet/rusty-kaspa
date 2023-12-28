use kaspa_database::prelude::CachePolicy;
use kaspa_utils::mem_size::MemMode;
use rand::Rng;

/// Adds stochastic noise to cache sizes to avoid predictable and equal sizes across all network nodes
fn noise(size: usize, magnitude: usize) -> usize {
    if size == 0 {
        // no noise if original size is zero
        size
    } else {
        size + rand::thread_rng().gen_range(0..16) * magnitude
    }
}

/// Bounds the size according to the "memory budget" (represented in bytes) and the approximate size of each unit in bytes
fn bounded_size(desired_units: usize, memory_budget_bytes: usize, approx_unit_bytes: usize) -> usize {
    let max_size = memory_budget_bytes / approx_unit_bytes;
    usize::min(desired_units, max_size)
}

pub struct CachePolicyBuilder {
    bytes_budget: usize,
    max_items: usize,
    min_items: usize,
    unit_bytes: Option<usize>,
    tracked: bool,
    mem_mode: MemMode,
}

impl Default for CachePolicyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CachePolicyBuilder {
    pub fn new() -> Self {
        Self {
            bytes_budget: usize::MAX,
            max_items: usize::MAX,
            min_items: 0,
            unit_bytes: None,
            tracked: false,
            mem_mode: MemMode::Undefined,
        }
    }

    pub fn bytes_budget(mut self, bytes_budget: usize) -> Self {
        self.bytes_budget = bytes_budget;
        self
    }

    pub fn max_items(mut self, max_items: usize) -> Self {
        self.max_items = max_items;
        self
    }

    pub fn min_items(mut self, min_items: usize) -> Self {
        self.min_items = min_items;
        self
    }

    pub fn unit_bytes(mut self, unit_bytes: usize) -> Self {
        self.unit_bytes = Some(unit_bytes);
        self
    }

    /// Use [`CachePolicy::Count`] mode
    pub fn untracked(mut self) -> Self {
        self.tracked = false;
        self.mem_mode = MemMode::Undefined;
        self
    }

    /// Use [`CachePolicy::Tracked`] mode with [`MemMode::Units`] mem mode
    pub fn tracked_units(mut self) -> Self {
        self.tracked = true;
        self.mem_mode = MemMode::Units;
        self
    }

    /// Use [`CachePolicy::Tracked`] mode with [`MemMode::Bytes`] mem mode
    pub fn tracked_bytes(mut self) -> Self {
        self.tracked = true;
        self.mem_mode = MemMode::Bytes;
        self
    }

    /// Downscale the upper-bound constants by a factor of `2^level`
    pub fn downscale(&self, level: u8) -> Self {
        // Downscale both upper-bound limits unless they have the initial MAX value.
        // The calc is equal to downscaled = budget / 2^level
        let bytes_budget =
            if self.bytes_budget == usize::MAX { self.bytes_budget } else { self.bytes_budget.checked_shr(level as u32).unwrap_or(0) };
        let max_items =
            if self.max_items == usize::MAX { self.max_items } else { self.max_items.checked_shr(level as u32).unwrap_or(0) };
        Self { bytes_budget, max_items, ..*self }
    }

    pub fn build(&self) -> CachePolicy {
        assert!(self.max_items < usize::MAX || self.bytes_budget < usize::MAX, "max_items or bytes_budget are expected");

        if self.tracked {
            match self.mem_mode {
                MemMode::Bytes => {
                    assert!(self.max_items == usize::MAX, "max_items is not supported in tracked bytes mode");
                    CachePolicy::Tracked {
                        max_size: noise(self.bytes_budget, 512), // 0.5KB noise magnitude
                        min_items: noise(self.min_items, 1),
                        mem_mode: MemMode::Bytes,
                    }
                }
                MemMode::Units => {
                    let max_items = if self.bytes_budget == usize::MAX {
                        self.max_items
                    } else {
                        bounded_size(
                            self.max_items,
                            self.bytes_budget,
                            self.unit_bytes.expect("unit_bytes are expected with bytes_budget in units mem mode"),
                        )
                    };
                    CachePolicy::Tracked {
                        max_size: noise(max_items, 1),
                        min_items: noise(self.min_items, 1),
                        mem_mode: MemMode::Units,
                    }
                }
                MemMode::Undefined => panic!("tracked mode requires a defined mem mode"),
            }
        } else {
            let max_items = if self.bytes_budget == usize::MAX {
                self.max_items
            } else {
                bounded_size(
                    self.max_items,
                    self.bytes_budget,
                    self.unit_bytes.expect("unit_bytes are expected with bytes_budget in non-tracked mode"),
                )
            };
            CachePolicy::Count(noise(max_items.max(self.min_items), 1))
        }
    }
}
