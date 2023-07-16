use crate::imports::*;

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
pub mod halt;
pub mod help;
pub mod hint;
pub mod import;
pub mod info;
pub mod list;
pub mod metrics;
pub mod mute;
pub mod name;
pub mod network;
#[path = "new-address.rs"]
pub mod new_address;
pub mod node;
pub mod open;
pub mod ping;
pub mod reload;
pub mod select;
pub mod send;
pub mod server;
pub mod set;
pub mod sign;
pub mod sweep;
pub mod test;
pub mod track;
// pub mod error;

pub fn register_handlers(cli: &Arc<KaspaCli>) -> Result<()> {
    register_handlers!(
        cli,
        cli.handlers(),
        [
            address,
            // broadcast,
            close,
            connect,
            // create_unsigned_tx,
            create,
            details,
            disconnect,
            estimate,
            exit,
            export,
            halt,
            help,
            hint,
            import,
            info,
            list,
            metrics,
            mute,
            name,
            network,
            new_address,
            node,
            open,
            ping,
            reload,
            select,
            send,
            server,
            set,
            // sign,
            // sweep,
            track,
            test,
            // error,
        ]
    );

    if application_runtime::is_web() {
        register_handlers!(cli, cli.handlers(), [reload,]);
    }

    Ok(())
}
