use crate::imports::*;

pub mod account;
pub mod address;
pub mod broadcast;
pub mod close;
pub mod connect;
#[path = "create-unsigned-tx.rs"]
pub mod create_unsigned_tx;
pub mod details;
pub mod disconnect;
pub mod estimate;
pub mod exit;
pub mod export;
pub mod guide;
pub mod halt;
pub mod help;
pub mod history;
// pub mod import;
pub mod list;
pub mod message;
pub mod miner;
pub mod monitor;
pub mod mute;
pub mod network;
pub mod node;
pub mod open;
pub mod ping;
pub mod pskb;
pub mod reload;
pub mod rpc;
pub mod select;
pub mod send;
pub mod server;
pub mod settings;
pub mod sign;
pub mod start;
pub mod stop;
pub mod sweep;
// pub mod test;
pub mod theme;
pub mod track;
pub mod transfer;
pub mod wallet;

// this module is registered manually within
// applications that support metrics
pub mod metrics;

// TODO
// broadcast
// create-unsigned-tx
// sign

pub fn register_handlers(cli: &Arc<KaspaCli>) -> Result<()> {
    register_handlers!(
        cli,
        cli.handlers(),
        [
            account, address, close, connect, details, disconnect, estimate, exit, export, guide, help, history, rpc, list, miner,
            message, monitor, mute, network, node, open, ping, pskb, reload, select, send, server, settings, sweep, track, transfer,
            wallet,
            // halt,
            // theme,  start, stop
        ]
    );

    Ok(())
}
