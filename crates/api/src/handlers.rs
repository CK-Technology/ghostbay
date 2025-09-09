use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};

use ghostbay_catalog::{CreateBucketRequest, CreateObjectRequest, BucketRepository, ObjectRepository, MultipartUploadRepository, MultipartPartRepository};
use ghostbay_engine::{GetObjectRequest, PutObjectRequest, StorageEngine, CreateMultipartUploadRequest, UploadPartRequest, CompleteMultipartUploadRequest, MultipartUploadPart};

use crate::{
    error::{ApiError, ApiResult},
    extractors::{ListObjectsQuery, S3Headers},
    responses::*,
    AppState,
};

pub async fn list_buckets(State(state): State<AppState>) -> ApiResult<Json<ListBucketsResponse>> {
    let repo = BucketRepository::new(state.catalog.pool().clone());
    let buckets = repo.list().await?;

    let bucket_infos: Vec<BucketInfo> = buckets
        .into_iter()
        .map(|bucket| BucketInfo {
            name: bucket.name,
            creation_date: bucket.created_at,
        })
        .collect();

    let response = ListBucketsResponse {
        owner: Owner {
            id: "ghostbay".to_string(),
            display_name: "GhostBay".to_string(),
        },
        buckets: Buckets {
            bucket: bucket_infos,
        },
    };

    Ok(Json(response))
}

pub async fn create_bucket(
    Path(bucket_name): Path<String>,
    State(state): State<AppState>,
    _headers: S3Headers,
) -> ApiResult<Response> {
    validate_bucket_name(&bucket_name)?;

    let repo = BucketRepository::new(state.catalog.pool().clone());

    // Check if bucket already exists
    if repo.find_by_name(&bucket_name).await?.is_some() {
        return Err(ApiError::BucketAlreadyExists(bucket_name));
    }

    let request = CreateBucketRequest {
        name: bucket_name.clone(),
        region: "us-east-1".to_string(),
    };

    repo.create(request).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Location", format!("/{}", bucket_name))
        .body(Body::empty())
        .unwrap())
}

pub async fn delete_bucket(
    Path(bucket_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Response> {
    let repo = BucketRepository::new(state.catalog.pool().clone());

    if !repo.delete(&bucket_name).await? {
        return Err(ApiError::BucketNotFound(bucket_name));
    }

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap())
}

pub async fn list_objects(
    Path(bucket_name): Path<String>,
    Query(query): Query<ListObjectsQuery>,
    State(state): State<AppState>,
) -> ApiResult<Json<ListObjectsV2Response>> {
    let bucket_repo = BucketRepository::new(state.catalog.pool().clone());
    let bucket = bucket_repo
        .find_by_name(&bucket_name)
        .await?
        .ok_or_else(|| ApiError::BucketNotFound(bucket_name.clone()))?;

    let object_repo = ObjectRepository::new(state.catalog.pool().clone());
    let objects = object_repo
        .list_by_bucket(bucket.id, query.prefix.as_deref(), query.max_keys.map(|k| k as i32))
        .await?;

    let object_infos: Vec<ObjectInfo> = objects
        .into_iter()
        .map(|obj| ObjectInfo {
            key: obj.key,
            last_modified: obj.updated_at,
            etag: obj.etag,
            size: obj.size as u64,
            storage_class: "STANDARD".to_string(),
            owner: Owner {
                id: "ghostbay".to_string(),
                display_name: "GhostBay".to_string(),
            },
        })
        .collect();

    let response = ListObjectsV2Response {
        name: bucket_name,
        prefix: query.prefix,
        key_count: object_infos.len() as u32,
        max_keys: query.max_keys.unwrap_or(1000),
        is_truncated: false, // TODO: Implement pagination
        continuation_token: query.continuation_token,
        next_continuation_token: None,
        contents: object_infos,
    };

    Ok(Json(response))
}

pub async fn put_object(
    Path((bucket_name, key)): Path<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let bucket_repo = BucketRepository::new(state.catalog.pool().clone());
    let bucket = bucket_repo
        .find_by_name(&bucket_name)
        .await?
        .ok_or_else(|| ApiError::BucketNotFound(bucket_name.clone()))?;

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("binary/octet-stream")
        .to_string();

    let content_length = body.len() as u64;

    // Create a stream from the bytes
    let stream = futures::stream::once(async move { Ok(body) });
    let boxed_stream = Box::pin(stream);

    let storage_request = PutObjectRequest {
        bucket: bucket_name.clone(),
        key: key.clone(),
        content_type: content_type.clone(),
        content_length: Some(content_length),
        data: boxed_stream,
    };

    let etag = state.storage.put_object(storage_request).await
        .map_err(|e| ApiError::Storage(e.to_string()))?;

    // Store metadata in catalog
    let object_repo = ObjectRepository::new(state.catalog.pool().clone());
    let storage_path = format!("{}/{}", bucket_name, key);
    
    let create_request = CreateObjectRequest {
        bucket_id: bucket.id,
        key: key.clone(),
        content_type,
        size: content_length as i64,
        storage_path,
        metadata: None,
    };

    object_repo.create(create_request, etag.clone()).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("ETag", format!("\"{}\"", etag))
        .body(Body::empty())
        .unwrap())
}

