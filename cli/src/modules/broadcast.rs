use crate::imports::*;
use crate::modules::serializable::SerializablePendingTransactions;
use kaspa_wallet_core::tx::generator::GeneratorSummary;
use serde::{Deserialize, Serialize};

#[derive(Default, Handler)]
#[help("Broadcast a signed transaction to the network")]
pub struct Broadcast;

impl Broadcast {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        
        // Get the signed transaction file path from arguments
        if argv.is_empty() {
            return Err(Error::Custom("Please provide the path to the signed transaction file".into()));
        }
        let tx_file = &argv[0];
        
        // Read and parse the signed transaction
        let tx_json = std::fs::read_to_string(tx_file)
            .map_err(|e| Error::Custom(format!("Failed to read file: {}", e)))?;
        let signed_tx: SignedTransactionResult = serde_json::from_str(&tx_json)?;
        
        // Submit each transaction to the network
        let mut tx_ids = vec![];
        for transaction in signed_tx.transactions.transactions {
            let tx_id = ctx.wallet().rpc_api().submit_transaction((&transaction.transaction).into(), false).await?;
            tx_ids.push(tx_id);
        }
        
        println!("Successfully broadcasted transactions:");
        for id in &tx_ids {
            println!("  {}", id);
        }
        
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct SignedTransactionResult {
    summary: GeneratorSummary,
    transactions: SerializablePendingTransactions,
}
