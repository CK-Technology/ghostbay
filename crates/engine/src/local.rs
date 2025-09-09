use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures::{Stream, StreamExt, TryStreamExt};
use md5::Digest;
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

use crate::{
    traits::*,
    StorageConfig,
};

#[derive(Debug, Clone)]
pub struct LocalStorageEngine {
    config: StorageConfig,
}

impl LocalStorageEngine {
    pub fn new(config: StorageConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;
        std::fs::create_dir_all(&config.temp_dir)?;
        
        Ok(Self { config })
    }

    fn object_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.config.data_dir.join(bucket).join(key)
    }

    fn temp_path(&self) -> PathBuf {
        self.config.temp_dir.join(format!("tmp_{}", Uuid::new_v4()))
    }

    async fn ensure_bucket_dir(&self, bucket: &str) -> Result<()> {
        let bucket_dir = self.config.data_dir.join(bucket);
        fs::create_dir_all(&bucket_dir).await?;
        Ok(())
    }

    async fn calculate_etag<S>(&self, mut stream: S) -> Result<String>
    where
        S: Stream<Item = Result<Bytes>> + Unpin,
    {
        use md5::{Digest, Md5};
        
        let mut hasher = Md5::new();
        
        while let Some(chunk) = stream.try_next().await? {
            hasher.update(&chunk);
        }
        
        Ok(format!("{:x}", hasher.finalize()))
    }
}

impl StorageEngine for LocalStorageEngine {
    async fn put_object(&self, request: PutObjectRequest) -> Result<String> {
        self.ensure_bucket_dir(&request.bucket).await?;
        
        let object_path = self.object_path(&request.bucket, &request.key);
        let temp_path = self.temp_path();
        
        // Ensure parent directories exist
        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        // Write to temporary file first
        let mut temp_file = fs::File::create(&temp_path).await?;
        let mut stream = request.data;
        use md5::Digest;
        let mut hasher = md5::Md5::new();
        
        while let Some(chunk) = stream.try_next().await? {
            hasher.update(&chunk);
            temp_file.write_all(&chunk).await?;
        }
        
        temp_file.sync_all().await?;
        drop(temp_file);
        
        // Atomic move to final location
        fs::rename(&temp_path, &object_path).await?;
        
        let etag = format!("{:x}", hasher.finalize());
        Ok(etag)
    }

    async fn get_object(&self, request: GetObjectRequest) -> Result<Option<GetObjectResponse>> {
        let object_path = self.object_path(&request.bucket, &request.key);
        
        if !object_path.exists() {
            return Ok(None);
        }
        
        let metadata = fs::metadata(&object_path).await?;
        let last_modified = metadata.modified()?.into();
        
        let stream: ByteStream = if let Some((start, end)) = request.range {
            let file = fs::File::open(&object_path).await?;
            let end = end.unwrap_or(metadata.len() - 1).min(metadata.len() - 1);
            
            if start > end || start >= metadata.len() {
                return Err(anyhow!("Invalid range: {}-{}", start, end));
            }
            
            let reader = tokio::io::BufReader::new(file);
            let stream = tokio_util::io::ReaderStream::new(reader)
                .map_err(|e| anyhow::Error::from(e))
                .skip(start as usize)
                .take((end - start + 1) as usize);
            
            Box::pin(stream)
        } else {
            let file = fs::File::open(&object_path).await?;
            let reader = tokio::io::BufReader::new(file);
            let stream = tokio_util::io::ReaderStream::new(reader)
                .map_err(|e| anyhow::Error::from(e));
            
            Box::pin(stream)
        };
        
        // Calculate ETag (simplified - just use file size and mtime)
        let etag = format!("\"{}\"", metadata.len());
        
        let object_metadata = ObjectMetadata {
            content_type: self.guess_content_type(&request.key),
            content_length: metadata.len(),
            etag,
            last_modified,
        };
        
        Ok(Some(GetObjectResponse {
            metadata: object_metadata,
            data: stream,
        }))
    }

