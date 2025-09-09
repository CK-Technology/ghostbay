use anyhow::Result;
use ghostbay_api::{create_router, AppState};
use ghostbay_auth::{AuthService, CreateAccessKeyRequest};
use ghostbay_catalog::CatalogService;
use ghostbay_engine::{create_storage_engine, StorageConfig};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use axum::{
    extract::Request,
    http::Uri,
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    Router,
};
use axum_server::tls_rustls::RustlsConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_address: String,
    pub port: u16,
    pub database_url: String,
    pub data_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub log_level: String,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub https_port: Option<u16>,
    pub redirect_http_to_https: bool,
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
            tls: None,
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

        // Create router with security headers
        let app = create_router()
            .with_state(app_state)
            .layer(middleware::from_fn(security_headers_middleware));

        let tls_config = self.config.tls.clone();
        
        if let Some(tls_config) = tls_config {
            // TLS enabled
            self.run_with_tls(app, tls_config).await
        } else {
            // HTTP only
            self.run_http_only(app).await
        }
    }

    async fn run_http_only(self, app: Router) -> Result<()> {
        let addr: SocketAddr = format!("{}:{}", self.config.bind_address, self.config.port).parse()?;
        let listener = TcpListener::bind(addr).await?;

        tracing::info!("GhostBay server listening on http://{}", addr);
        tracing::info!("Health check available at: http://{}/health", addr);
        tracing::info!("S3 API available at: http://{}/", addr);
        tracing::warn!("⚠️  TLS is disabled. Consider enabling HTTPS in production!");

        axum::serve(listener, app).await?;
        Ok(())
    }

    async fn run_with_tls(self, app: Router, tls_config: TlsConfig) -> Result<()> {
        // Load TLS certificates
        let rustls_config = self.load_tls_config(&tls_config).await?;
        
        let https_port = tls_config.https_port.unwrap_or(443);
        let https_addr: SocketAddr = format!("{}:{}", self.config.bind_address, https_port).parse()?;

        tracing::info!("GhostBay server starting with TLS...");
        tracing::info!("HTTPS server listening on https://{}", https_addr);
        tracing::info!("Health check available at: https://{}/health", https_addr);
        tracing::info!("S3 API available at: https://{}/", https_addr);

        if tls_config.redirect_http_to_https {
            // Start HTTP redirect server
            let redirect_app = Router::new()
                .fallback(redirect_to_https)
                .layer(middleware::from_fn(security_headers_middleware));
            
            let http_addr: SocketAddr = format!("{}:{}", self.config.bind_address, self.config.port).parse()?;
            let http_listener = TcpListener::bind(http_addr).await?;
            
            tracing::info!("HTTP redirect server listening on http://{}", http_addr);
            
            // Start HTTP redirect server in background
            tokio::spawn(async move {
                if let Err(e) = axum::serve(http_listener, redirect_app).await {
                    tracing::error!("HTTP redirect server error: {}", e);
                }
            });
        }

        // Start HTTPS server
        axum_server::bind_rustls(https_addr, rustls_config)
            .serve(app.into_make_service())
            .await?;

        Ok(())
    }

    async fn load_tls_config(&self, tls_config: &TlsConfig) -> Result<RustlsConfig> {
        let config = RustlsConfig::from_pem_file(
            &tls_config.cert_path,
            &tls_config.key_path,
        ).await?;

        Ok(config)
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

// Security headers middleware
async fn security_headers_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    
    let headers = response.headers_mut();
    
    // HSTS (HTTP Strict Transport Security)
    headers.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains; preload".parse().unwrap(),
    );
    
    // Content Security Policy
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; object-src 'none'; frame-ancestors 'none'".parse().unwrap(),
    );
    
    // X-Frame-Options
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    
    // X-Content-Type-Options
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    
    // Referrer Policy
    headers.insert("Referrer-Policy", "strict-origin-when-cross-origin".parse().unwrap());
    
    // Permissions Policy
    headers.insert(
        "Permissions-Policy",
        "geolocation=(), microphone=(), camera=()".parse().unwrap(),
    );

    response
}

// HTTPS redirect handler
async fn redirect_to_https(uri: Uri) -> impl IntoResponse {
    let authority = uri.authority().map(|a| a.as_str()).unwrap_or("localhost");
    let path_and_query = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    
    // Remove port from authority and use HTTPS default port
    let host = authority.split(':').next().unwrap_or(authority);
    let https_url = format!("https://{}{}", host, path_and_query);
    
    Redirect::permanent(&https_url)
}