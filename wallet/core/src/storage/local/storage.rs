use crate::imports::*;
use crate::result::Result;
use std::path::{Path, PathBuf};
use workflow_core::runtime;
use workflow_store::fs;

/// Wallet file storage interface
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
        Self::try_new(&format!("{}.wallet", super::DEFAULT_WALLET_FILE)).unwrap()
    }

    pub fn default_settings_store() -> Self {
        Self::try_new(&format!("{}.settings", super::DEFAULT_SETTINGS_FILE)).unwrap()
    }

    pub fn try_new(name: &str) -> Result<Storage> {
        let filename = if runtime::is_web() {
            PathBuf::from(name)
        } else {
            let filename = Path::new(super::DEFAULT_STORAGE_FOLDER).join(name);
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
        let file = self.filename();
        if file.exists() {
            return Ok(());
        }

        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).await?;
        }
        Ok(())
    }

    pub fn ensure_dir_sync(&self) -> Result<()> {
        let file = self.filename();
        if file.exists() {
            return Ok(());
        }

        if let Some(dir) = file.parent() {
            fs::create_dir_all_sync(dir)?;
        }
        Ok(())
    }
}