pub async fn get_object(
    Path((bucket_name, key)): Path<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let bucket_repo = BucketRepository::new(state.catalog.pool().clone());
    let bucket = bucket_repo
        .find_by_name(&bucket_name)
        .await?
        .ok_or_else(|| ApiError::BucketNotFound(bucket_name.clone()))?;

    let object_repo = ObjectRepository::new(state.catalog.pool().clone());
    let _object = object_repo
        .find_by_bucket_and_key(bucket.id, &key)
        .await?
        .ok_or_else(|| ApiError::ObjectNotFound(key.clone()))?;

    // Parse range header if present
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_range_header);

    let get_request = GetObjectRequest {
        bucket: bucket_name,
        key: key.clone(),
        range,
    };

    let storage_response = state.storage
        .get_object(get_request)
        .await
        .map_err(|e| ApiError::Storage(e.to_string()))?
        .ok_or_else(|| ApiError::ObjectNotFound(key))?;

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", storage_response.metadata.content_type)
        .header("Content-Length", storage_response.metadata.content_length.to_string())
        .header("ETag", storage_response.metadata.etag)
        .header("Last-Modified", storage_response.metadata.last_modified.format("%a, %d %b %Y %H:%M:%S GMT").to_string());

    // Convert the stream to a Body
    let stream = storage_response.data.map(|result| {
        result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    });

    let body = Body::from_stream(stream);
    let response = response.body(body).unwrap();

    Ok(response)
}

pub async fn head_object(
    Path((bucket_name, key)): Path<(String, String)>,
    State(state): State<AppState>,
) -> ApiResult<Response> {
    let bucket_repo = BucketRepository::new(state.catalog.pool().clone());
    let bucket = bucket_repo
        .find_by_name(&bucket_name)
        .await?
        .ok_or_else(|| ApiError::BucketNotFound(bucket_name.clone()))?;

    let object_repo = ObjectRepository::new(state.catalog.pool().clone());
    let _object = object_repo
        .find_by_bucket_and_key(bucket.id, &key)
        .await?
        .ok_or_else(|| ApiError::ObjectNotFound(key.clone()))?;

    let metadata = state.storage
        .head_object(&bucket_name, &key)
        .await
        .map_err(|e| ApiError::Storage(e.to_string()))?
        .ok_or_else(|| ApiError::ObjectNotFound(key))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", metadata.content_type)
        .header("Content-Length", metadata.content_length.to_string())
        .header("ETag", metadata.etag)
        .header("Last-Modified", metadata.last_modified.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
        .body(Body::empty())
        .unwrap())
}

pub async fn delete_object(
    Path((bucket_name, key)): Path<(String, String)>,
    State(state): State<AppState>,
) -> ApiResult<Response> {
    let bucket_repo = BucketRepository::new(state.catalog.pool().clone());
    let bucket = bucket_repo
        .find_by_name(&bucket_name)
        .await?
        .ok_or_else(|| ApiError::BucketNotFound(bucket_name.clone()))?;

    // Delete from catalog first
    let object_repo = ObjectRepository::new(state.catalog.pool().clone());
    object_repo.delete(bucket.id, &key).await?;

    // Delete from storage
    state.storage
        .delete_object(&bucket_name, &key)
        .await
        .map_err(|e| ApiError::Storage(e.to_string()))?;

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap())
}

fn validate_bucket_name(name: &str) -> ApiResult<()> {
    if name.is_empty() || name.len() < 3 || name.len() > 63 {
        return Err(ApiError::InvalidBucketName(
            "Bucket name must be between 3 and 63 characters long".to_string(),
        ));
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ApiError::InvalidBucketName(
            "Bucket name can only contain lowercase letters, numbers, and hyphens".to_string(),
        ));
    }

    Ok(())
}

fn parse_range_header(range: &str) -> Option<(u64, Option<u64>)> {
    if !range.starts_with("bytes=") {
        return None;
    }

    let range = &range[6..]; // Remove "bytes="
    let parts: Vec<&str> = range.split('-').collect();

    if parts.len() != 2 {
        return None;
    }

    let start = parts[0].parse::<u64>().ok()?;
    let end = if parts[1].is_empty() {
        None
    } else {
        parts[1].parse::<u64>().ok()
    };

    Some((start, end))
}

// Multipart Upload Handlers

