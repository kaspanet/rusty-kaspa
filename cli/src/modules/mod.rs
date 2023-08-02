use crate::imports::*;

pub mod account;
pub mod address;
pub mod broadcast;
pub mod close;
pub mod connect;
pub mod create;
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
pub mod import;
pub mod list;
pub mod miner;
pub mod monitor;
pub mod mute;
pub mod name;
pub mod network;
pub mod node;
pub mod open;
pub mod ping;
pub mod reload;
pub mod rpc;
pub mod select;
pub mod send;
pub mod server;
pub mod set;
pub mod sign;
pub mod start;
pub mod stop;
pub mod sweep;
pub mod test;
pub mod theme;
pub mod track;
pub mod wallet;

// TODO
// broadcast
// create-unsigned-tx
// sign

pub fn register_handlers(cli: &Arc<KaspaCli>) -> Result<()> {
    register_handlers!(
        cli,
        cli.handlers(),
        [
            account, address, close, connect, create, details, disconnect, estimate, exit, export, guide, halt, help, history, import,
            rpc, list, miner, monitor, mute, name, network, node, open, ping, reload, select, send, server, set, start, stop, sweep,
            theme, track, test, wallet,
        ]
    );

    Ok(())
}
