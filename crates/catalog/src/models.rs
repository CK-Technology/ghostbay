use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bucket {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub versioning_enabled: bool,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Object {
    pub id: Uuid,
    pub bucket_id: Uuid,
    pub key: String,
    pub version_id: Option<Uuid>,
    pub etag: String,
    pub size: i64,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub storage_path: String,
    pub metadata: Option<String>, // JSON serialized metadata
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUpload {
    pub id: Uuid,
    pub bucket_id: Uuid,
    pub object_key: String,
    pub upload_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartPart {
    pub id: Uuid,
    pub upload_id: Uuid,
    pub part_number: i32,
    pub etag: String,
    pub size: i64,
    pub created_at: DateTime<Utc>,
    pub storage_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBucketRequest {
    pub name: String,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateObjectRequest {
    pub bucket_id: Uuid,
    pub key: String,
    pub content_type: String,
    pub size: i64,
    pub storage_path: String,
    pub metadata: Option<serde_json::Value>,
}