    async fn head_object(&self, bucket: &str, key: &str) -> Result<Option<ObjectMetadata>> {
        let object_path = self.object_path(bucket, key);
        
        if !object_path.exists() {
            return Ok(None);
        }
        
        let metadata = fs::metadata(&object_path).await?;
        let last_modified = metadata.modified()?.into();
        let etag = format!("\"{}\"", metadata.len());
        
        Ok(Some(ObjectMetadata {
            content_type: self.guess_content_type(key),
            content_length: metadata.len(),
            etag,
            last_modified,
        }))
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<bool> {
        let object_path = self.object_path(bucket, key);
        
        if !object_path.exists() {
            return Ok(false);
        }
        
        fs::remove_file(&object_path).await?;
        Ok(true)
    }

    async fn copy_object(&self, src_bucket: &str, src_key: &str, dst_bucket: &str, dst_key: &str) -> Result<String> {
        let src_path = self.object_path(src_bucket, src_key);
        let dst_path = self.object_path(dst_bucket, dst_key);
        
        if !src_path.exists() {
            return Err(anyhow!("Source object not found"));
        }
        
        self.ensure_bucket_dir(dst_bucket).await?;
        
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        fs::copy(&src_path, &dst_path).await?;
        
        let metadata = fs::metadata(&dst_path).await?;
        let etag = format!("\"{}\"", metadata.len());
        
        Ok(etag)
    }
    
    // Multipart upload operations
    async fn create_multipart_upload(&self, request: CreateMultipartUploadRequest) -> Result<String> {
        self.ensure_bucket_dir(&request.bucket).await?;
        
        // Generate a unique upload ID
        let upload_id = format!("mpu_{}", Uuid::new_v4());
        
        // Create a temp directory for this multipart upload
        let upload_dir = self.config.temp_dir.join(&upload_id);
        fs::create_dir_all(&upload_dir).await?;
        
        // Store metadata about the upload
        let metadata = serde_json::json!({
            "bucket": request.bucket,
            "key": request.key,
            "content_type": request.content_type,
            "upload_id": upload_id,
            "metadata": request.metadata
        });
        
        let metadata_path = upload_dir.join("metadata.json");
        fs::write(&metadata_path, metadata.to_string()).await?;
        
        Ok(upload_id)
    }
    
    async fn upload_part(&self, request: UploadPartRequest) -> Result<String> {
        let upload_dir = self.config.temp_dir.join(&request.upload_id);
        
        if !upload_dir.exists() {
            return Err(anyhow!("Multipart upload not found: {}", request.upload_id));
        }
        
        // Write part to a numbered file
        let part_path = upload_dir.join(format!("part_{:05}", request.part_number));
        let mut part_file = fs::File::create(&part_path).await?;
        let mut stream = request.data;
        
        use md5::Digest;
        let mut hasher = md5::Md5::new();
        
        while let Some(chunk) = stream.try_next().await? {
            hasher.update(&chunk);
            part_file.write_all(&chunk).await?;
        }
        
        part_file.sync_all().await?;
        drop(part_file);
        
        let etag = format!("{:x}", hasher.finalize());
        Ok(etag)
    }
    
    async fn complete_multipart_upload(&self, request: CompleteMultipartUploadRequest) -> Result<String> {
        let upload_dir = self.config.temp_dir.join(&request.upload_id);
        
        if !upload_dir.exists() {
            return Err(anyhow!("Multipart upload not found: {}", request.upload_id));
        }
        
        // Read upload metadata
        let metadata_path = upload_dir.join("metadata.json");
        let metadata_content = fs::read_to_string(&metadata_path).await?;
        let upload_metadata: serde_json::Value = serde_json::from_str(&metadata_content)?;
        
        // Verify bucket and key match
        let expected_bucket = upload_metadata["bucket"].as_str().unwrap_or("");
        let expected_key = upload_metadata["key"].as_str().unwrap_or("");
        
        if expected_bucket != request.bucket || expected_key != request.key {
            return Err(anyhow!("Bucket/key mismatch in multipart upload"));
        }
        
        // Sort parts by part number
        let mut sorted_parts = request.parts.clone();
        sorted_parts.sort_by_key(|p| p.part_number);
        
        // Validate all parts exist and etags match
        for part in &sorted_parts {
            let part_path = upload_dir.join(format!("part_{:05}", part.part_number));
            if !part_path.exists() {
                return Err(anyhow!("Part {} not found", part.part_number));
            }
        }
        
        // Create the final object by concatenating parts
        let final_path = self.object_path(&request.bucket, &request.key);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        let mut final_file = fs::File::create(&final_path).await?;
        let mut combined_hasher = md5::Md5::new();
        
        for part in &sorted_parts {
            let part_path = upload_dir.join(format!("part_{:05}", part.part_number));
            let part_data = fs::read(&part_path).await?;
            combined_hasher.update(&part_data);
            final_file.write_all(&part_data).await?;
        }
        
        final_file.sync_all().await?;
        drop(final_file);
        
        // Clean up temp directory
        fs::remove_dir_all(&upload_dir).await?;
        
        // Calculate final ETag (for multipart, it's different from simple MD5)
        // AWS uses: MD5 of concatenated MD5s + "-" + part count
        let mut etag_parts = Vec::new();
        for part in &sorted_parts {
            etag_parts.extend_from_slice(part.etag.as_bytes());
        }
        let final_etag = format!("{:x}-{}", md5::Md5::digest(&etag_parts), sorted_parts.len());
        
        Ok(final_etag)
    }
    
    async fn abort_multipart_upload(&self, _bucket: &str, _key: &str, upload_id: &str) -> Result<()> {
        let upload_dir = self.config.temp_dir.join(upload_id);
        
        if upload_dir.exists() {
            fs::remove_dir_all(&upload_dir).await?;
        }
        
        Ok(())
    }
}

impl LocalStorageEngine {
    fn guess_content_type(&self, key: &str) -> String {
        let extension = Path::new(key)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
            
        match extension {
            "txt" => "text/plain",
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "json" => "application/json",
            "xml" => "application/xml",
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "zip" => "application/zip",
            _ => "binary/octet-stream",
        }.to_string()
    }
}