pub async fn create_multipart_upload(
    Path((bucket_name, key)): Path<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<crate::responses::InitiateMultipartUploadResponse>> {
    let bucket_repo = BucketRepository::new(state.catalog.pool().clone());
    let bucket = bucket_repo
        .find_by_name(&bucket_name)
        .await?
        .ok_or_else(|| ApiError::BucketNotFound(bucket_name.clone()))?;

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("binary/octet-stream")
        .to_string();

    // Extract metadata from x-amz-meta- headers
    let mut metadata = serde_json::Map::new();
    for (header_name, header_value) in headers.iter() {
        if let Some(name) = header_name.as_str().strip_prefix("x-amz-meta-") {
            if let Ok(value) = header_value.to_str() {
                metadata.insert(name.to_string(), serde_json::Value::String(value.to_string()));
            }
        }
    }

    let storage_request = CreateMultipartUploadRequest {
        bucket: bucket_name.clone(),
        key: key.clone(),
        content_type: content_type.clone(),
        metadata: if metadata.is_empty() { None } else { Some(serde_json::Value::Object(metadata)) },
    };

    let upload_id = state.storage.create_multipart_upload(storage_request).await
        .map_err(|e| ApiError::Storage(e.to_string()))?;

    // Store upload in database
    let multipart_repo = MultipartUploadRepository::new(state.catalog.pool().clone());
    let _multipart_upload = multipart_repo.create(bucket.id, &key, &upload_id).await?;

    let response = crate::responses::InitiateMultipartUploadResponse {
        bucket: bucket_name,
        key,
        upload_id,
    };

    Ok(Json(response))
}

pub async fn upload_part(
    Path((bucket_name, key)): Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let upload_id = params.get("uploadId")
        .ok_or_else(|| ApiError::BadRequest("Missing uploadId parameter".to_string()))?;
    
    let part_number: i32 = params.get("partNumber")
        .ok_or_else(|| ApiError::BadRequest("Missing partNumber parameter".to_string()))?
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid partNumber".to_string()))?;

    if part_number < 1 || part_number > 10000 {
        return Err(ApiError::BadRequest("Part number must be between 1 and 10000".to_string()));
    }

    // Verify upload exists
    let multipart_repo = MultipartUploadRepository::new(state.catalog.pool().clone());
    let upload = multipart_repo.find_by_upload_id(upload_id).await?
        .ok_or_else(|| ApiError::BadRequest("Upload not found".to_string()))?;

    // Save body length before moving it
    let body_len = body.len() as i64;
    
    // Create a stream from the bytes
    let stream = futures::stream::once(async move { Ok(body) });
    let boxed_stream = Box::pin(stream);

    let storage_request = UploadPartRequest {
        bucket: bucket_name,
        key,
        upload_id: upload_id.clone(),
        part_number,
        data: boxed_stream,
    };

    let etag = state.storage.upload_part(storage_request).await
        .map_err(|e| ApiError::Storage(e.to_string()))?;

    // Store part in database
    let part_repo = MultipartPartRepository::new(state.catalog.pool().clone());
    let storage_path = format!("{}/part_{:05}", upload_id, part_number);
    let _part = part_repo.create(upload.id, part_number, etag.clone(), body_len, storage_path).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("ETag", format!("\"{}\"", etag))
        .body(Body::empty())
        .unwrap())
}

pub async fn complete_multipart_upload(
    Path((bucket_name, key)): Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
    Json(request): Json<crate::responses::CompleteMultipartUploadRequest>,
) -> ApiResult<Json<crate::responses::CompleteMultipartUploadResponse>> {
    let upload_id = params.get("uploadId")
        .ok_or_else(|| ApiError::BadRequest("Missing uploadId parameter".to_string()))?;

    // Verify upload exists and get bucket
    let multipart_repo = MultipartUploadRepository::new(state.catalog.pool().clone());
    let upload = multipart_repo.find_by_upload_id(upload_id).await?
        .ok_or_else(|| ApiError::BadRequest("Upload not found".to_string()))?;

    let bucket_repo = BucketRepository::new(state.catalog.pool().clone());
    let bucket = bucket_repo
        .find_by_name(&bucket_name)
        .await?
        .ok_or_else(|| ApiError::BucketNotFound(bucket_name.clone()))?;

    if upload.bucket_id != bucket.id || upload.object_key != key {
        return Err(ApiError::BadRequest("Upload bucket/key mismatch".to_string()));
    }

    // Convert request parts to storage format
    let parts: Vec<MultipartUploadPart> = request.complete_multipart_upload.part
        .into_iter()
        .map(|p| MultipartUploadPart {
            part_number: p.part_number,
            etag: p.etag.trim_matches('"').to_string(), // Remove quotes if present
            size: 0, // Size will be determined by storage engine
        })
        .collect();

    let storage_request = CompleteMultipartUploadRequest {
        bucket: bucket_name.clone(),
        key: key.clone(),
        upload_id: upload_id.clone(),
        parts,
    };

    let etag = state.storage.complete_multipart_upload(storage_request).await
        .map_err(|e| ApiError::Storage(e.to_string()))?;

    // Create object record in catalog
    let object_repo = ObjectRepository::new(state.catalog.pool().clone());
    let storage_path = format!("{}/{}", bucket_name, key);
    
    // Calculate total size from parts
    let part_repo = MultipartPartRepository::new(state.catalog.pool().clone());
    let parts_list = part_repo.list_by_upload(upload.id).await?;
    let total_size: i64 = parts_list.iter().map(|p| p.size).sum();
    
    let create_request = CreateObjectRequest {
        bucket_id: bucket.id,
        key: key.clone(),
        content_type: "binary/octet-stream".to_string(), // TODO: Get from upload metadata
        size: total_size,
        storage_path,
        metadata: None, // TODO: Get from upload metadata
    };

    object_repo.create(create_request, etag.clone()).await?;

    // Clean up multipart upload records
    part_repo.delete_by_upload(upload.id).await?;
    multipart_repo.delete(upload_id).await?;

    let location = format!("https://{}.s3.amazonaws.com/{}", bucket_name, key);
    let response = crate::responses::CompleteMultipartUploadResponse {
        location,
        bucket: bucket_name,
        key,
        etag: format!("\"{}\"", etag),
    };

    Ok(Json(response))
}

