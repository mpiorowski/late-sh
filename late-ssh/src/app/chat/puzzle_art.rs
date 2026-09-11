//! Submission parsing and image preparation for #puzzle-art. Publication and
//! approval live in late-core; all downloads reuse the guarded image loader.
use std::{io::Cursor, time::Duration};

use anyhow::{Context, Result, bail};
use image::{ImageFormat, ImageReader};
use late_core::models::sliding_puzzle_artwork::Submission;
use reqwest::Url;
use sha2::{Digest, Sha256};

use crate::{
    app::files::image_upload,
    config::{FilesConfig, MAX_IMAGE_BYTES},
};

pub(crate) fn parse_submission(body: &str) -> Result<(String, String)> {
    let mut url = None;
    let mut title_words = Vec::new();
    for word in body.split_whitespace() {
        if word.starts_with("https://") || word.starts_with("http://") {
            if url.is_some() {
                bail!("Post exactly one image URL per submission.");
            }
            let parsed = Url::parse(word).context("Use a direct public image URL.")?;
            if parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                bail!("Use a public image URL without credentials.");
            }
            url = Some(parsed.to_string());
        } else {
            title_words.push(word);
        }
    }
    let url = url.context("Post one image URL, optionally with a title. Upload or paste an image first to get its URL; /rules explains how.")?;
    let title = if title_words.is_empty() {
        "Community artwork".to_string()
    } else {
        title_words.join(" ")
    };
    if title.chars().count() > 120 || title.chars().any(char::is_control) {
        bail!("Keep the artwork title to 120 characters without control characters.");
    }
    Ok((url, title))
}

/// Decode before review, then re-encode a static PNG without source metadata.
/// The submitted chat message and the puzzle use this exact hosted image.
pub(crate) fn normalize_image(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.len() > MAX_IMAGE_BYTES {
        bail!("Images must be at most 10 MiB.");
    }
    let format = image::guess_format(&bytes).context("Use a PNG, JPEG or WebP image.")?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
    ) {
        bail!("Use a PNG, JPEG or WebP image.");
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    reader.limits(limits.clone());
    let (width, height) = reader
        .into_dimensions()
        .context("Could not read the image dimensions; use at most 4096 pixels per side.")?;
    if width != height || !(256..=4096).contains(&width) {
        bail!("Use a square image, 256–4096 pixels per side.");
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    reader.limits(limits);
    let decoded = reader.decode().context("The image could not be decoded.")?;
    let mut output = Cursor::new(Vec::new());
    decoded
        .write_to(&mut output, ImageFormat::Png)
        .context("The image could not be prepared.")?;
    let output = output.into_inner();
    if output.len() > MAX_IMAGE_BYTES {
        bail!("The prepared image exceeds 10 MiB; use a smaller image.");
    }
    Ok(output)
}

pub(crate) async fn prepare(files: Option<&FilesConfig>, body: &str) -> Result<Submission> {
    let (source_url, title) = parse_submission(body)?;
    let files =
        files.context("Image submissions are unavailable while upload storage is disabled.")?;
    let bytes =
        image_upload::download_url_bytes(&source_url, Duration::from_secs(30), MAX_IMAGE_BYTES)
            .await
            .context(
                "Could not fetch the image. Use a direct public image URL without redirects.",
            )?;
    let bytes = tokio::task::spawn_blocking(move || normalize_image(bytes)).await??;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let image_url = image_upload::upload_puzzle_artwork(files, bytes)
        .await
        .context("Could not store the image; try again.")?;
    Ok(Submission {
        source_url,
        image_url,
        sha256,
        title,
    })
}
