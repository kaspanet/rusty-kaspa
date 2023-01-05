//TODO: these modules closely imitate stores in `consensus::model::stores::database`, perhaps move stores to seperate database package eventually. 
//TODO: `tips` is exact replica of `consensus::model::stores::tips`, see above.

mod utxos_by_script_public_key;
mod tips; 
mod circulating_supply;
pub mod store_manager;

use super::model;