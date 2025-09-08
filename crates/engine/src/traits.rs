use anyhow::Result;
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use uuid::Uuid;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>;

#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    pub content_type: String,
    pub content_length: u64,
    pub etag: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

pub struct PutObjectRequest {
    pub bucket: String,
    pub key: String,
    pub content_type: String,
    pub content_length: Option<u64>,
    pub data: ByteStream,
}

#[derive(Debug, Clone)]
pub struct GetObjectRequest {
    pub bucket: String,
    pub key: String,
    pub range: Option<(u64, Option<u64>)>, // (start, end)
}

pub struct GetObjectResponse {
    pub metadata: ObjectMetadata,
    pub data: ByteStream,
}

pub trait StorageEngine: Send + Sync {
    async fn put_object(&self, request: PutObjectRequest) -> Result<String>;
    
    async fn get_object(&self, request: GetObjectRequest) -> Result<Option<GetObjectResponse>>;
    
    async fn head_object(&self, bucket: &str, key: &str) -> Result<Option<ObjectMetadata>>;
    
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<bool>;
    
    async fn copy_object(&self, src_bucket: &str, src_key: &str, dst_bucket: &str, dst_key: &str) -> Result<String>;
}