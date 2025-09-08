use anyhow::Result;
use ghostbay_api::{create_router, AppState};
use ghostbay_auth::{AuthService, CreateAccessKeyRequest};
use ghostbay_catalog::CatalogService;
use ghostbay_engine::{create_storage_engine, StorageConfig};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_address: String,
    pub port: u16,
    pub database_url: String,
    pub data_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub log_level: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 3000,
            database_url: "sqlite:./ghostbay.db".to_string(),
            data_dir: PathBuf::from("./data"),
            temp_dir: PathBuf::from("./tmp"),
            log_level: "info".to_string(),
        }
    }
}

pub struct GhostBayServer {
    config: ServerConfig,
}

impl GhostBayServer {
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    pub async fn run(self) -> Result<()> {
        self.setup_tracing()?;

        tracing::info!("Starting GhostBay server...");
        tracing::info!("Configuration: {:?}", self.config);

        // Initialize catalog service
        let catalog = CatalogService::new(&self.config.database_url).await?;

        // Run database migrations
        ghostbay_catalog::migrations::ensure_database_exists(&self.config.database_url).await?;
        ghostbay_catalog::migrations::run_migrations(catalog.pool()).await?;

        // Initialize storage engine
        let storage_config = StorageConfig {
            data_dir: self.config.data_dir.clone(),
            temp_dir: self.config.temp_dir.clone(),
        };
        let storage = Arc::new(create_storage_engine(storage_config)?);

        // Initialize auth service with database connection
        let auth_service = AuthService::new(catalog.pool().clone());
        
        // Create a default access key for testing if none exist
        let request = CreateAccessKeyRequest {
            policies: vec!["admin".to_string()],
            description: Some("Default admin access key for testing".to_string()),
            expires_at: None,
        };
        let default_key = auth_service.create_access_key(request).await?;
        tracing::info!(
            "Created default access key: {} (secret: {})",
            default_key.access_key_id,
            default_key.secret_access_key
        );

        let auth = Arc::new(auth_service);

        // Create application state
        let app_state = AppState {
            catalog,
            storage,
            auth,
        };

        // Create router
        let app = create_router().with_state(app_state);

        // Start server
        let addr: SocketAddr = format!("{}:{}", self.config.bind_address, self.config.port).parse()?;
        let listener = TcpListener::bind(addr).await?;

        tracing::info!("GhostBay server listening on {}", addr);
        tracing::info!("Health check available at: http://{}/health", addr);
        tracing::info!("S3 API available at: http://{}/", addr);

        axum::serve(listener, app).await?;

        Ok(())
    }

    fn setup_tracing(&self) -> Result<()> {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&self.config.log_level));

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();

        Ok(())
    }
}