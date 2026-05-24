use std::{
    hash::{Hash, Hasher},
    io::Cursor,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{
    ExtendedColorType, GenericImageView, ImageEncoder, RgbaImage, codecs::png::PngEncoder,
};
use ratatui::layout::Rect;
use uuid::Uuid;

const KITTY_CHUNK_BYTES: usize = 4096;
const KITTY_LATE_IMAGE_ID_MIN: u32 = 0x4C00_0000;
const KITTY_LATE_IMAGE_ID_MAX: u32 = 0x4CFF_FFFF;
const KITTY_LATE_Z_INDEX: i32 = -1_024_076_853;
const MAX_DECODED_IMAGE_PIXELS: u64 = 25_000_000;
const TERMINAL_IMAGE_CELL_PIXEL_WIDTH: u32 = 8;
const TERMINAL_IMAGE_CELL_PIXEL_HEIGHT: u32 = 16;
const TERMINAL_COMMAND_CHUNK_BYTES: usize = 16 * 1024;
const SIXEL_ALPHA_THRESHOLD: u8 = 16;
const SIXEL_MAX_BYTES: usize = 2 * 1024 * 1024;
const SIXEL_PALETTE_LEVELS: &[u8] = &[6, 4, 3, 2];
const KITTY_PROTOCOL_IDENTITIES: &[&str] =
    &["kitty", "ghostty", "wezterm", "rio", "warp", "konsole"];
const ITERM2_PROTOCOL_IDENTITIES: &[&str] = &["iterm", "mintty", "hterm"];
const SIXEL_PROTOCOL_IDENTITIES: &[&str] =
    &["windows terminal", "foot", "contour", "mlterm", "sixel"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TerminalImageProtocol {
    Kitty,
    Iterm2,
    Sixel,
}

#[derive(Clone, Debug)]
pub struct TerminalImageData {
    pub png_bytes: Arc<Vec<u8>>,
    pub sixel_bytes: Option<Arc<Vec<u8>>>,
    pub display_cols: u16,
    pub display_rows: u16,
    cache_key: u64,
}

impl TerminalImageData {
    fn new(
        png_bytes: Vec<u8>,
        sixel_bytes: Option<Vec<u8>>,
        display_cols: u16,
        display_rows: u16,
    ) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        png_bytes.len().hash(&mut hasher);
        sixel_bytes.as_ref().map(Vec::len).hash(&mut hasher);
        display_cols.hash(&mut hasher);
        display_rows.hash(&mut hasher);
        png_bytes.hash(&mut hasher);
        sixel_bytes.hash(&mut hasher);
        Self {
            png_bytes: Arc::new(png_bytes),
            sixel_bytes: sixel_bytes.map(Arc::new),
            display_cols,
            display_rows,
            cache_key: hasher.finish(),
        }
    }

    pub(crate) fn cache_key(&self) -> u64 {
        self.cache_key
    }

    pub(crate) fn supports_protocol(&self, protocol: TerminalImageProtocol) -> bool {
        match protocol {
            TerminalImageProtocol::Kitty | TerminalImageProtocol::Iterm2 => {
                !self.png_bytes.is_empty()
            }
            TerminalImageProtocol::Sixel => self.sixel_bytes.is_some(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalImagePlacement {
    pub message_id: Uuid,
    pub area: Rect,
    pub data: TerminalImageData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalImagePlacementKey {
    message_id: Uuid,
    x: u16,
    y: u16,
    cols: u16,
    rows: u16,
    cache_key: u64,
}

#[derive(Default)]
pub struct TerminalImageFrame {
    placements: Vec<TerminalImagePlacement>,
}

impl TerminalImageFrame {
    pub(crate) fn push(&mut self, placement: TerminalImagePlacement) {
        self.placements.push(placement);
    }

    fn keys(&self) -> Vec<TerminalImagePlacementKey> {
        self.placements
            .iter()
            .map(|placement| TerminalImagePlacementKey {
                message_id: placement.message_id,
                x: placement.area.x,
                y: placement.area.y,
                cols: placement.area.width,
                rows: placement.area.height,
                cache_key: placement.data.cache_key(),
            })
            .collect()
    }
}

#[derive(Default)]
pub(crate) struct TerminalImageRenderState {
    protocol: Option<TerminalImageProtocol>,
    placements: Vec<TerminalImagePlacementKey>,
}

impl TerminalImageRenderState {
    pub(crate) fn build_commands(
        &mut self,
        protocol: Option<TerminalImageProtocol>,
        frame: &TerminalImageFrame,
    ) -> Vec<Vec<u8>> {
        if protocol.is_none() {
            let previous_had_kitty =
                self.protocol == Some(TerminalImageProtocol::Kitty) && !self.placements.is_empty();
            self.protocol = None;
            self.placements.clear();
            if previous_had_kitty {
                return kitty_cleanup_commands();
            }
            return Vec::new();
        }

        let keys = frame.keys();
        if self.protocol == protocol && self.placements == keys {
            return Vec::new();
        }

        let previous_had_kitty =
            self.protocol == Some(TerminalImageProtocol::Kitty) && !self.placements.is_empty();
        self.protocol = protocol;
        self.placements = keys;

        let Some(protocol) = protocol else {
            return Vec::new();
        };

        let mut commands = Vec::new();
        if previous_had_kitty || protocol == TerminalImageProtocol::Kitty {
            commands.extend(kitty_cleanup_commands());
        }

        for placement in &frame.placements {
            match protocol {
                TerminalImageProtocol::Kitty => {
                    commands.extend(kitty_image_commands(placement));
                }
                TerminalImageProtocol::Iterm2 => {
                    commands.extend(iterm2_image_commands(placement));
                }
                TerminalImageProtocol::Sixel => {
                    commands.extend(sixel_image_commands(placement));
                }
            }
        }
        commands
    }
}

pub(crate) async fn fetch_terminal_image(
    url: String,
    max_cols: u32,
    max_rows: u32,
    protocol: TerminalImageProtocol,
) -> Result<TerminalImageData> {
    tracing::trace!("attempting to render terminal image: {}", url);
    let bytes = crate::app::files::image_upload::download_url_bytes(
        &url,
        std::time::Duration::from_secs(15),
        crate::app::files::image_upload::max_upload_bytes(),
    )
    .await?;

    tokio::task::spawn_blocking(move || {
        terminal_image_from_bytes(&bytes, max_cols, max_rows, protocol)
    })
    .await?
}

fn terminal_image_from_bytes(
    bytes: &[u8],
    max_cols: u32,
    max_rows: u32,
    protocol: TerminalImageProtocol,
) -> Result<TerminalImageData> {
    let img = image::load_from_memory(bytes).context("failed to decode terminal image")?;
    let (width, height) = img.dimensions();
    if width == 0 || height == 0 {
        bail!("image has invalid dimensions");
    }
    if u64::from(width) * u64::from(height) > MAX_DECODED_IMAGE_PIXELS {
        bail!("image dimensions are too large");
    }

    let (display_cols, display_rows) = display_cells_for_image(width, height, max_cols, max_rows);
    let pixel_width = u32::from(display_cols)
        .saturating_mul(TERMINAL_IMAGE_CELL_PIXEL_WIDTH)
        .max(1);
    let pixel_height = u32::from(display_rows)
        .saturating_mul(TERMINAL_IMAGE_CELL_PIXEL_HEIGHT)
        .max(1);
    let resized = img.resize_exact(
        pixel_width,
        pixel_height,
        image::imageops::FilterType::Lanczos3,
    );
    let rgba = resized.to_rgba8();
    let mut png = Vec::new();
    {
        let encoder = PngEncoder::new(Cursor::new(&mut png));
        encoder
            .write_image(
                rgba.as_raw(),
                pixel_width,
                pixel_height,
                ExtendedColorType::Rgba8,
            )
            .context("failed to encode terminal image preview")?;
    }

    let sixel = if protocol == TerminalImageProtocol::Sixel {
        Some(encode_sixel_image(&rgba, pixel_width, pixel_height)?)
    } else {
        None
    };

    Ok(TerminalImageData::new(
        png,
        sixel,
        display_cols,
        display_rows,
    ))
}

fn display_cells_for_image(width: u32, height: u32, max_cols: u32, max_rows: u32) -> (u16, u16) {
    if width == 0 || height == 0 || max_cols == 0 || max_rows == 0 {
        return (1, 1);
    }

    let mut cols = width.min(max_cols).max(1);
    let mut rows = ((cols as f32 * height as f32 / width as f32) / 2.0)
        .ceil()
        .max(1.0) as u32;
    if rows > max_rows {
        rows = max_rows.max(1);
        cols = ((rows as f32 * 2.0 * width as f32 / height as f32)
            .ceil()
            .max(1.0) as u32)
            .min(max_cols)
            .max(1);
    }

    (
        cols.min(u32::from(u16::MAX)) as u16,
        rows.min(u32::from(u16::MAX)) as u16,
    )
}

pub(crate) fn protocol_from_term(term: &str) -> Option<TerminalImageProtocol> {
    protocol_from_identity(term)
}

pub(crate) fn protocol_from_terminal_program(program: &str) -> Option<TerminalImageProtocol> {
    protocol_from_identity(program)
}

pub(crate) fn protocol_from_xtversion(version: &str) -> Option<TerminalImageProtocol> {
    protocol_from_identity(version)
}

pub(crate) fn protocol_from_env_hint(name: &str, value: &str) -> Option<TerminalImageProtocol> {
    match name.trim() {
        "TERM_PROGRAM" | "LC_TERMINAL" => protocol_from_terminal_program(value),
        "TERM_FEATURES" => protocol_from_terminal_features(value),
        "KITTY_WINDOW_ID" | "KITTY_PID" | "KITTY_PUBLIC_KEY" => {
            non_empty_protocol(value, TerminalImageProtocol::Kitty)
        }
        "WEZTERM_PANE" | "WEZTERM_EXECUTABLE" => {
            non_empty_protocol(value, TerminalImageProtocol::Kitty)
        }
        "KONSOLE_VERSION" | "GHOSTTY_RESOURCES_DIR" | "GHOSTTY_BIN_DIR" => {
            non_empty_protocol(value, TerminalImageProtocol::Kitty)
        }
        "WT_SESSION" | "WT_PROFILE_ID" => non_empty_protocol(value, TerminalImageProtocol::Sixel),
        _ => None,
    }
}

pub(crate) fn protocol_from_terminal_features(features: &str) -> Option<TerminalImageProtocol> {
    if terminal_features_include_file(features) {
        Some(TerminalImageProtocol::Iterm2)
    } else {
        None
    }
}

fn protocol_from_identity(value: &str) -> Option<TerminalImageProtocol> {
    let value = value.trim().to_ascii_lowercase();
    if ITERM2_PROTOCOL_IDENTITIES
        .iter()
        .any(|identity| value.contains(identity))
    {
        Some(TerminalImageProtocol::Iterm2)
    } else if KITTY_PROTOCOL_IDENTITIES
        .iter()
        .any(|identity| value.contains(identity))
    {
        Some(TerminalImageProtocol::Kitty)
    } else if SIXEL_PROTOCOL_IDENTITIES
        .iter()
        .any(|identity| value.contains(identity))
    {
        Some(TerminalImageProtocol::Sixel)
    } else {
        None
    }
}

fn non_empty_protocol(
    value: &str,
    protocol: TerminalImageProtocol,
) -> Option<TerminalImageProtocol> {
    if value.trim().is_empty() {
        None
    } else {
        Some(protocol)
    }
}

fn terminal_features_include_file(features: &str) -> bool {
    features.chars().any(|ch| ch == 'F')
}

pub(crate) fn term_disables_terminal_images(term: &str) -> bool {
    let term = term.trim().to_ascii_lowercase();
    term.contains("tmux")
        || term == "screen"
        || term.starts_with("screen-")
        || term.starts_with("screen.")
}

pub(crate) fn xtversion_probe() -> Vec<u8> {
    b"\x1b[>q".to_vec()
}

pub(crate) fn iterm2_capabilities_probe() -> Vec<u8> {
    b"\x1b]1337;Capabilities\x1b\\".to_vec()
}

pub(crate) fn terminal_string_terminator() -> &'static [u8] {
    b"\x1b\\"
}

fn kitty_delete_command(control: impl AsRef<str>) -> Vec<u8> {
    format!("\x1b_G{}\x1b\\", control.as_ref()).into_bytes()
}

pub(crate) fn kitty_cleanup_commands() -> Vec<Vec<u8>> {
    kitty_cleanup_base_commands()
}

pub(crate) fn terminal_image_cleanup_commands() -> Vec<Vec<u8>> {
    kitty_cleanup_base_commands()
}

fn cursor_to(area: Rect) -> Vec<u8> {
    format!(
        "\x1b[{};{}H",
        area.y.saturating_add(1),
        area.x.saturating_add(1)
    )
    .into_bytes()
}

fn kitty_image_commands(placement: &TerminalImagePlacement) -> Vec<Vec<u8>> {
    let encoded = STANDARD.encode(placement.data.png_bytes.as_slice());
    let image_id = kitty_image_id(placement.message_id);
    let mut commands = Vec::new();
    commands.push(cursor_to(placement.area));

    let mut chunks = encoded.as_bytes().chunks(KITTY_CHUNK_BYTES).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = if chunks.peek().is_some() { 1 } else { 0 };
        let control = if first {
            first = false;
            format!(
                "a=T,f=100,q=2,i={image_id},p=1,z={},c={},r={},C=1,m={more}",
                KITTY_LATE_Z_INDEX, placement.area.width, placement.area.height
            )
        } else {
            format!("q=2,m={more}")
        };
        let mut command = format!("\x1b_G{control};").into_bytes();
        command.extend_from_slice(chunk);
        command.extend_from_slice(b"\x1b\\");
        commands.push(command);
    }

    commands
}

fn kitty_image_id(message_id: Uuid) -> u32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    message_id.hash(&mut hasher);
    KITTY_LATE_IMAGE_ID_MIN | ((hasher.finish() as u32) & 0x00FF_FFFF)
}

fn kitty_cleanup_base_commands() -> Vec<Vec<u8>> {
    vec![
        kitty_delete_command(format!("a=d,d=Z,z={KITTY_LATE_Z_INDEX},q=2")),
        kitty_delete_command(format!(
            "a=d,d=R,x={KITTY_LATE_IMAGE_ID_MIN},y={KITTY_LATE_IMAGE_ID_MAX},q=2"
        )),
    ]
}

fn iterm2_image_commands(placement: &TerminalImagePlacement) -> Vec<Vec<u8>> {
    let mut commands = vec![cursor_to(placement.area)];
    let encoded = STANDARD.encode(placement.data.png_bytes.as_slice());
    commands.push(
        format!(
            "\x1b]1337;File=inline=1;width={};height={};preserveAspectRatio=1;size={}:{}\x07",
            placement.area.width,
            placement.area.height,
            placement.data.png_bytes.len(),
            encoded
        )
        .into_bytes(),
    );
    commands
}

fn sixel_image_commands(placement: &TerminalImagePlacement) -> Vec<Vec<u8>> {
    let mut commands = vec![cursor_to(placement.area)];
    if placement.area.width == placement.data.display_cols
        && placement.area.height == placement.data.display_rows
        && let Some(sixel) = placement.data.sixel_bytes.as_deref()
    {
        push_chunked_terminal_command(&mut commands, sixel);
    }
    commands
}

fn push_chunked_terminal_command(commands: &mut Vec<Vec<u8>>, bytes: &[u8]) {
    commands.extend(
        bytes
            .chunks(TERMINAL_COMMAND_CHUNK_BYTES)
            .map(|chunk| chunk.to_vec()),
    );
}

fn encode_sixel_image(rgba: &RgbaImage, width: u32, height: u32) -> Result<Vec<u8>> {
    let mut fallback_len = 0;
    for levels in SIXEL_PALETTE_LEVELS {
        let encoded = encode_sixel_with_levels(rgba, width, height, *levels);
        if encoded.len() <= SIXEL_MAX_BYTES {
            return Ok(encoded);
        }
        fallback_len = encoded.len();
    }
    bail!("sixel image is too large ({fallback_len} bytes)")
}

fn encode_sixel_with_levels(rgba: &RgbaImage, width: u32, height: u32, levels: u8) -> Vec<u8> {
    let width_usize = width as usize;
    let height_usize = height as usize;
    let color_count = sixel_color_count(levels);
    let mut used_colors = vec![false; color_count];
    for pixel in rgba.pixels() {
        if let Some(index) = sixel_palette_index(pixel.0, levels) {
            used_colors[index] = true;
        }
    }

    let mut out = Vec::with_capacity((width_usize * height_usize / 2).max(128));
    out.extend_from_slice(b"\x1bPq");
    out.push(b'"');
    push_decimal(&mut out, 1);
    out.push(b';');
    push_decimal(&mut out, 1);
    out.push(b';');
    push_decimal(&mut out, width_usize);
    out.push(b';');
    push_decimal(&mut out, height_usize);

    for (index, used) in used_colors.iter().copied().enumerate() {
        if used {
            let (r, g, b) = sixel_palette_rgb(index, levels);
            push_sixel_color_definition(&mut out, index, r, g, b);
        }
    }

    let mut band_masks = vec![vec![0u8; width_usize]; color_count];
    let mut band_used = vec![false; color_count];
    let mut used_in_band: Vec<usize> = Vec::new();
    for band_y in (0..height_usize).step_by(6) {
        for index in used_in_band.drain(..) {
            band_masks[index].fill(0);
            band_used[index] = false;
        }

        let band_height = (height_usize - band_y).min(6);
        for dy in 0..band_height {
            let y = band_y + dy;
            for x in 0..width_usize {
                let pixel = rgba.get_pixel(x as u32, y as u32);
                if let Some(index) = sixel_palette_index(pixel.0, levels) {
                    if !band_used[index] {
                        band_used[index] = true;
                        used_in_band.push(index);
                    }
                    if let Some(mask) = band_masks[index].get_mut(x) {
                        *mask |= 1 << dy;
                    }
                }
            }
        }

        used_in_band.sort_unstable();
        if used_in_band.is_empty() {
            if band_y + 6 < height_usize {
                out.push(b'-');
            }
            continue;
        }

        for (position, index) in used_in_band.iter().copied().enumerate() {
            push_sixel_color_select(&mut out, index);
            let masks = &band_masks[index];
            let last = masks
                .iter()
                .rposition(|mask| *mask != 0)
                .unwrap_or_default();
            append_sixel_rle(&mut out, &masks[..=last]);
            if position + 1 == used_in_band.len() {
                if band_y + 6 < height_usize {
                    out.push(b'-');
                }
            } else {
                out.push(b'$');
            }
        }
    }

    out.extend_from_slice(terminal_string_terminator());
    out
}

fn sixel_color_count(levels: u8) -> usize {
    let levels = levels as usize;
    levels * levels * levels
}

fn sixel_palette_index(pixel: [u8; 4], levels: u8) -> Option<usize> {
    if pixel[3] <= SIXEL_ALPHA_THRESHOLD {
        return None;
    }
    let levels = levels as usize;
    let r = quantize_sixel_channel(pixel[0], levels);
    let g = quantize_sixel_channel(pixel[1], levels);
    let b = quantize_sixel_channel(pixel[2], levels);
    Some(r * levels * levels + g * levels + b)
}

fn quantize_sixel_channel(value: u8, levels: usize) -> usize {
    if levels <= 1 {
        return 0;
    }
    ((usize::from(value) * (levels - 1)) + 127) / 255
}

fn sixel_palette_rgb(index: usize, levels: u8) -> (u8, u8, u8) {
    let levels = levels as usize;
    let r = index / (levels * levels);
    let g = (index / levels) % levels;
    let b = index % levels;
    (
        sixel_palette_percent(r, levels),
        sixel_palette_percent(g, levels),
        sixel_palette_percent(b, levels),
    )
}

fn sixel_palette_percent(level: usize, levels: usize) -> u8 {
    if levels <= 1 {
        return 0;
    }
    (((level * 100) + ((levels - 1) / 2)) / (levels - 1)) as u8
}

fn push_sixel_color_definition(out: &mut Vec<u8>, index: usize, r: u8, g: u8, b: u8) {
    push_sixel_color_select(out, index);
    out.extend_from_slice(b";2;");
    push_decimal(out, usize::from(r));
    out.push(b';');
    push_decimal(out, usize::from(g));
    out.push(b';');
    push_decimal(out, usize::from(b));
}

fn push_sixel_color_select(out: &mut Vec<u8>, index: usize) {
    out.push(b'#');
    push_decimal(out, index);
}

fn append_sixel_rle(out: &mut Vec<u8>, masks: &[u8]) {
    let mut i = 0;
    while i < masks.len() {
        let ch = b'?' + masks[i];
        let mut run = 1;
        while i + run < masks.len() && masks[i + run] == masks[i] {
            run += 1;
        }
        if run >= 4 {
            out.push(b'!');
            push_decimal(out, run);
            out.push(ch);
        } else {
            for _ in 0..run {
                out.push(ch);
            }
        }
        i += run;
    }
}

fn push_decimal(out: &mut Vec<u8>, value: usize) {
    let mut buf = [0u8; 20];
    let mut n = value;
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[i..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_family_identities_use_kitty_protocol() {
        for value in [
            "kitty",
            "xterm-kitty",
            "ghostty",
            "xterm-ghostty",
            "WezTerm 20240203",
            "rio",
            "WarpTerminal",
            "konsole",
        ] {
            assert_eq!(
                protocol_from_identity(value),
                Some(TerminalImageProtocol::Kitty)
            );
        }
    }

    #[test]
    fn iterm_family_identities_use_iterm2_protocol() {
        for value in ["iTerm.app", "iTerm2", "mintty", "hterm"] {
            assert_eq!(
                protocol_from_identity(value),
                Some(TerminalImageProtocol::Iterm2)
            );
        }
    }

    #[test]
    fn sixel_family_identities_use_sixel_protocol() {
        for value in [
            "Windows Terminal 1.23.0",
            "foot",
            "foot-extra",
            "contour",
            "mlterm",
            "xterm-sixel",
        ] {
            assert_eq!(
                protocol_from_identity(value),
                Some(TerminalImageProtocol::Sixel)
            );
        }
    }

    #[test]
    fn terminal_env_hints_enable_image_protocols() {
        assert_eq!(
            protocol_from_env_hint("LC_TERMINAL", "iTerm2"),
            Some(TerminalImageProtocol::Iterm2)
        );
        assert_eq!(
            protocol_from_env_hint("WEZTERM_PANE", "3"),
            Some(TerminalImageProtocol::Kitty)
        );
        assert_eq!(
            protocol_from_env_hint("WT_SESSION", "abc"),
            Some(TerminalImageProtocol::Sixel)
        );
        assert_eq!(protocol_from_env_hint("WEZTERM_PANE", ""), None);
        assert_eq!(protocol_from_env_hint("WT_SESSION", ""), None);
    }

    #[test]
    fn terminal_features_enable_iterm2_file_protocol() {
        assert_eq!(
            protocol_from_terminal_features("T1CwMUBSxF"),
            Some(TerminalImageProtocol::Iterm2)
        );
        assert_eq!(protocol_from_terminal_features("T1CwMUBSx"), None);
    }

    #[test]
    fn tmux_term_disables_terminal_images() {
        assert!(term_disables_terminal_images("tmux-256color"));
        assert!(term_disables_terminal_images("screen-256color"));
        assert!(term_disables_terminal_images("screen.xterm-256color"));
        assert!(!term_disables_terminal_images("xterm-kitty"));
    }

    #[test]
    fn sixel_encoder_emits_dcs_raster_palette_and_pixels() {
        let rgba = RgbaImage::from_pixel(4, 1, image::Rgba([255, 0, 0, 255]));
        let encoded = encode_sixel_with_levels(&rgba, 4, 1, 6);
        let text = String::from_utf8_lossy(&encoded);

        assert!(encoded.starts_with(b"\x1bPq"));
        assert!(encoded.ends_with(terminal_string_terminator()));
        assert!(text.contains("\"1;1;4;1"));
        assert!(text.contains("#180;2;100;0;0"));
        assert!(text.contains("#180!4@"));
    }

    #[test]
    fn sixel_encoder_leaves_transparent_pixels_unpainted() {
        let rgba = RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 0]));
        let encoded = encode_sixel_with_levels(&rgba, 1, 1, 6);
        let text = String::from_utf8_lossy(&encoded);

        assert!(!text.contains("#180"));
        assert!(text.contains("\"1;1;1;1"));
    }

    #[test]
    fn sixel_command_does_not_reencode_when_placement_is_smaller_than_cache() {
        let rgba = RgbaImage::from_pixel(16, 16, image::Rgba([0, 255, 0, 255]));
        let sixel = encode_sixel_image(&rgba, 16, 16).expect("sixel encodes");
        let data = TerminalImageData::new(vec![], Some(sixel), 2, 1);
        let placement = TerminalImagePlacement {
            message_id: Uuid::nil(),
            area: Rect::new(0, 0, 1, 1),
            data,
        };

        assert_eq!(
            sixel_image_commands(&placement),
            vec![cursor_to(placement.area)]
        );
    }

    #[test]
    fn non_sixel_terminal_image_data_skips_sixel_encoding() {
        let mut png = Vec::new();
        {
            let rgba = RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
            let encoder = PngEncoder::new(Cursor::new(&mut png));
            encoder
                .write_image(rgba.as_raw(), 1, 1, ExtendedColorType::Rgba8)
                .unwrap();
        }

        let data = terminal_image_from_bytes(&png, 1, 1, TerminalImageProtocol::Kitty).unwrap();
        assert!(data.sixel_bytes.is_none());
        assert!(data.supports_protocol(TerminalImageProtocol::Kitty));
        assert!(!data.supports_protocol(TerminalImageProtocol::Sixel));
    }
}
