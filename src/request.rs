use nu_protocol::{Record, Span, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

/// Resolve client IP from X-Forwarded-For header using trusted proxy list.
/// Parses right-to-left, stopping at first untrusted IP.
/// Falls back to remote_ip if no valid header or all IPs are trusted proxies.
pub fn resolve_trusted_ip(
    headers: &http::header::HeaderMap,
    remote_ip: Option<IpAddr>,
    trusted_proxies: &[ipnet::IpNet],
) -> Option<IpAddr> {
    // If no trusted proxies configured, just use remote_ip
    if trusted_proxies.is_empty() {
        return remote_ip;
    }

    // Check if remote_ip itself is trusted
    let remote_is_trusted = remote_ip
        .map(|ip| trusted_proxies.iter().any(|net| net.contains(&ip)))
        .unwrap_or(false);

    if !remote_is_trusted {
        return remote_ip;
    }

    // Get X-Forwarded-For header
    let xff = match headers.get("x-forwarded-for") {
        Some(v) => v.to_str().ok()?,
        None => return remote_ip,
    };

    // Parse IPs from right to left
    let ips: Vec<&str> = xff.split(',').map(|s| s.trim()).collect();

    for ip_str in ips.into_iter().rev() {
        if let Ok(ip) = ip_str.parse::<IpAddr>() {
            // If this IP is not in trusted proxies, it's the client
            if !trusted_proxies.iter().any(|net| net.contains(&ip)) {
                return Some(ip);
            }
        }
    }

    // All IPs were trusted proxies, fall back to remote_ip
    remote_ip
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub proto: String,
    #[serde(with = "http_serde::method")]
    pub method: http::method::Method,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_ip: Option<std::net::IpAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    /// Client IP resolved from X-Forwarded-For using trusted proxy list, or remote_ip as fallback
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted_ip: Option<std::net::IpAddr>,
    #[serde(with = "http_serde::header_map")]
    pub headers: http::header::HeaderMap,
    #[serde(with = "http_serde::uri")]
    pub uri: http::Uri,
    pub path: String,
    pub query: HashMap<String, String>,
}

pub fn request_to_value(request: &Request, span: Span) -> Value {
    let mut record = Record::new();

    record.push("proto", Value::string(request.proto.clone(), span));
    record.push("method", Value::string(request.method.to_string(), span));
    record.push("uri", Value::string(request.uri.to_string(), span));
    record.push("path", Value::string(request.path.clone(), span));

    if let Some(authority) = &request.authority {
        record.push("authority", Value::string(authority.clone(), span));
    }

    if let Some(remote_ip) = &request.remote_ip {
        record.push("remote_ip", Value::string(remote_ip.to_string(), span));
    }

    if let Some(remote_port) = &request.remote_port {
        record.push("remote_port", Value::int(*remote_port as i64, span));
    }

    if let Some(trusted_ip) = &request.trusted_ip {
        record.push("trusted_ip", Value::string(trusted_ip.to_string(), span));
    }

    // Convert headers to a record
    let mut headers_record = Record::new();
    for (key, value) in request.headers.iter() {
        headers_record.push(
            key.to_string(),
            Value::string(value.to_str().unwrap_or_default().to_string(), span),
        );
    }
    record.push("headers", Value::record(headers_record, span));

    // Convert query parameters to a record
    let mut query_record = Record::new();
    for (key, value) in &request.query {
        query_record.push(key.clone(), Value::string(value.clone(), span));
    }
    record.push("query", Value::record(query_record, span));

    Value::record(record, span)
}
