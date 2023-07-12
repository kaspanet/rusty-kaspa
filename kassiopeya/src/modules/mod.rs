mod exit;

use crate::imports::*;

pub fn register_handlers(cli: &Arc<KaspaCli>) -> Result<()> {
    register_handlers!(cli, cli.handlers(), [exit]);

    Ok(())
}
