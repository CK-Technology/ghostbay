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

#[derive(Debug, Clone)]
pub struct MultipartUploadPart {
    pub part_number: i32,
    pub etag: String,
    pub size: u64,
}

pub struct CreateMultipartUploadRequest {
    pub bucket: String,
    pub key: String,
    pub content_type: String,
    pub metadata: Option<serde_json::Value>,
}

pub struct UploadPartRequest {
    pub bucket: String,
    pub key: String,
    pub upload_id: String,
    pub part_number: i32,
    pub data: ByteStream,
}

pub struct CompleteMultipartUploadRequest {
    pub bucket: String,
    pub key: String,
    pub upload_id: String,
    pub parts: Vec<MultipartUploadPart>,
}

pub trait StorageEngine: Send + Sync {
    async fn put_object(&self, request: PutObjectRequest) -> Result<String>;
    
    async fn get_object(&self, request: GetObjectRequest) -> Result<Option<GetObjectResponse>>;
    
    async fn head_object(&self, bucket: &str, key: &str) -> Result<Option<ObjectMetadata>>;
    
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<bool>;
    
    async fn copy_object(&self, src_bucket: &str, src_key: &str, dst_bucket: &str, dst_key: &str) -> Result<String>;
    
    // Multipart upload operations
    async fn create_multipart_upload(&self, request: CreateMultipartUploadRequest) -> Result<String>;
    
    async fn upload_part(&self, request: UploadPartRequest) -> Result<String>;
    
    async fn complete_multipart_upload(&self, request: CompleteMultipartUploadRequest) -> Result<String>;
    
    async fn abort_multipart_upload(&self, bucket: &str, key: &str, upload_id: &str) -> Result<()>;
}