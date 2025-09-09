use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    trace::TraceLayer,
};

pub mod handlers;
pub mod middleware;
pub mod error;
pub mod extractors;
pub mod responses;

pub use error::*;
pub use handlers::*;

#[derive(Clone)]
pub struct AppState {
    pub catalog: ghostbay_catalog::CatalogService,
    pub storage: std::sync::Arc<ghostbay_engine::LocalStorageEngine>,
    pub auth: std::sync::Arc<ghostbay_auth::AuthService>,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        // S3 API routes
        .route("/", get(handlers::list_buckets))
        .route("/:bucket", put(handlers::create_bucket))
        .route("/:bucket", get(handlers::list_objects))
        .route("/:bucket", delete(handlers::delete_bucket))
        // Object routes with conditional multipart handling
        .route("/:bucket/*key", put(handlers::put_object_or_part))
        .route("/:bucket/*key", post(handlers::create_multipart_upload_or_complete))
        .route("/:bucket/*key", delete(handlers::delete_object_or_abort_upload))
        .route("/:bucket/*key", get(handlers::get_object))
        .route("/:bucket/*key", axum::routing::head(handlers::head_object))
        // Health check
        .route("/health", get(health_check))
        // Apply middleware
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new())
                .layer(CorsLayer::permissive()),
        )
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "ghostbay",
        "version": env!("CARGO_PKG_VERSION")
    }))
}