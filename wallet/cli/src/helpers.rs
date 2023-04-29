use crate::result::Result;
use kaspa_wallet_core::{account::Account, Wallet};
use std::sync::Arc;
use workflow_terminal::Terminal;

pub async fn ask_mnemonic(term: &Arc<Terminal>) -> Result<Vec<String>> {
    let mut words: Vec<String> = vec![];
    loop {
        if words.is_empty() {
            term.writeln("Please enter mnemonic (12 or 24 words)");
        } else if words.len() < 12 {
            let remains_for_12 = 12 - words.len();
            let remains_for_24 = 24 - words.len();
            term.writeln(&format!("Please enter additional {} or {} words or <enter> to abort", remains_for_12, remains_for_24));
        } else {
            let remains_for_24 = 24 - words.len();
            term.writeln(&format!("Please enter additional {} words or <enter> to abort", remains_for_24));
        }
        let text = term.ask(false, "Words:").await?;
        let list = text.split_whitespace().map(|s| s.to_string()).collect::<Vec<String>>();
        if list.is_empty() {
            return Err("User abort".into());
        }
        words.extend(list);

        if words.len() > 24 || words.len() == 12 || words.len() == 24 {
            break;
        }
    }

    if words.len() > 24 {
        Err("Mnemonic must be 12 or 24 words".into())
    } else {
        Ok(words)
    }
}

pub fn kas_str_to_sompi(amount: &str) -> Result<u64> {
    let amount = amount.trim();
    if amount.is_empty() {
        return Ok(0);
    }

    let mut amount = amount.parse::<f64>()?;
    amount *= kaspa_consensus_core::constants::SOMPI_PER_KASPA as f64;
    Ok(amount as u64)
}

#[allow(dead_code)]
pub async fn ask_account(term: &Arc<Terminal>, wallet: &Wallet) -> Result<Option<Arc<Account>>> {
    let accounts = wallet.accounts();

    for (index, account) in accounts.iter().enumerate() {
        term.writeln(format!("{index}) {}", account.name()));
    }

    let selection = term.ask(false, "Choose account:").await?;
    let index: usize = selection.parse()?;
    let acct = accounts.get(index).cloned();

    Ok(acct)
}
