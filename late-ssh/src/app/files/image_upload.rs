use std::{
    env,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Utc};
use hmac::{Hmac, Mac};
use reqwest::{Url, redirect::Policy};
use sha2::{Digest, Sha256};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
const CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
pub(crate) const USER_AGENT: &str = "late-sh/1.0";

struct ValidatedDownloadUrl {
    url: Url,
    host: String,
    addrs: Vec<SocketAddr>,
}

#[derive(Debug, Clone)]
struct FileStorageConfig {
    endpoint: String,
    bucket: String,
    public_base_url: String,
    access_key_id: String,
    secret_access_key: String,
    region: String,
}

impl FileStorageConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            endpoint: env_required_any("LATE_FILES_S3_ENDPOINT", "S3_ENDPOINT")?,
            bucket: env_required("LATE_FILES_S3_BUCKET")?,
            public_base_url: env_required("LATE_FILES_PUBLIC_BASE_URL")?,
            access_key_id: env_required_any("LATE_FILES_S3_ACCESS_KEY_ID", "S3_ACCESS_KEY_ID")?,
            secret_access_key: env_required_any(
                "LATE_FILES_S3_SECRET_ACCESS_KEY",
                "S3_SECRET_ACCESS_KEY",
            )?,
            region: env::var("LATE_FILES_S3_REGION")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "auto".to_string()),
        })
    }
}

pub fn is_file_upload_configured() -> bool {
    FileStorageConfig::from_env().is_ok()
}

pub fn detect_image_mime(data: &[u8]) -> Option<&'static str> {
    match data {
        [0x89, 0x50, 0x4E, 0x47, ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        d if d.starts_with(b"GIF8") => Some("image/gif"),
        d if d.len() > 12 && d.starts_with(b"RIFF") && &d[8..12] == b"WEBP" => Some("image/webp"),
        _ => None,
    }
}

pub fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "bin",
    }
}

pub async fn download_and_reupload_url(url: String) -> Result<String> {
    let max_bytes = max_upload_bytes();
    let bytes = download_url_bytes(&url, Duration::from_secs(30), max_bytes).await?;

    let mime = detect_image_mime(&bytes)
        .ok_or_else(|| anyhow::anyhow!("url does not point to a supported image"))?;
    upload_image_bytes(bytes, mime).await
}

pub(crate) async fn download_url_bytes(
    raw_url: &str,
    timeout: Duration,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let validated = validate_download_url(raw_url).await?;
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .redirect(Policy::none());

    // Pin hostname requests to the IPs we already checked so a second DNS
    // lookup cannot return a private address after validation.
    if validated.host.parse::<IpAddr>().is_err() {
        builder = builder.resolve_to_addrs(&validated.host, &validated.addrs);
    }

    let client = builder.build()?;
    let resp = client.get(validated.url).send().await?;
    if resp.status().is_redirection() {
        bail!("redirects are not allowed");
    }
    if !resp.status().is_success() {
        bail!("download failed: http {}", resp.status());
    }

    read_response_limited(resp, max_bytes).await
}

pub async fn upload_image_bytes(data: Vec<u8>, mime: &str) -> Result<String> {
    let max_bytes = max_upload_bytes();
    ensure_upload_size(data.len(), max_bytes)?;

    let detected_mime =
        detect_image_mime(&data).ok_or_else(|| anyhow::anyhow!("unsupported image type"))?;
    if detected_mime != mime {
        tracing::debug!(
            provided_mime = mime,
            detected_mime,
            "using detected image MIME type"
        );
    }

    let config = FileStorageConfig::from_env()
        .context("file upload storage is not configured; missing LATE_FILES_* env")?;
    let now = Utc::now();
    let key = format!(
        "chat/{:04}/{:02}/{}.{}",
        now.year(),
        now.month(),
        Uuid::now_v7(),
        ext_for_mime(detected_mime)
    );

    put_object(&config, &key, data, detected_mime, now).await?;
    Ok(public_url(&config, &key))
}

