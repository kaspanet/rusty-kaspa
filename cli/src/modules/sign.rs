use crate::imports::*;
use crate::modules::serializable::SerializablePendingTransactions;
use kaspa_wallet_core::tx::generator::GeneratorSummary;
use kaspa_wallet_core::tx::{Fees, Generator, GeneratorSettings, PaymentDestination, Signer};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use workflow_core::abortable::Abortable;

#[derive(Default, Handler)]
#[help("Sign the given unsigned transaction")]
pub struct Sign;

impl Sign {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let account = ctx.wallet().account()?;

        // Get the transaction file path and wallet secret from arguments
        if argv.len() < 2 {
            return Err(Error::Custom("Please provide the path to the unsigned transaction file and wallet secret".into()));
        }
        let tx_file = &argv[0];
        let (wallet_secret, _) = ctx.ask_wallet_secret(Some(&account)).await?;

        // Read and parse the unsigned transaction details
        let tx_json = std::fs::read_to_string(tx_file).map_err(|e| Error::Custom(format!("Failed to read file: {}", e)))?;
        let tx_details: TransactionDetails = serde_json::from_str(&tx_json)?;

        // Get the private key data for signing
        let keydata = account.prv_key_data(wallet_secret).await?;

        // Create a signer
        let signer = Arc::new(Signer::new(account.clone().as_dyn_arc(), keydata, None));

        // Create transaction settings
        let settings =
            GeneratorSettings::try_new_with_account(account.clone().as_dyn_arc(), tx_details.destination, tx_details.fees, None)?;

        // Create a generator with the signer
        let generator = Generator::try_new(settings, Some(signer), Some(&Abortable::default()))?;

        // Generate and sign the transaction
        let mut stream = generator.stream();
        let mut signed_txs = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transaction.try_sign()?;
            signed_txs.push(transaction);
        }

        // Convert to serializable format
        let serializable_txs = SerializablePendingTransactions::from_pending_transactions(&signed_txs);

        // Save the signed transactions to a file
        let result = SignedTransactionResult { summary: generator.summary(), transactions: serializable_txs };

        let tx_json = serde_json::to_string_pretty(&result)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Custom("System time is before Unix epoch".into()))?
            .as_secs();
        let filename = format!("signed_tx_{}.json", timestamp);
        std::fs::write(&filename, tx_json).map_err(|e| Error::Custom(format!("Failed to write file: {}", e)))?;

        println!("Signed transaction saved to: {}", filename);
        println!("You can now broadcast this transaction using the 'broadcast' command");

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct TransactionDetails {
    destination: PaymentDestination,
    fees: Fees,
    summary: GeneratorSummary,
}

#[derive(Serialize, Deserialize)]
struct SignedTransactionResult {
    summary: GeneratorSummary,
    transactions: SerializablePendingTransactions,
}
