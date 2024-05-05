//!
//! Location (file path) representation & multi-platform helper utilities.
//!

use crate::imports::*;
use crate::result::Result;
use std::path::{Path, PathBuf};
use workflow_core::runtime;
use workflow_store::fs;

/// Wallet file storage interface
/// @category Wallet SDK
#[wasm_bindgen(inspectable)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Storage {
    filename: PathBuf,
}

#[wasm_bindgen]
impl Storage {
    #[wasm_bindgen(getter, js_name = filename)]
    pub fn filename_as_string(&self) -> String {
        self.filename.to_str().unwrap().to_string()
    }
}

impl Storage {
    pub fn default_wallet_store() -> Self {
        Self::try_new(&format!("{}.wallet", super::default_wallet_file())).unwrap()
    }

    pub fn default_settings_store() -> Self {
        Self::try_new(&format!("{}.settings", super::default_wallet_file())).unwrap()
    }

    pub fn try_new(name: &str) -> Result<Storage> {
        let filename = if runtime::is_web() {
            PathBuf::from(name)
        } else {
            let filename = Path::new(super::default_storage_folder()).join(name);
            fs::resolve_path(filename.to_str().unwrap())?
        };

        Ok(Storage { filename })
    }

    pub fn try_new_with_folder(folder: &str, name: &str) -> Result<Storage> {
        let filename = if runtime::is_web() {
            PathBuf::from(name)
        } else {
            let filename = Path::new(folder).join(name);
            fs::resolve_path(filename.to_str().unwrap())?
        };

        Ok(Storage { filename })
    }

    pub fn rename_sync(&mut self, filename: &str) -> Result<()> {
        let target_filename = Path::new(filename).to_path_buf();
        workflow_store::fs::rename_sync(self.filename(), &target_filename)?;
        self.filename = target_filename;
        Ok(())
    }

    pub fn filename(&self) -> &PathBuf {
        &self.filename
    }

    pub async fn purge(&self) -> Result<()> {
        workflow_store::fs::remove(self.filename()).await?;
        Ok(())
    }

    pub async fn exists(&self) -> Result<bool> {
        Ok(workflow_store::fs::exists(self.filename()).await?)
    }

    pub fn exists_sync(&self) -> Result<bool> {
        Ok(workflow_store::fs::exists_sync(self.filename())?)
    }

    pub async fn ensure_dir(&self) -> Result<()> {
        if self.exists().await? {
            return Ok(());
        }

        let file = self.filename();

        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).await?;
        }
        Ok(())
    }

    pub fn ensure_dir_sync(&self) -> Result<()> {
        if !runtime::is_web() && !runtime::is_chrome_extension() {
            if self.exists_sync()? {
                return Ok(());
            }

            let file = self.filename();
            if let Some(dir) = file.parent() {
                fs::create_dir_all_sync(dir)?;
            }
        }
        Ok(())
    }
}
