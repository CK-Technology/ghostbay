use anyhow::Result;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

pub async fn ensure_database_exists(database_url: &str) -> Result<()> {
    if !Sqlite::database_exists(database_url).await.unwrap_or(false) {
        Sqlite::create_database(database_url).await?;
        tracing::info!("Database created: {}", database_url);
    }
    Ok(())
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    // Create buckets table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS buckets (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            versioning_enabled BOOLEAN NOT NULL DEFAULT FALSE,
            region TEXT NOT NULL DEFAULT 'us-east-1'
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create objects table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS objects (
            id TEXT PRIMARY KEY NOT NULL,
            bucket_id TEXT NOT NULL,
            key TEXT NOT NULL,
            version_id TEXT,
            etag TEXT NOT NULL,
            size INTEGER NOT NULL,
            content_type TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            storage_path TEXT NOT NULL,
            metadata TEXT,
            FOREIGN KEY (bucket_id) REFERENCES buckets (id) ON DELETE CASCADE,
            UNIQUE(bucket_id, key)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create multipart_uploads table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS multipart_uploads (
            id TEXT PRIMARY KEY NOT NULL,
            bucket_id TEXT NOT NULL,
            object_key TEXT NOT NULL,
            upload_id TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            expires_at TEXT,
            FOREIGN KEY (bucket_id) REFERENCES buckets (id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create multipart_parts table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS multipart_parts (
            id TEXT PRIMARY KEY NOT NULL,
            upload_id TEXT NOT NULL,
            part_number INTEGER NOT NULL,
            etag TEXT NOT NULL,
            size INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            storage_path TEXT NOT NULL,
            FOREIGN KEY (upload_id) REFERENCES multipart_uploads (id) ON DELETE CASCADE,
            UNIQUE(upload_id, part_number)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create access_keys table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS access_keys (
            id TEXT PRIMARY KEY NOT NULL,
            access_key_id TEXT NOT NULL UNIQUE,
            secret_access_key TEXT NOT NULL,
            created_at TEXT NOT NULL,
            expires_at TEXT,
            is_active BOOLEAN NOT NULL DEFAULT TRUE,
            policies TEXT NOT NULL,
            description TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create useful indexes
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_objects_bucket_key ON objects (bucket_id, key)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_objects_bucket_prefix ON objects (bucket_id, key)")
        .execute(pool)
        .await?;
    
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_access_keys_active ON access_keys (access_key_id, is_active)")
        .execute(pool)
        .await?;

    tracing::info!("Database migrations completed successfully");
    Ok(())
}