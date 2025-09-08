use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use ring::{digest, hmac};
use std::collections::HashMap;

pub struct SigV4Validator;

impl SigV4Validator {
    pub fn validate_signature(
        secret_key: &str,
        access_key: &str,
        method: &str,
        uri: &str,
        query_string: &str,
        headers: &HashMap<String, String>,
        payload_hash: &str,
        signature: &str,
        timestamp: DateTime<Utc>,
        region: &str,
        service: &str,
    ) -> Result<bool> {
        // Validate timestamp (within 15 minutes)
        let now = Utc::now();
        let max_age = Duration::minutes(15);
        if (now - timestamp).abs() > max_age {
            return Err(anyhow::anyhow!("Request timestamp too old"));
        }

        let canonical_request = Self::create_canonical_request(
            method, uri, query_string, headers, payload_hash
        );

        let string_to_sign = Self::create_string_to_sign(
            &canonical_request, timestamp, region, service
        );

        let signing_key = Self::get_signing_key(
            secret_key, timestamp, region, service
        )?;

        let expected_signature = Self::calculate_signature(&signing_key, &string_to_sign);

        Ok(expected_signature == signature)
    }

    pub fn generate_presigned_url(
        secret_key: &str,
        access_key: &str,
        method: &str,
        bucket: &str,
        key: &str,
        expires_in_seconds: u64,
        region: &str,
        service: &str,
        host: &str,
    ) -> Result<String> {
        let now = Utc::now();
        let expires = now + Duration::seconds(expires_in_seconds as i64);
        
        let mut query_params = HashMap::new();
        query_params.insert("X-Amz-Algorithm".to_string(), "AWS4-HMAC-SHA256".to_string());
        query_params.insert("X-Amz-Credential".to_string(), 
            format!("{}/{}/{}/{}/aws4_request", access_key, now.format("%Y%m%d"), region, service));
        query_params.insert("X-Amz-Date".to_string(), now.format("%Y%m%dT%H%M%SZ").to_string());
        query_params.insert("X-Amz-Expires".to_string(), expires_in_seconds.to_string());
        query_params.insert("X-Amz-SignedHeaders".to_string(), "host".to_string());

        let uri = format!("/{}/{}", bucket, key);
        let query_string = Self::build_query_string(&query_params);
        
        let headers = {
            let mut h = HashMap::new();
            h.insert("host".to_string(), host.to_string());
            h
        };

        let canonical_request = Self::create_canonical_request(
            method, &uri, &query_string, &headers, "UNSIGNED-PAYLOAD"
        );

        let string_to_sign = Self::create_string_to_sign(
            &canonical_request, now, region, service
        );

        let signing_key = Self::get_signing_key(secret_key, now, region, service)?;
        let signature = Self::calculate_signature(&signing_key, &string_to_sign);

        Ok(format!("https://{}/{}?{}&X-Amz-Signature={}", host, uri.trim_start_matches('/'), query_string, signature))
    }

