use crate::imports::*;
use std::time::{SystemTime, UNIX_EPOCH};
use kaspa_wallet_core::tx::{Fees, PaymentDestination, PaymentOutput};
use std::ops::Mul;
use kaspa_wallet_core::tx::generator::GeneratorSummary;
use workflow_core::abortable::Abortable;

#[derive(Default, Handler)]
#[help("Create an unsigned transaction that can be signed later")]
pub struct CreateUnsignedTx;

impl CreateUnsignedTx {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let account = ctx.wallet().account()?;
        
        // Get destination address and amount from arguments
        if argv.len() < 2 {
            return Err(Error::Custom("Please provide destination address and amount in KAS".into()));
        }
        
        let address = Address::constructor(&argv[0]);
        let amount: u64 = argv[1].parse::<f64>()
            .map_err(|e| Error::Custom(format!("Invalid amount: {}", e)))?
            .mul(100_000_000.0) as u64; // Convert KAS to Sompi
        
        // Create a payment destination
        let destination = PaymentDestination::from(PaymentOutput::new(address, amount));
        
        // Create an unsigned transaction with minimal fee
        let fees = Fees::from(1000u64); // Minimal fee in Sompi
        let summary = account.estimate(destination.clone(), fees.clone(), None, &Abortable::default()).await?;
        
        // Save the transaction details to a file
        let tx_details = TransactionDetails {
            destination,
            fees,
            summary: summary.clone(),
        };
        
        let tx_json = serde_json::to_string_pretty(&tx_details)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Custom("System time is before Unix epoch".into()))?
            .as_secs();
        let filename = format!("unsigned_tx_{}.json", timestamp);
        std::fs::write(&filename, tx_json).map_err(|e| Error::Custom(format!("Failed to write file: {}", e)))?;
        
        println!("Unsigned transaction details saved to: {}", filename);
        println!("Estimated fees: {} Sompi", summary.aggregated_fees());
        println!("You can now sign this transaction using the 'sign' command");
        
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct TransactionDetails {
    destination: PaymentDestination,
    fees: Fees,
    summary: GeneratorSummary,
}
