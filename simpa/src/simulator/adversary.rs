//! Adversarial mining scenarios for the Simpa simulator (issue #1112 / DK-208).
//!
//! Honest Simpa runs with a uniform broadcast delay produce almost no DAG
//! conflicts, so DAGKnight's UMC cascade never runs and the DK counters stay ~0.
//! The scenarios in this module deterministically manufacture the conflict
//! shapes the convergence proof reasons about, so that each run can assert (via
//! `Consensus::dagknight_counters()`) that the intended consensus flow was
//! actually exercised.
//!
//! All schedules (release times, burst windows, difficulty phases) are derived
//! from the simulation `--seed` and the simulated timeline, so a
//! `(scenario, seed)` pair is fully reproducible.

use kaspa_consensus_core::block::Block;

/// The selectable adversarial scenarios. Each maps to one or more DK counters
/// it is expected to move (see the strategy -> counter map in the design doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// Adversary mines privately (insert-local, no broadcast), accumulates a
    /// hidden side-DAG and releases it in one burst at a scheduled sim-time.
    /// Expect: total_calls>0, total_cascade_flips>0, total_voting_blocks>0.
    WithheldSideDag,
    /// Adversary broadcasts its own blocks with a large extra delay during a
    /// window [t0,t1], creating temporary parallel subgroups.
    /// Expect: cascade total_calls>0.
    LatencyBurst,
    /// Two (or more) adversaries build symmetric private forks and release them
    /// at the same tick, producing symmetric competing subgroups.
    /// Expect: max_cascade_flips large.
    EqualRankTies,
    /// Many short independent conflicts at fresh points: the cascade rarely
    /// reuses a persisted checkpoint.
    /// Expect: checkpoint_from_scratch dominates.
    WeakShortcut,
    /// One sustained conflict zone that is repeatedly re-cascaded as the
    /// adversary extends the same fork, so the cascade reloads its checkpoint.
    /// Expect: checkpoint_from_checkpoint dominates.
    StrongShortcut,
    /// Force blocks to flip blue/red across a context switch; run under
    /// `--features baseline-debugging` so the paper baseline is cross-checked.
    /// Expect: baseline path exercised (total_calls>0), no panic == agreement.
    GrayContextChange,
    /// Vary the adversary mining rate over the run to create high-rank bursts.
    /// Expect: deeper cascades (total_cascade_flips>0, larger max_cascade_flips).
    VariableDifficulty,
}

impl Scenario {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "withheld-side-dag" => Scenario::WithheldSideDag,
            "latency-burst" => Scenario::LatencyBurst,
            "equal-rank-ties" => Scenario::EqualRankTies,
            "weak-shortcut" => Scenario::WeakShortcut,
            "strong-shortcut" => Scenario::StrongShortcut,
            "gray-context-change" => Scenario::GrayContextChange,
            "variable-difficulty" => Scenario::VariableDifficulty,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Scenario::WithheldSideDag => "withheld-side-dag",
            Scenario::LatencyBurst => "latency-burst",
            Scenario::EqualRankTies => "equal-rank-ties",
            Scenario::WeakShortcut => "weak-shortcut",
            Scenario::StrongShortcut => "strong-shortcut",
            Scenario::GrayContextChange => "gray-context-change",
            Scenario::VariableDifficulty => "variable-difficulty",
        }
    }

    pub const ALL: [Scenario; 7] = [
        Scenario::WithheldSideDag,
        Scenario::LatencyBurst,
        Scenario::EqualRankTies,
        Scenario::WeakShortcut,
        Scenario::StrongShortcut,
        Scenario::GrayContextChange,
        Scenario::VariableDifficulty,
    ];

    /// Withhold-family scenarios keep a *pure* private fork by ignoring incoming
    /// honest blocks (so `build_block_template` selects only the adversary's own
    /// private tips as parents). Only `latency-burst` builds on the honest chain.
    pub fn ignores_incoming(&self) -> bool {
        !matches!(self, Scenario::LatencyBurst)
    }

    /// Default number of adversary miners for the scenario (can be overridden).
    pub fn default_adv_miners(&self) -> u64 {
        match self {
            Scenario::EqualRankTies => 2,
            _ => 1,
        }
    }
}

/// Deterministic parameters shared by every adversary miner in a run.
#[derive(Clone, Debug)]
pub struct AdversaryParams {
    /// Total number of miners in the simulation.
    pub num_miners: u64,
    /// Number of adversary miners (the highest-id miners are the adversaries so
    /// that miner 0 — whose consensus the counters are read from — stays honest
    /// and experiences the manufactured conflict).
    pub adv_miners: u64,
    /// Simulation start time (== genesis timestamp; `Environment::now()` is
    /// absolute, offset by this).
    pub start_time: u64,
    /// Simulated duration in milliseconds.
    pub duration_ms: u64,
    /// Base network delay in milliseconds (used to stagger targeted sends so
    /// parents are always delivered before their children).
    pub base_delay_ms: u64,
    /// Fraction of the duration the adversary withholds before its burst release.
    pub release_frac: f64,
    /// Latency-burst window as fractions of the duration.
    pub burst_frac: (f64, f64),
    /// Extra delay (ms) applied to adversary broadcasts during a latency burst.
    pub burst_delay_ms: u64,
    /// Deterministic seed (mirrors `--seed`).
    pub seed: u64,
}

impl AdversaryParams {
    /// Absolute sim-time at which the withheld buffer is released.
    pub fn release_at(&self) -> u64 {
        self.start_time + (self.duration_ms as f64 * self.release_frac) as u64
    }

    /// Absolute [t0, t1] latency-burst window.
    pub fn burst_window(&self) -> (u64, u64) {
        (
            self.start_time + (self.duration_ms as f64 * self.burst_frac.0) as u64,
            self.start_time + (self.duration_ms as f64 * self.burst_frac.1) as u64,
        )
    }

    /// The highest `adv_miners` ids are adversaries.
    pub fn is_adversary(&self, id: u64) -> bool {
        self.adv_miners > 0 && id >= self.num_miners.saturating_sub(self.adv_miners)
    }

    /// Iterator over every peer id except `self_id` (targets for released blocks).
    pub fn peers_except(&self, self_id: u64) -> impl Iterator<Item = u64> + '_ {
        (0..self.num_miners).filter(move |&p| p != self_id)
    }
}

/// A concrete plan (scenario + parameters) handed to the adversary miners.
#[derive(Clone, Debug)]
pub struct AdversaryPlan {
    pub scenario: Scenario,
    pub params: AdversaryParams,
}

/// Per-miner mutable adversary state (only present for adversary miners).
pub struct AdversaryRuntime {
    pub plan: AdversaryPlan,
    /// Whether the withheld buffer has already been flushed.
    pub released: bool,
    /// Privately mined, not-yet-released blocks (in mining/topological order).
    pub buffer: Vec<Block>,
    /// Monotonic counter used to stagger targeted sends (parent-before-child).
    pub send_seq: u64,
    /// Number of completed withhold/release cycles (weak-shortcut).
    pub cycle_count: u64,
}

impl AdversaryRuntime {
    pub fn new(plan: AdversaryPlan) -> Self {
        Self { plan, released: false, buffer: Vec::new(), send_seq: 0, cycle_count: 0 }
    }

    pub fn scenario(&self) -> Scenario {
        self.plan.scenario
    }
}