    fn create_canonical_request(
        method: &str,
        uri: &str,
        query_string: &str,
        headers: &HashMap<String, String>,
        payload_hash: &str,
    ) -> String {
        let canonical_uri = Self::canonical_uri_encode(uri);
        let canonical_query = Self::canonical_query_string(query_string);
        let (canonical_headers, signed_headers) = Self::canonical_headers(headers);

        format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            canonical_uri,
            canonical_query,
            canonical_headers,
            signed_headers,
            payload_hash
        )
    }

    fn create_string_to_sign(
        canonical_request: &str,
        timestamp: DateTime<Utc>,
        region: &str,
        service: &str,
    ) -> String {
        let algorithm = "AWS4-HMAC-SHA256";
        let credential_scope = format!(
            "{}/{}/{}/aws4_request",
            timestamp.format("%Y%m%d"),
            region,
            service
        );

        let hashed_canonical_request = hex::encode(digest::digest(&digest::SHA256, canonical_request.as_bytes()));

        format!(
            "{}\n{}\n{}\n{}",
            algorithm,
            timestamp.format("%Y%m%dT%H%M%SZ"),
            credential_scope,
            hashed_canonical_request
        )
    }

    fn get_signing_key(
        secret_key: &str,
        timestamp: DateTime<Utc>,
        region: &str,
        service: &str,
    ) -> Result<hmac::Key> {
        let k_secret = format!("AWS4{}", secret_key);
        let k_date = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, k_secret.as_bytes()),
            timestamp.format("%Y%m%d").to_string().as_bytes(),
        );

        let k_region = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, k_date.as_ref()),
            region.as_bytes(),
        );

        let k_service = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, k_region.as_ref()),
            service.as_bytes(),
        );

        let k_signing = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, k_service.as_ref()),
            b"aws4_request",
        );

        Ok(hmac::Key::new(hmac::HMAC_SHA256, k_signing.as_ref()))
    }

    fn calculate_signature(signing_key: &hmac::Key, string_to_sign: &str) -> String {
        let signature = hmac::sign(signing_key, string_to_sign.as_bytes());
        hex::encode(signature.as_ref())
    }

    fn canonical_uri_encode(uri: &str) -> String {
        if uri.is_empty() {
            "/".to_string()
        } else {
            // URI encode path segments but keep slashes
            uri.split('/')
                .map(|segment| urlencoding::encode(segment).to_string())
                .collect::<Vec<_>>()
                .join("/")
        }
    }

    fn canonical_query_string(query: &str) -> String {
        if query.is_empty() {
            return String::new();
        }

        let mut params: Vec<_> = query
            .split('&')
            .map(|param| {
                if let Some((key, value)) = param.split_once('=') {
                    format!("{}={}", urlencoding::encode(key), urlencoding::encode(value))
                } else {
                    format!("{}=", urlencoding::encode(param))
                }
            })
            .collect();

        params.sort();
        params.join("&")
    }

    fn canonical_headers(headers: &HashMap<String, String>) -> (String, String) {
        let mut sorted_headers: Vec<_> = headers
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.trim().to_string()))
            .collect();

        sorted_headers.sort_by(|a, b| a.0.cmp(&b.0));

        let canonical_headers = sorted_headers
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        let signed_headers = sorted_headers
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>()
            .join(";");

        (format!("{}\n", canonical_headers), signed_headers)
    }

    fn build_query_string(params: &HashMap<String, String>) -> String {
        let mut sorted_params: Vec<_> = params.iter().collect();
        sorted_params.sort_by(|a, b| a.0.cmp(b.0));
        
        sorted_params
            .into_iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

pub fn parse_authorization_header(auth_header: &str) -> Result<SigV4AuthInfo> {
    if !auth_header.starts_with("AWS4-HMAC-SHA256 ") {
        return Err(anyhow::anyhow!("Invalid authorization header format"));
    }

    let auth_parts = auth_header.strip_prefix("AWS4-HMAC-SHA256 ").unwrap();
    let mut credential = None;
    let mut signed_headers = None;
    let mut signature = None;

    for part in auth_parts.split(", ") {
        if let Some((key, value)) = part.split_once('=') {
            match key {
                "Credential" => credential = Some(value.to_string()),
                "SignedHeaders" => signed_headers = Some(value.to_string()),
                "Signature" => signature = Some(value.to_string()),
                _ => {}
            }
        }
    }

    let credential = credential.ok_or_else(|| anyhow::anyhow!("Missing Credential in authorization header"))?;
    let signed_headers = signed_headers.ok_or_else(|| anyhow::anyhow!("Missing SignedHeaders in authorization header"))?;
    let signature = signature.ok_or_else(|| anyhow::anyhow!("Missing Signature in authorization header"))?;

    let credential_parts: Vec<&str> = credential.split('/').collect();
    if credential_parts.len() != 5 {
        return Err(anyhow::anyhow!("Invalid credential format"));
    }

    Ok(SigV4AuthInfo {
        access_key_id: credential_parts[0].to_string(),
        date: credential_parts[1].to_string(),
        region: credential_parts[2].to_string(),
        service: credential_parts[3].to_string(),
        signed_headers: signed_headers.split(';').map(|s| s.to_string()).collect(),
        signature,
    })
}

#[derive(Debug, Clone)]
pub struct SigV4AuthInfo {
    pub access_key_id: String,
    pub date: String,
    pub region: String,
    pub service: String,
    pub signed_headers: Vec<String>,
    pub signature: String,
}

pub fn hash_payload(payload: &[u8]) -> String {
    let digest = digest::digest(&digest::SHA256, payload);
    hex::encode(digest.as_ref())
}