use axum::{
    async_trait,
    extract::{FromRequestParts, Query},
    http::{request::Parts, StatusCode},
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct ListObjectsQuery {
    #[serde(rename = "list-type")]
    pub list_type: Option<String>,
    pub prefix: Option<String>,
    #[serde(rename = "max-keys")]
    pub max_keys: Option<u32>,
    #[serde(rename = "continuation-token")]
    pub continuation_token: Option<String>,
    pub delimiter: Option<String>,
    #[serde(rename = "start-after")]
    pub start_after: Option<String>,
}

#[derive(Debug)]
pub struct S3Headers {
    pub headers: HashMap<String, String>,
}

#[async_trait]
impl<S> FromRequestParts<S> for S3Headers
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let mut headers = HashMap::new();
        
        for (name, value) in parts.headers.iter() {
            if let Ok(value_str) = value.to_str() {
                headers.insert(name.to_string(), value_str.to_string());
            }
        }

        Ok(S3Headers { headers })
    }
}