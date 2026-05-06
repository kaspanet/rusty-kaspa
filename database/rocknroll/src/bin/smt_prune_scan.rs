use std::{
    io::{self, Write},
    process::ExitCode,
};

use clap::Parser;
use kaspa_consensus::model::stores::{
    headers::{DbHeadersStore, HeaderStoreReader},
    pruning::{DbPruningStore, PruningStoreReader},
};
use kaspa_consensus::{config::ConfigBuilder, params::Params};
use kaspa_database::prelude::CachePolicy;
use kaspa_smt_store::processor::{SmtStores, StaleSmtEntriesCount};
use rocknroll::{
    Result,
    args::DbSourceArgs,
    db::{ResolvedConsensusDb, resolve_consensus_db},
};

#[derive(Parser, Debug)]
#[command(about = "Scan a consensus DB for stale SMT pruning entries")]
struct Args {
    #[command(flatten)]
    db: DbSourceArgs,

    #[arg(long, value_name = "BLUE_SCORE", help = "Override the pruning cutoff blue score")]
    cutoff_blue_score: Option<u64>,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    let args = Args::parse();
    let resolved = resolve_consensus_db(&args.db)?;
    print_db_header(&resolved);
    println!("archival: {}", resolved.archival.map_or("unknown".to_string(), |value| value.to_string()));

    if resolved.archival == Some(true) {
        print_archival_skip();
        return Ok(ExitCode::SUCCESS);
    }

    let db = resolved.open_consensus_readonly(args.db.files_limit)?;
    let pruning_store = DbPruningStore::new(db.clone());
    let pruning_point = pruning_store.pruning_point()?;
    let retention_period_root = pruning_store.retention_period_root()?;
    let retention_checkpoint = pruning_store.retention_checkpoint()?;
    println!("pruning_point: {pruning_point}");
    println!("retention_period_root: {retention_period_root}");
    println!("retention_checkpoint: {retention_checkpoint}");

    if retention_checkpoint != retention_period_root {
        print_unstable_pruning();
        return Ok(ExitCode::from(2));
    }
    println!("stable: true");

    let headers_store = DbHeadersStore::new(db.clone(), CachePolicy::Empty, CachePolicy::Empty);
    let pruning_point_blue_score = headers_store.get_blue_score(pruning_point)?;
    let params: Params = resolved.network.into();
    let config = ConfigBuilder::new(params).adjust_perf_params_to_consensus_params().build();
    let finality_depth = config.params.finality_depth();
    let cutoff_blue_score = args.cutoff_blue_score.unwrap_or_else(|| inclusive_prune_cutoff(pruning_point_blue_score, finality_depth));
    println!("pruning_point_blue_score: {pruning_point_blue_score}");
    println!("finality_depth: {finality_depth}");
    println!("cutoff_blue_score: {cutoff_blue_score}");
    println!("scanning...");
    let _ = io::stdout().flush();

    let smt_stores = SmtStores::new(db, 1, 1);
    let stale = smt_stores.count_entries_at_or_below(cutoff_blue_score)?;
    print_counts(stale);

    Ok(ExitCode::SUCCESS)
}

fn inclusive_prune_cutoff(blue_score: u64, finality_depth: u64) -> u64 {
    blue_score.saturating_sub(finality_depth).saturating_sub(1)
}

fn print_archival_skip() {
    println!("scan: skipped");
    println!("reason: archival nodes do not prune SMT history");
}

fn print_unstable_pruning() {
    println!("stable: false");
    println!("scan: skipped");
    println!("reason: retention checkpoint differs from retention period root, so pruning has not completed cleanly");
}

fn print_counts(stale: StaleSmtEntriesCount) {
    println!("branch_versions: {}", stale.branch_versions);
    println!("lane_versions: {}", stale.lane_versions);
    println!("score_index: {}", stale.score_index);
    println!("total: {}", stale.total());
}

fn print_db_header(resolved: &ResolvedConsensusDb) {
    println!("network: {}", resolved.network);
    if let Some(app_dir) = resolved.app_dir.as_ref() {
        println!("app_dir: {}", app_dir.display());
    }
    if let Some(meta_db_path) = resolved.meta_db_path.as_ref() {
        println!("meta_db: {}", meta_db_path.display());
    }
    if let Some(active_consensus_dir) = resolved.active_consensus_dir.as_ref() {
        println!("active_consensus_dir: {active_consensus_dir}");
    }
    println!("consensus_db: {}", resolved.consensus_db_path.display());
}