pub async fn abort_multipart_upload(
    Path((bucket_name, key)): Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
) -> ApiResult<Response> {
    let upload_id = params.get("uploadId")
        .ok_or_else(|| ApiError::BadRequest("Missing uploadId parameter".to_string()))?;

    // Verify upload exists
    let multipart_repo = MultipartUploadRepository::new(state.catalog.pool().clone());
    let upload = multipart_repo.find_by_upload_id(upload_id).await?
        .ok_or_else(|| ApiError::BadRequest("Upload not found".to_string()))?;

    // Clean up storage
    state.storage.abort_multipart_upload(&bucket_name, &key, upload_id).await
        .map_err(|e| ApiError::Storage(e.to_string()))?;

    // Clean up database records
    let part_repo = MultipartPartRepository::new(state.catalog.pool().clone());
    part_repo.delete_by_upload(upload.id).await?;
    multipart_repo.delete(upload_id).await?;

    Ok(Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap())
}

pub async fn put_object_or_part(
    Path((bucket_name, key)): Path<(String, String)>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Response> {
    // Convert Body to Bytes
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => return Err(ApiError::BadRequest(format!("Failed to read body: {}", e))),
    };

    if query.contains_key("uploadId") && query.contains_key("partNumber") {
        match upload_part(Path((bucket_name, key)), query, State(state), headers, bytes).await {
            Ok(json_response) => Ok((StatusCode::OK, json_response).into_response()),
            Err(e) => Err(e),
        }
    } else {
        match put_object(Path((bucket_name, key)), State(state), headers, bytes).await {
            Ok(json_response) => Ok((StatusCode::OK, json_response).into_response()),
            Err(e) => Err(e),
        }
    }
}

pub async fn create_multipart_upload_or_complete(
    Path((bucket_name, key)): Path<(String, String)>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Response> {
    if query.contains_key("uploads") {
        match create_multipart_upload(Path((bucket_name, key)), State(state), headers).await {
            Ok(json_response) => Ok((StatusCode::OK, json_response).into_response()),
            Err(e) => Err(e),
        }
    } else if query.contains_key("uploadId") {
        // Convert Body to JSON for complete_multipart_upload
        let bytes = match axum::body::to_bytes(body, usize::MAX).await {
            Ok(bytes) => bytes,
            Err(e) => return Err(ApiError::BadRequest(format!("Failed to read body: {}", e))),
        };
        
        let request: crate::responses::CompleteMultipartUploadRequest = match serde_json::from_slice(&bytes) {
            Ok(req) => req,
            Err(e) => return Err(ApiError::BadRequest(format!("Invalid JSON: {}", e))),
        };
        
        match complete_multipart_upload(Path((bucket_name, key)), query, State(state), axum::Json(request)).await {
            Ok(json_response) => Ok((StatusCode::OK, json_response).into_response()),
            Err(e) => Err(e),
        }
    } else {
        Err(ApiError::BadRequest("Invalid POST operation".to_string()))
    }
}

pub async fn delete_object_or_abort_upload(
    Path((bucket_name, key)): Path<(String, String)>,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
) -> ApiResult<Response> {
    if query.contains_key("uploadId") {
        abort_multipart_upload(Path((bucket_name, key)), query, State(state)).await
    } else {
        delete_object(Path((bucket_name, key)), State(state)).await
    }
}