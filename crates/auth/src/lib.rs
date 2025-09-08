use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;

pub mod sigv4;
pub mod keys;
pub mod policy;

pub use sigv4::*;
pub use keys::*;
pub use policy::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub access_key_id: String,
    pub authenticated: bool,
    pub policies: Vec<String>,
    pub session_token: Option<String>,
}

pub struct AuthService {
    key_repo: AccessKeyRepository,
}

impl AuthService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            key_repo: AccessKeyRepository::new(pool),
        }
    }

    pub async fn get_access_key(&self, access_key_id: &str) -> Result<Option<AccessKey>> {
        self.key_repo.find_by_access_key_id(access_key_id).await
    }

    pub async fn create_access_key(&self, request: CreateAccessKeyRequest) -> Result<AccessKey> {
        self.key_repo.create(request).await
    }

    pub async fn validate_signature(&self, request: &SignatureValidationRequest) -> Result<AuthContext> {
        let access_key = self.get_access_key(&request.access_key_id).await?
            .ok_or_else(|| anyhow::anyhow!("Access key not found"))?;

        if let Some(expires_at) = access_key.expires_at {
            if chrono::Utc::now() > expires_at {
                return Err(anyhow::anyhow!("Access key expired"));
            }
        }

        // Use SigV4 validator to verify the signature
        let is_valid = SigV4Validator::validate_signature(
            &access_key.secret_access_key,
            &access_key.access_key_id,
            &request.method,
            &request.uri,
            &request.query_string,
            &request.signed_headers,
            &request.payload_hash,
            &request.signature,
            request.timestamp,
            &request.region,
            &request.service,
        )?;

        if !is_valid {
            return Err(anyhow::anyhow!("Invalid signature"));
        }

        Ok(AuthContext {
            access_key_id: access_key.access_key_id,
            authenticated: true,
            policies: access_key.policies,
            session_token: None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SignatureValidationRequest {
    pub access_key_id: String,
    pub signature: String,
    pub signed_headers: HashMap<String, String>,
    pub method: String,
    pub uri: String,
    pub query_string: String,
    pub payload_hash: String,
    pub timestamp: DateTime<Utc>,
    pub region: String,
    pub service: String,
}