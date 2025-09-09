use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListBucketsResponse {
    pub owner: Owner,
    pub buckets: Buckets,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Owner {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Buckets {
    pub bucket: Vec<BucketInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BucketInfo {
    pub name: String,
    pub creation_date: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListObjectsV2Response {
    pub name: String,
    pub prefix: Option<String>,
    pub key_count: u32,
    pub max_keys: u32,
    pub is_truncated: bool,
    pub continuation_token: Option<String>,
    pub next_continuation_token: Option<String>,
    pub contents: Vec<ObjectInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ObjectInfo {
    pub key: String,
    #[serde(rename = "LastModified")]
    pub last_modified: DateTime<Utc>,
    #[serde(rename = "ETag")]
    pub etag: String,
    pub size: u64,
    pub storage_class: String,
    pub owner: Owner,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CopyObjectResponse {
    pub copy_object_result: CopyObjectResult,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CopyObjectResult {
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "LastModified")]
    pub last_modified: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InitiateMultipartUploadResponse {
    pub bucket: String,
    pub key: String,
    pub upload_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CompleteMultipartUploadResponse {
    pub location: String,
    pub bucket: String,
    pub key: String,
    #[serde(rename = "ETag")]
    pub etag: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Part {
    #[serde(rename = "ETag")]
    pub etag: String,
    pub part_number: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompleteMultipartUploadRequest {
    #[serde(rename = "CompleteMultipartUpload")]
    pub complete_multipart_upload: CompleteMultipartUploadData,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CompleteMultipartUploadData {
    pub part: Vec<Part>,
}