use anyhow::Result;
use bytes::Bytes;
use futures::Stream;
use std::path::PathBuf;
use uuid::Uuid;

pub mod local;
pub mod traits;

pub use local::*;
pub use traits::*;

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            temp_dir: PathBuf::from("./tmp"),
        }
    }
}

pub fn create_storage_engine(config: StorageConfig) -> Result<LocalStorageEngine> {
    LocalStorageEngine::new(config)
}