async fn put_object(
    config: &FileStorageConfig,
    key: &str,
    data: Vec<u8>,
    mime: &str,
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    let upload_url = format!(
        "{}/{}/{}",
        config.endpoint.trim_end_matches('/'),
        config.bucket,
        key
    );
    let parsed_url = Url::parse(&upload_url).context("invalid LATE_FILES_S3_ENDPOINT")?;
    let host = canonical_host(&parsed_url)?;
    let date_stamp = now.format("%Y%m%d").to_string();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let payload_hash = sha256_hex(&data);
    let canonical_uri = format!("/{}/{}", config.bucket, key);
    let credential_scope = format!("{}/{}/s3/aws4_request", date_stamp, config.region);
    let signed_headers = "cache-control;content-type;host;x-amz-content-sha256;x-amz-date";
    let canonical_headers = format!(
        "cache-control:{CACHE_CONTROL}\ncontent-type:{mime}\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let canonical_request =
        format!("PUT\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = signing_key(&config.secret_access_key, &date_stamp, &config.region, "s3");
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        config.access_key_id
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()?;
    let resp = client
        .put(parsed_url)
        .header("authorization", authorization)
        .header("cache-control", CACHE_CONTROL)
        .header("content-type", mime)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date)
        .body(data)
        .send()
        .await?;

    if resp.status().is_success() {
        tracing::info!(key, "uploaded image to R2");
        return Ok(());
    }

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    bail!("r2 upload failed: http {} {}", status, body.trim());
}

fn env_required(key: &str) -> Result<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{key} is not set"))
}

fn env_required_any(primary: &str, fallback: &str) -> Result<String> {
    env_required(primary).or_else(|_| env_required(fallback))
}

pub(crate) fn max_upload_bytes() -> usize {
    env::var("LATE_FILES_MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_UPLOAD_BYTES)
}

fn ensure_upload_size(len: usize, max: usize) -> Result<()> {
    if len > max {
        bail!("image is too large (max {})", format_bytes(max));
    }
    Ok(())
}

fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MiB", bytes / (1024 * 1024))
    } else {
        format!("{} KiB", bytes / 1024)
    }
}

async fn validate_download_url(raw_url: &str) -> Result<ValidatedDownloadUrl> {
    let url = Url::parse(raw_url).context("invalid url")?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("url must start with http(s)://");
    }
    let Some(host) = url.host_str() else {
        bail!("url must include a host");
    };
    let host = host.to_string();
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        bail!("localhost urls are not allowed");
    }
    if let Ok(ip) = host.parse::<IpAddr>()
        && is_blocked_ip(ip)
    {
        bail!("private network urls are not allowed");
    }
    let port = url.port_or_known_default().context("url must include a port")?;
    let addrs = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::net::lookup_host((host.as_str(), port))
            .await
            .with_context(|| format!("failed to resolve {host}"))?
            .collect::<Vec<_>>()
    };
    if addrs.is_empty() {
        bail!("url host did not resolve");
    }
    if addrs.iter().any(|addr| is_blocked_ip(addr.ip())) {
        bail!("private network urls are not allowed");
    }

    Ok(ValidatedDownloadUrl {
        url,
        host,
        addrs,
    })
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 198 && (18..=19).contains(&octets[1]))
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
        }
    }
}

async fn read_response_limited(mut resp: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
    if let Some(len) = resp.content_length()
        && len > max_bytes as u64
    {
        bail!("image is too large (max {})", format_bytes(max_bytes));
    }

    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if out.len().saturating_add(chunk.len()) > max_bytes {
            bail!("image is too large (max {})", format_bytes(max_bytes));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

fn public_url(config: &FileStorageConfig, key: &str) -> String {
    format!("{}/{}", config.public_base_url.trim_end_matches('/'), key)
}

fn canonical_host(url: &Url) -> Result<String> {
    let host = url.host_str().context("S3 endpoint must include a host")?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

fn signing_key(secret_access_key: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(
        format!("AWS4{secret_access_key}").as_bytes(),
        date.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
