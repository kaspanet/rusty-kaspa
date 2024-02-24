//! Local storage implementation for the wallet SDK.
//! This module provides a local storage implementation
//! that functions uniformly in native and JS environments.
//! In native and NodeJS environments, this subsystem
//! will use the native file system IO. In the browser
//! environment, if called from the web page context
//! this will use `localStorage` and if invoked in the
//! chromium extension context it will use the
//! `chrome.storage.local` API. The implementation
//! is backed by the [`workflow_store`](https://docs.rs/workflow-store/)
//! crate.

pub mod cache;
pub mod collection;
pub mod interface;
pub mod payload;
pub mod storage;
pub mod streams;
pub mod transaction;
pub mod wallet;

pub use collection::Collection;
pub use payload::Payload;
pub use storage::Storage;
pub use wallet::WalletStorage;

use crate::error::Error;
use crate::result::Result;
use wasm_bindgen::prelude::*;
use workflow_store::fs::create_dir_all_sync;

static mut DEFAULT_STORAGE_FOLDER: Option<String> = None;
static mut DEFAULT_WALLET_FILE: Option<String> = None;
static mut DEFAULT_SETTINGS_FILE: Option<String> = None;

pub fn default_storage_folder() -> &'static str {
    // SAFETY: This operation is initializing a static mut variable,
    // however, the actual variable is accessible only through
    // this function.
    unsafe { DEFAULT_STORAGE_FOLDER.get_or_insert("~/.kaspa".to_string()).as_str() }
}

pub fn default_wallet_file() -> &'static str {
    // SAFETY: This operation is initializing a static mut variable,
    // however, the actual variable is accessible only through
    // this function.
    unsafe { DEFAULT_WALLET_FILE.get_or_insert("kaspa".to_string()).as_str() }
}

pub fn default_settings_file() -> &'static str {
    // SAFETY: This operation is initializing a static mut variable,
    // however, the actual variable is accessible only through
    // this function.
    unsafe { DEFAULT_SETTINGS_FILE.get_or_insert("kaspa".to_string()).as_str() }
}

/// Set a custom storage folder for the wallet SDK
/// subsystem.  Encrypted wallet files and transaction
/// data will be stored in this folder. If not set
/// the storage folder will default to `~/.kaspa`
/// (note that the folder is hidden).
///
/// This must be called before using any other wallet
/// SDK functions.
///
/// NOTE: This function will create a folder if it
/// doesn't exist. This function will have no effect
/// if invoked in the browser environment.
///
/// # Safety
///
/// This function is unsafe because it is setting a static
/// mut variable, meaning this function is not thread-safe.
/// However the function must be used before any other
/// wallet operations are performed. You must not change
/// the default storage folder once the wallet has been
/// initialized.
///
pub unsafe fn set_default_storage_folder(folder: String) -> Result<()> {
    create_dir_all_sync(&folder).map_err(|err| Error::custom(format!("Failed to create storage folder: {err}")))?;
    DEFAULT_STORAGE_FOLDER = Some(folder);
    Ok(())
}

/// Set a custom storage folder for the wallet SDK
/// subsystem.  Encrypted wallet files and transaction
/// data will be stored in this folder. If not set
/// the storage folder will default to `~/.kaspa`
/// (note that the folder is hidden).
///
/// This must be called before using any other wallet
/// SDK functions.
///
/// NOTE: This function will create a folder if it
/// doesn't exist. This function will have no effect
/// if invoked in the browser environment.
///
/// @param {String} folder - the path to the storage folder
///
/// @category Wallet API
#[wasm_bindgen(js_name = setDefaultStorageFolder, skip_jsdoc)]
pub fn js_set_default_storage_folder(folder: String) -> Result<()> {
    // SAFETY: This is unsafe because we are setting a static mut variable
    // meaning this function is not thread-safe. However the function
    // must be used before any other wallet operations are performed.
    unsafe { set_default_storage_folder(folder) }
}

/// Set the name of the default wallet file name
/// or the `localStorage` key.  If `Wallet::open`
/// is called without a wallet file name, this name
/// will be used.  Please note that this name
/// will be suffixed with `.wallet` suffix.
///
/// This function should be called before using any
/// other wallet SDK functions.
///
/// # Safety
///
/// This function is unsafe because it is setting a static
/// mut variable, meaning this function is not thread-safe.
///
pub unsafe fn set_default_wallet_file(folder: String) -> Result<()> {
    DEFAULT_WALLET_FILE = Some(folder);
    Ok(())
}

/// Set the name of the default wallet file name
/// or the `localStorage` key.  If `Wallet::open`
/// is called without a wallet file name, this name
/// will be used.  Please note that this name
/// will be suffixed with `.wallet` suffix.
///
/// This function should be called before using any
/// other wallet SDK functions.
///
/// @param {String} folder - the name to the wallet file or key.
///
/// @category Wallet API
#[wasm_bindgen(js_name = setDefaultWalletFile)]
pub fn js_set_default_wallet_file(folder: String) -> Result<()> {
    // SAFETY: This is unsafe because we are setting a static mut variable
    // meaning this function is not thread-safe.
    unsafe {
        DEFAULT_WALLET_FILE = Some(folder);
    }
    Ok(())
}
