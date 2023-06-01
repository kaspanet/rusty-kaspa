use crate::imports::*;
use crate::result::Result;
use std::path::{Path, PathBuf};
use workflow_core::runtime;
use workflow_store::fs;

/// Wallet file storage interface
#[wasm_bindgen(inspectable)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Store {
    filename: PathBuf,
}

#[wasm_bindgen]
impl Store {
    #[wasm_bindgen(getter, js_name = filename)]
    pub fn filename_as_string(&self) -> String {
        self.filename.to_str().unwrap().to_string()
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new(super::DEFAULT_WALLET_FOLDER, super::DEFAULT_WALLET_FILE).unwrap()
    }
}

impl Store {
    pub fn new(folder: &str, name: &str) -> Result<Store> {
        let filename = if runtime::is_web() {
            PathBuf::from(name) //filename.file_name().ok_or(Error::InvalidFilename(format!("{}", filename.display())))?)
        } else {
            // let filename = Path::new(DEFAULT_WALLET_FOLDER).join(name);
            let filename = Path::new(folder).join(name);
            let filename = fs::resolve_path(filename.to_str().unwrap());
            filename
        };

        Ok(Store { filename })
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
}

// pub struct Settings;

// #[derive(Default)]
