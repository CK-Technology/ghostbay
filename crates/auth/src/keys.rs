use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use rand::Rng;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessKey {
    pub id: Uuid,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub policies: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccessKeyRequest {
    pub policies: Vec<String>,
    pub description: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

pub struct AccessKeyRepository {
    pool: SqlitePool,
}

impl AccessKeyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, req: CreateAccessKeyRequest) -> Result<AccessKey> {
        let id = Uuid::new_v4();
        let access_key_id = generate_access_key_id();
        let secret_access_key = generate_secret_access_key();
        let now = Utc::now();
        let policies_json = serde_json::to_string(&req.policies)?;

        sqlx::query(
            r#"
            INSERT INTO access_keys (id, access_key_id, secret_access_key, created_at, expires_at, is_active, policies, description)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(id.to_string())
        .bind(&access_key_id)
        .bind(&secret_access_key)
        .bind(now.to_rfc3339())
        .bind(req.expires_at.map(|e| e.to_rfc3339()))
        .bind(true)
        .bind(&policies_json)
        .bind(&req.description)
        .execute(&self.pool)
        .await?;

        Ok(AccessKey {
            id,
            access_key_id,
            secret_access_key,
            created_at: now,
            expires_at: req.expires_at,
            is_active: true,
            policies: req.policies,
            description: req.description,
        })
    }

    pub async fn find_by_access_key_id(&self, access_key_id: &str) -> Result<Option<AccessKey>> {
        let row = sqlx::query(
            "SELECT id, access_key_id, secret_access_key, created_at, expires_at, is_active, policies, description FROM access_keys WHERE access_key_id = ? AND is_active = true"
        )
        .bind(access_key_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let id: String = row.get("id");
            let access_key_id: String = row.get("access_key_id");
            let secret_access_key: String = row.get("secret_access_key");
            let created_at: String = row.get("created_at");
            let expires_at: Option<String> = row.get("expires_at");
            let is_active: bool = row.get("is_active");
            let policies_json: String = row.get("policies");
            let description: Option<String> = row.get("description");

            let policies: Vec<String> = serde_json::from_str(&policies_json)?;
            let access_key = AccessKey {
                id: Uuid::parse_str(&id)?,
                access_key_id,
                secret_access_key,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
                expires_at: expires_at
                    .map(|e| chrono::DateTime::parse_from_rfc3339(&e))
                    .transpose()?
                    .map(|e| e.with_timezone(&Utc)),
                is_active,
                policies,
                description,
            };
            Ok(Some(access_key))
        } else {
            Ok(None)
        }
    }

    pub async fn list(&self, include_inactive: bool) -> Result<Vec<AccessKey>> {
        let rows = if include_inactive {
            sqlx::query(
                "SELECT id, access_key_id, secret_access_key, created_at, expires_at, is_active, policies, description FROM access_keys ORDER BY created_at DESC"
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, access_key_id, secret_access_key, created_at, expires_at, is_active, policies, description FROM access_keys WHERE is_active = true ORDER BY created_at DESC"
            )
            .fetch_all(&self.pool)
            .await?
        };

        let mut access_keys = Vec::new();
        for row in rows {
            let id: String = row.get("id");
            let access_key_id: String = row.get("access_key_id");
            let secret_access_key: String = row.get("secret_access_key");
            let created_at: String = row.get("created_at");
            let expires_at: Option<String> = row.get("expires_at");
            let is_active: bool = row.get("is_active");
            let policies_json: String = row.get("policies");
            let description: Option<String> = row.get("description");

            let policies: Vec<String> = serde_json::from_str(&policies_json)?;
            let access_key = AccessKey {
                id: Uuid::parse_str(&id)?,
                access_key_id,
                secret_access_key,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
                expires_at: expires_at
                    .map(|e| chrono::DateTime::parse_from_rfc3339(&e))
                    .transpose()?
                    .map(|e| e.with_timezone(&Utc)),
                is_active,
                policies,
                description,
            };
            access_keys.push(access_key);
        }

        Ok(access_keys)
    }

    pub async fn deactivate(&self, access_key_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE access_keys SET is_active = false WHERE access_key_id = ?"
        )
        .bind(access_key_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, access_key_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM access_keys WHERE access_key_id = ?"
        )
        .bind(access_key_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn rotate(&self, access_key_id: &str) -> Result<Option<AccessKey>> {
        let existing = self.find_by_access_key_id(access_key_id).await?;
        if let Some(existing_key) = existing {
            let new_secret = generate_secret_access_key();
            let now = Utc::now();

            sqlx::query(
                "UPDATE access_keys SET secret_access_key = ?, created_at = ? WHERE access_key_id = ?"
            )
            .bind(&new_secret)
            .bind(now.to_rfc3339())
            .bind(access_key_id)
            .execute(&self.pool)
            .await?;

            Ok(Some(AccessKey {
                secret_access_key: new_secret,
                created_at: now,
                ..existing_key
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn cleanup_expired(&self) -> Result<u64> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE access_keys SET is_active = false WHERE expires_at IS NOT NULL AND expires_at < ?"
        )
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

fn generate_access_key_id() -> String {
    let mut rng = rand::thread_rng();
    let random_part: String = (0..16)
        .map(|_| {
            let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            chars[rng.gen_range(0..chars.len())] as char
        })
        .collect();
    format!("AKIA{}", random_part)
}

fn generate_secret_access_key() -> String {
    let mut rng = rand::thread_rng();
    (0..40)
        .map(|_| {
            let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            chars[rng.gen_range(0..chars.len())] as char
        })
        .collect()
}