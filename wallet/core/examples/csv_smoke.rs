// ...existing code...
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    // Use current working directory (avoids read-only /tmp in some environments)
    let mut out = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
     out.set_file_name(format!("kaspa-tx-sample-{}.csv", ts));
     let mut f = File::create(&out)?;
     let header = "transaction_id,timestamp,timestamp_unix_ms,block_daa_score,transaction_type,amount,amount_sompi,fees_sompi,fees,is_coinbase,network,note,utxo_count,addresses\n";
     f.write_all(header.as_bytes())?;
     let rows = vec![
         "a1b2c3...,2026-01-15T10:30:45.123Z,1736934645123,54321000,incoming,500.00000000,50000000000,, ,true,mainnet,\"Sample note, with comma\",1,kaspa:qr0...\n",
         "f6e5d4...,2026-01-16T14:22:10.456Z,1737034930456,54325000,outgoing,-123.45678901,12345678901,10000,0.00010000,false,mainnet,\"Payment for \"\"services\"\"\",3,\"kaspa:qr...,kaspa:qp...\"\n",
     ];
     for r in rows { f.write_all(r.as_bytes())?; }
     println!("Wrote sample CSV to: {}", out.display());
     Ok(())
 }
 // ...existing code...