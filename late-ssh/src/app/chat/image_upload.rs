use anyhow::{bail, Result};

pub fn detect_image_mime(data: &[u8]) -> Option<&'static str> {
    match data {
        d if d.starts_with(&[0x89, 0x50, 0x4E, 0x47]) => Some("image/png"),
        d if d.starts_with(&[0xFF, 0xD8, 0xFF]) => Some("image/jpeg"),
        d if d.starts_with(b"GIF8") => Some("image/gif"),
        d if d.len() > 12 && d.starts_with(b"RIFF") && &d[8..12] == b"WEBP" => {
            Some("image/webp")
        }
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

/// Download an image from `url` and re-upload it to 0x0.st (catbox fallback).
/// Returns the final hosting URL.
pub async fn download_and_reupload_url(url: String) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("late-sh/1.0")
        .build()?;
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        bail!("Download failed: HTTP {}", resp.status());
    }
    let bytes = resp.bytes().await?.to_vec();
    let mime = detect_image_mime(&bytes)
        .ok_or_else(|| anyhow::anyhow!("L'URL ne pointe pas vers une image reconnue (PNG/JPEG/GIF/WebP)"))?;
    upload_image_bytes(bytes, mime).await
}

pub async fn upload_image_bytes(data: Vec<u8>, mime: &str) -> Result<String> {
    let ext = ext_for_mime(mime);
    let filename = format!("upload.{ext}");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("late-sh/1.0")
        .build()?;

    let part = reqwest::multipart::Part::bytes(data)
        .file_name(filename)
        .mime_str(mime)?;

    let form = reqwest::multipart::Form::new().part("files[]", part);

    let resp = client
        .post("https://uguu.se/upload")
        .multipart(form)
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!("uguu.se HTTP {}", resp.status());
    }

    let text = resp.text().await?;
    tracing::info!("uguu.se response: {}", text);
    
    #[derive(serde::Deserialize)]
    struct UguuResponse {
        files: Vec<UguuFile>,
    }
    #[derive(serde::Deserialize)]
    struct UguuFile {
        url: String,
    }

    if let Ok(json) = serde_json::from_str::<UguuResponse>(&text) {
        if let Some(file) = json.files.first() {
            return Ok(file.url.clone());
        }
    }

    // Improved naive JSON parsing fallback
    if let Some(url_start) = text.find("\"url\"") {
        let after_key = &text[url_start..];
        if let Some(colon_pos) = after_key.find(':') {
            let after_colon = &after_key[colon_pos + 1..];
            if let Some(quote_start) = after_colon.find('"') {
                let start = quote_start + 1;
                if let Some(quote_end) = after_colon[start..].find('"') {
                    let mut url = after_colon[start..start + quote_end].to_string();
                    url = url.replace("\\/", "/");
                    tracing::info!("Extracted URL: {}", url);
                    return Ok(url);
                }
            }
        }
    }

    bail!("Failed to parse uguu.se response: {}", text)
}
