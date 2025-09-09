use anyhow::Result;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::models::*;

pub struct BucketRepository {
    pool: SqlitePool,
}

impl BucketRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, req: CreateBucketRequest) -> Result<Bucket> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO buckets (id, name, created_at, updated_at, versioning_enabled, region)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(false)
        .bind(&req.region)
        .execute(&self.pool)
        .await?;

        let bucket = Bucket {
            id,
            name: req.name,
            created_at: now,
            updated_at: now,
            versioning_enabled: false,
            region: req.region,
        };

        Ok(bucket)
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Bucket>> {
        let row = sqlx::query(
            "SELECT id, name, created_at, updated_at, versioning_enabled, region FROM buckets WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let bucket = Bucket {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                name: row.get("name"),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
                versioning_enabled: row.get("versioning_enabled"),
                region: row.get("region"),
            };
            Ok(Some(bucket))
        } else {
            Ok(None)
        }
    }

    pub async fn list(&self) -> Result<Vec<Bucket>> {
        let rows = sqlx::query(
            "SELECT id, name, created_at, updated_at, versioning_enabled, region FROM buckets ORDER BY created_at"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut buckets = Vec::new();
        for row in rows {
            let bucket = Bucket {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                name: row.get("name"),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
                versioning_enabled: row.get("versioning_enabled"),
                region: row.get("region"),
            };
            buckets.push(bucket);
        }

        Ok(buckets)
    }

    pub async fn delete(&self, name: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM buckets WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

pub struct ObjectRepository {
    pool: SqlitePool,
}

impl ObjectRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, req: CreateObjectRequest, etag: String) -> Result<Object> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let metadata_json = req.metadata.map(|m| serde_json::to_string(&m)).transpose()?;

        sqlx::query(
            r#"
            INSERT INTO objects (id, bucket_id, key, etag, size, content_type, created_at, updated_at, storage_path, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(req.bucket_id.to_string())
        .bind(&req.key)
        .bind(&etag)
        .bind(req.size)
        .bind(&req.content_type)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(&req.storage_path)
        .bind(&metadata_json)
        .execute(&self.pool)
        .await?;

        let object = Object {
            id,
            bucket_id: req.bucket_id,
            key: req.key,
            version_id: None,
            etag,
            size: req.size,
            content_type: req.content_type,
            created_at: now,
            updated_at: now,
            storage_path: req.storage_path,
            metadata: metadata_json,
        };

        Ok(object)
    }

    pub async fn find_by_bucket_and_key(&self, bucket_id: Uuid, key: &str) -> Result<Option<Object>> {
        let row = sqlx::query(
            r#"
            SELECT id, bucket_id, key, version_id, etag, size, content_type, created_at, updated_at, storage_path, metadata
            FROM objects 
            WHERE bucket_id = ? AND key = ?
            "#,
        )
        .bind(bucket_id.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let object = Object {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                bucket_id: Uuid::parse_str(&row.get::<String, _>("bucket_id"))?,
                key: row.get("key"),
                version_id: row.get::<Option<String>, _>("version_id").map(|v| Uuid::parse_str(&v)).transpose()?,
                etag: row.get("etag"),
                size: row.get("size"),
                content_type: row.get("content_type"),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
                storage_path: row.get("storage_path"),
                metadata: row.get("metadata"),
            };
            Ok(Some(object))
        } else {
            Ok(None)
        }
    }

    pub async fn list_by_bucket(&self, bucket_id: Uuid, prefix: Option<&str>, limit: Option<i32>) -> Result<Vec<Object>> {
        let limit = limit.unwrap_or(1000).min(1000);
        let bucket_id_str = bucket_id.to_string();
        
        let rows = if let Some(prefix) = prefix {
            let like_pattern = format!("{}%", prefix);
            sqlx::query(
                r#"
                SELECT id, bucket_id, key, version_id, etag, size, content_type, created_at, updated_at, storage_path, metadata
                FROM objects 
                WHERE bucket_id = ? AND key LIKE ?
                ORDER BY key
                LIMIT ?
                "#,
            )
            .bind(&bucket_id_str)
            .bind(&like_pattern)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, bucket_id, key, version_id, etag, size, content_type, created_at, updated_at, storage_path, metadata
                FROM objects 
                WHERE bucket_id = ?
                ORDER BY key
                LIMIT ?
                "#,
            )
            .bind(&bucket_id_str)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        let mut objects = Vec::new();
        for row in rows {
            let object = Object {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                bucket_id: Uuid::parse_str(&row.get::<String, _>("bucket_id"))?,
                key: row.get("key"),
                version_id: row.get::<Option<String>, _>("version_id").map(|v| Uuid::parse_str(&v)).transpose()?,
                etag: row.get("etag"),
                size: row.get("size"),
                content_type: row.get("content_type"),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))?.with_timezone(&Utc),
                storage_path: row.get("storage_path"),
                metadata: row.get("metadata"),
            };
            objects.push(object);
        }

        Ok(objects)
    }

    pub async fn delete(&self, bucket_id: Uuid, key: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM objects WHERE bucket_id = ? AND key = ?")
            .bind(bucket_id.to_string())
            .bind(key)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

pub struct MultipartUploadRepository {
    pool: SqlitePool,
}

impl MultipartUploadRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, bucket_id: Uuid, object_key: &str, upload_id: &str) -> Result<MultipartUpload> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(7); // 7 days default expiration

        sqlx::query(
            r#"
            INSERT INTO multipart_uploads (id, bucket_id, object_key, upload_id, created_at, expires_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(bucket_id.to_string())
        .bind(object_key)
        .bind(upload_id)
        .bind(now.to_rfc3339())
        .bind(expires_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        let upload = MultipartUpload {
            id,
            bucket_id,
            object_key: object_key.to_string(),
            upload_id: upload_id.to_string(),
            created_at: now,
            expires_at: Some(expires_at),
        };

        Ok(upload)
    }

    pub async fn find_by_upload_id(&self, upload_id: &str) -> Result<Option<MultipartUpload>> {
        let row = sqlx::query(
            r#"
            SELECT id, bucket_id, object_key, upload_id, created_at, expires_at
            FROM multipart_uploads 
            WHERE upload_id = ?
            "#,
        )
        .bind(upload_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let upload = MultipartUpload {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                bucket_id: Uuid::parse_str(&row.get::<String, _>("bucket_id"))?,
                object_key: row.get("object_key"),
                upload_id: row.get("upload_id"),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
                expires_at: row.get::<Option<String>, _>("expires_at")
                    .map(|s| chrono::DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
                    .transpose()?,
            };
            Ok(Some(upload))
        } else {
            Ok(None)
        }
    }

    pub async fn delete(&self, upload_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM multipart_uploads WHERE upload_id = ?")
            .bind(upload_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_expired(&self) -> Result<Vec<MultipartUpload>> {
        let now = Utc::now();
        let rows = sqlx::query(
            r#"
            SELECT id, bucket_id, object_key, upload_id, created_at, expires_at
            FROM multipart_uploads 
            WHERE expires_at < ?
            "#,
        )
        .bind(now.to_rfc3339())
        .fetch_all(&self.pool)
        .await?;

        let mut uploads = Vec::new();
        for row in rows {
            let upload = MultipartUpload {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                bucket_id: Uuid::parse_str(&row.get::<String, _>("bucket_id"))?,
                object_key: row.get("object_key"),
                upload_id: row.get("upload_id"),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
                expires_at: row.get::<Option<String>, _>("expires_at")
                    .map(|s| chrono::DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
                    .transpose()?,
            };
            uploads.push(upload);
        }

        Ok(uploads)
    }
}

pub struct MultipartPartRepository {
    pool: SqlitePool,
}

impl MultipartPartRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, upload_id: Uuid, part_number: i32, etag: String, size: i64, storage_path: String) -> Result<MultipartPart> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO multipart_parts (id, upload_id, part_number, etag, size, created_at, storage_path)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(upload_id.to_string())
        .bind(part_number)
        .bind(&etag)
        .bind(size)
        .bind(now.to_rfc3339())
        .bind(&storage_path)
        .execute(&self.pool)
        .await?;

        let part = MultipartPart {
            id,
            upload_id,
            part_number,
            etag,
            size,
            created_at: now,
            storage_path,
        };

        Ok(part)
    }

    pub async fn find_by_upload_and_part(&self, upload_id: Uuid, part_number: i32) -> Result<Option<MultipartPart>> {
        let row = sqlx::query(
            r#"
            SELECT id, upload_id, part_number, etag, size, created_at, storage_path
            FROM multipart_parts 
            WHERE upload_id = ? AND part_number = ?
            "#,
        )
        .bind(upload_id.to_string())
        .bind(part_number)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let part = MultipartPart {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                upload_id: Uuid::parse_str(&row.get::<String, _>("upload_id"))?,
                part_number: row.get("part_number"),
                etag: row.get("etag"),
                size: row.get("size"),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
                storage_path: row.get("storage_path"),
            };
            Ok(Some(part))
        } else {
            Ok(None)
        }
    }

    pub async fn list_by_upload(&self, upload_id: Uuid) -> Result<Vec<MultipartPart>> {
        let rows = sqlx::query(
            r#"
            SELECT id, upload_id, part_number, etag, size, created_at, storage_path
            FROM multipart_parts 
            WHERE upload_id = ?
            ORDER BY part_number
            "#,
        )
        .bind(upload_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut parts = Vec::new();
        for row in rows {
            let part = MultipartPart {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                upload_id: Uuid::parse_str(&row.get::<String, _>("upload_id"))?,
                part_number: row.get("part_number"),
                etag: row.get("etag"),
                size: row.get("size"),
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))?.with_timezone(&Utc),
                storage_path: row.get("storage_path"),
            };
            parts.push(part);
        }

        Ok(parts)
    }

    pub async fn delete_by_upload(&self, upload_id: Uuid) -> Result<u64> {
        let result = sqlx::query("DELETE FROM multipart_parts WHERE upload_id = ?")
            .bind(upload_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}