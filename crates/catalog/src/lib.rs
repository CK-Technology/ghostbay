use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

pub mod models;
pub mod repository;
pub mod migrations;

pub use models::*;
pub use repository::*;

#[derive(Debug, Clone)]
pub struct CatalogService {
    pool: SqlitePool,
}

impl CatalogService {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        Ok(Self { pool })
    }
    
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}