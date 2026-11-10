//! Link matchers for the terminal. Link matchers are used to match links in the terminal and perform
//! actions on them. Specifically we use them to open links in the browser and copy addresses, transaction
//! ids, and block hashes to the clipboard.

use crate::{imports::*, notifier::Notification};
use application_runtime::{is_nw, is_wasm};
use workflow_core::task::dispatch;
use workflow_dom::{clipboard, link};
use workflow_wasm::jserror::*;

/// Base URL of the block explorer, fixed at build time via `AERO_EXPLORER_URL` (no explorer links are opened if unset).
fn explorer_url(path: &str) -> Option<String> {
    option_env!("AERO_EXPLORER_URL").map(|base| format!("{}/{path}", base.trim_end_matches('/')))
}

pub fn register_link_matchers(cli: &Arc<AeroCli>) -> Result<()> {
    if !is_wasm() {
        return Ok(());
    }

    // http links (open)
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"http[s]?:\/\/\S+", "i"),
        Arc::new(Box::new(move |_modifiers, url| {
            if is_nw() {
                nw_sys::shell::open_external(url);
            } else {
                link::open(url);
            }
        })),
    )?;

    // addresses (open,copy) {explorer}/addresses/
    let cli_ = cli.clone();
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"(aero|aerotest):\S+", "i"),
        Arc::new(Box::new(move |modifiers, uri| {
            if modifiers.ctrl || modifiers.meta {
                if uri.starts_with("aerotest") {
                    cli_.term().writeln("testnet addresses can not be currently looked up with the block explorer");
                } else {
                    match explorer_url(&format!("addresses/{uri}")) {
                        Some(url) if is_nw() => nw_sys::shell::open_external(&url),
                        Some(url) => link::open(&url),
                        None => cli_.term().writeln("no block explorer configured (set AERO_EXPLORER_URL at build time)"),
                    }
                }
            } else {
                write_to_clipboard(&cli_, uri);
            }
        })),
    )?;

    // blocks (open,copy) {explorer}/blocks/
    let cli_ = cli.clone();
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"(block|pool):?\s+[0-9a-fA-F]{64}", "i"),
        Arc::new(Box::new(move |modifiers, text| {
            let re = Regex::new(r"(?i)^(block|pool):?\s+").unwrap();
            let uri = re.replace(text, "");

            if modifiers.ctrl || modifiers.meta {
                if let Some(url) = explorer_url(&format!("blocks/{uri}")) {
                    nw_sys::shell::open_external(&url);
                }
            } else {
                write_to_clipboard(&cli_, uri.to_string().as_str());
            }
        })),
    )?;

    // transactions
    let cli_ = cli.clone();
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"(transaction|tx|txid)(\s+|\s*:\s*)[0-9a-fA-F]{64}", "i"),
        Arc::new(Box::new(move |modifiers, text| {
            let re = Regex::new(r"(?i)^(transaction|tx|txid)\s*:?\s*").unwrap();
            let uri = re.replace(text, "");

            if modifiers.ctrl || modifiers.meta {
                if let Some(url) = explorer_url(&format!("txs/{uri}")) {
                    nw_sys::shell::open_external(&url);
                }
            } else {
                write_to_clipboard(&cli_, uri.to_string().as_str());
            }
        })),
    )?;

    // 32 byte hex encoded sequences (copy)
    let cli_ = cli.clone();
    cli.term().register_link_matcher(
        &js_sys::RegExp::new(r"[0-9a-fA-F]{64}", "i"),
        Arc::new(Box::new(move |_modifiers, text| {
            let re = Regex::new(r"(?i)^(transaction|tx|txid)\s*:?\s*").unwrap();
            let text = re.replace(text, "");
            write_to_clipboard(&cli_, text.to_string().as_str());
        })),
    )?;

    Ok(())
}

fn write_to_clipboard(cli: &Arc<AeroCli>, text: &str) {
    if is_nw() {
        let clipboard = nw_sys::clipboard::get();
        clipboard.set(text);
        cli.notifier().notify(Notification::Clipboard);
    } else {
        let cli = cli.clone();
        let text = text.to_owned();
        dispatch(async move {
            if let Err(err) = clipboard::write_text(&text).await {
                log_error!("{:?}", JsErrorData::from(err));
            } else {
                cli.notifier().notify(Notification::Clipboard);
            }
        });
    }
}
