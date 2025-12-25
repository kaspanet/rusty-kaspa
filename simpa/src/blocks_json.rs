use std::fs::File;
use std::io::Write;

use flate2::{write::GzEncoder, Compression};

use kaspa_consensus::params::Params;
use kaspa_consensus::{consensus::Consensus, params::OverrideParams};
use kaspa_consensus_core::{api::ConsensusApi, block::Block};
use kaspa_testing_integration::common::json::JtfBlock;

use crate::topologically_ordered_hashes;

pub(crate) fn write_blocks_json(params: &Params, consensus: &Consensus, file_path: &str) {
    let file = File::create(file_path).unwrap();
    let mut writer = GzEncoder::new(file, Compression::default());
    write_kaspad_params(params, &mut writer);
    let hashes = topologically_ordered_hashes(consensus, consensus.get_retention_period_root(), true);
    for hash in hashes {
        let block = consensus.get_block(hash).unwrap();
        write_block_json(block, &mut writer)
    }
    writer.finish().unwrap();
}

fn write_kaspad_params<W: Write>(params: &Params, writer: &mut W) {
    let override_params: OverrideParams = params.clone().into();
    serde_json::to_writer(&mut *writer, &override_params).unwrap();
    writer.write_all(b"\n").unwrap();
}

fn write_block_json<W: Write>(block: Block, writer: &mut W) {
    let rpc_block = (&block).into();
    write_jtf_block_json(rpc_block, writer);
}

fn write_jtf_block_json<W: Write>(block: JtfBlock, writer: &mut W) {
    serde_json::to_writer(&mut *writer, &block).unwrap();
    writer.write_all(b"\n").unwrap();
}
