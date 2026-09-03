use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::Path,
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use image::{GenericImageView, Rgba, RgbaImage};
use late_core::models::chips::Difficulty;
use ratatui::{layout::Rect, text::Line};
use reqwest::Url;
use tokio::sync::mpsc;

use crate::app::files::{
    inline_image::{InlineImagePreview, InlineImageRenderSettings, render_image_bytes},
    terminal_image::{
        MAX_DECODED_IMAGE_PIXELS, TerminalImageData, TerminalImageProtocol,
        terminal_image_from_rgba, terminal_image_pixel_dimensions,
    },
};

const MIN_IMAGE_TILE_HEIGHT: u16 = 3;
const MAX_IMAGE_TILE_HEIGHT: u16 = 8;
const NATIVE_LABEL_AMBER: Rgba<u8> = Rgba([184, 122, 43, 255]);
const NATIVE_LABEL_SHADOW: Rgba<u8> = Rgba([18, 12, 7, 255]);
const NATIVE_GAP_BORDER: Rgba<u8> = Rgba([104, 72, 30, 255]);
/// Bit width of a `DIGIT_GLYPHS` row.
const DIGIT_GLYPH_COLUMNS: u32 = 3;
/// Bit width of a `SOLVED_GLYPHS` row.
const SOLVED_GLYPH_COLUMNS: u32 = 5;
const DIGIT_GLYPHS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];
const SOLVED_GLYPHS: [[u8; 7]; 6] = [
    [
        0b11111, 0b10000, 0b10000, 0b11111, 0b00001, 0b00001, 0b11111,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
    ],
    [
        0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
    ],
    [
        0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
    ],
    [
        0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
    ],
];
const LOCAL_ARTWORK_DIRECTORY: &str = "artwork/sliding-puzzle";
const PLACEHOLDER_IMAGE_URLS: [&str; 3] = [
    "https://fastly.picsum.photos/id/1025/800/800.jpg?hmac=fvdRIVjOccpJuvVsTr3FHnSAeges_Igqa46__zj3Q7U",
    "https://fastly.picsum.photos/id/1039/800/800.jpg?hmac=_yLp1ssgvLOd-kNxo1vwmJtEzWlhT2aIDVZOFfHT8YE",
    "https://fastly.picsum.photos/id/1069/800/800.jpg?hmac=hvhA2h_VdqmbXPVnRHgToGg8yVCUig4945-OXQNbJd8",
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TileView {
    #[default]
    Numbered,
    Image,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageStatus {
    Numbered,
    Loading,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ImageTileGeometry {
    pub(crate) width: u16,
    pub(crate) height: u16,
}

pub(crate) const MIN_IMAGE_TILE_GEOMETRY: ImageTileGeometry = ImageTileGeometry {
    width: MIN_IMAGE_TILE_HEIGHT * 2,
    height: MIN_IMAGE_TILE_HEIGHT,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ImageRequestKey {
    source_index: usize,
    dimension: usize,
    geometry: ImageTileGeometry,
    settings: InlineImageRenderSettings,
}

type ImageResult = (
    ImageRequestKey,
    Result<(InlineImagePreview, Arc<Vec<u8>>), String>,
);

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeImageRequestKey {
    artwork: ImageRequestKey,
    protocol: TerminalImageProtocol,
}

#[derive(Clone, Debug)]
pub(crate) struct NativePuzzleImageSet {
    dimension: usize,
    geometry: ImageTileGeometry,
    protocol: TerminalImageProtocol,
    unsolved_tiles: Vec<TerminalImageData>,
    solved_cells: Vec<TerminalImageData>,
}

impl NativePuzzleImageSet {
    pub(crate) fn geometry(&self) -> ImageTileGeometry {
        self.geometry
    }

    pub(crate) fn supports_protocol(&self, protocol: TerminalImageProtocol) -> bool {
        self.protocol == protocol
    }

    pub(crate) fn cell_image(
        &self,
        board: &[u8],
        destination: usize,
    ) -> Option<&TerminalImageData> {
        if board.len() != self.dimension.saturating_mul(self.dimension) {
            return None;
        }
        if board_is_solved(board) {
            return self.solved_cells.get(destination);
        }
        self.unsolved_tiles
            .get(usize::from(*board.get(destination)?))
    }

    pub(crate) fn cache_key_for_board(&self, board: &[u8]) -> Option<u64> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for destination in 0..board.len() {
            self.cell_image(board, destination)?
                .cache_key()
                .hash(&mut hasher);
        }
        Some(hasher.finish())
    }

    pub(crate) fn cache_key(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.dimension.hash(&mut hasher);
        self.geometry.hash(&mut hasher);
        self.protocol.hash(&mut hasher);
        for image in self.unsolved_tiles.iter().chain(&self.solved_cells) {
            image.cache_key().hash(&mut hasher);
        }
        hasher.finish()
    }

    pub(crate) fn is_opaque(&self) -> bool {
        self.unsolved_tiles
            .iter()
            .chain(&self.solved_cells)
            .all(TerminalImageData::is_opaque)
    }
}

type NativeImageResult = (
    NativeImageRequestKey,
    Result<(NativePuzzleImageSet, Arc<Vec<u8>>), String>,
);

/// Session-local render caches for the image view. Both halves — the Chafa
/// preview and the native cell set — hold exactly one result: whatever the
/// currently selected source/geometry/protocol asks for. Superseded results
/// are dropped rather than accumulated, so a session that resizes or changes
/// difficulty repeatedly never grows.
pub(crate) struct ImageTiles {
    view: TileView,
    active_request: Option<ImageRequestKey>,
    preview: Option<(ImageRequestKey, InlineImagePreview)>,
    in_flight: Option<ImageRequestKey>,
    failure: Option<ImageRequestKey>,
    result_tx: mpsc::Sender<ImageResult>,
    result_rx: mpsc::Receiver<ImageResult>,
    native_active_request: Option<NativeImageRequestKey>,
    native_render: Option<(NativeImageRequestKey, NativePuzzleImageSet)>,
    native_in_flight: Option<NativeImageRequestKey>,
    native_failure: Option<NativeImageRequestKey>,
    source_bytes: HashMap<usize, Arc<Vec<u8>>>,
    native_result_tx: mpsc::Sender<NativeImageResult>,
    native_result_rx: mpsc::Receiver<NativeImageResult>,
}

impl ImageTiles {
    pub(crate) fn new() -> Self {
        let (result_tx, result_rx) = mpsc::channel(1);
        let (native_result_tx, native_result_rx) = mpsc::channel(1);
        Self {
            view: TileView::Numbered,
            active_request: None,
            preview: None,
            in_flight: None,
            failure: None,
            result_tx,
            result_rx,
            native_active_request: None,
            native_render: None,
            native_in_flight: None,
            native_failure: None,
            source_bytes: HashMap::new(),
            native_result_tx,
            native_result_rx,
        }
    }

    pub(crate) fn view(&self) -> TileView {
        self.view
    }

    /// Drops every cached raster and source download. Called when the board
    /// stops being the open screen: a session that played once would otherwise
    /// hold a couple of megabytes of decoded tiles until it disconnects, and
    /// `poll` — the only thing that can evict them — no longer runs.
    /// The selected view survives, so returning to the board reloads it.
    ///
    /// The channels are drained here too, and for the same reason. A request
    /// that was still in flight when the board closed lands in a buffer only
    /// `poll` reads, so its whole encoded set would outlive everything this
    /// method just dropped. Clearing the in-flight markers alongside the drain
    /// is what keeps that safe: a discarded result can no longer be the thing
    /// that clears them, and a marker left set would refuse every later
    /// request for the rest of the session.
    pub(crate) fn release(&mut self) -> bool {
        let held =
            self.preview.is_some() || self.native_render.is_some() || !self.source_bytes.is_empty();
        while self.result_rx.try_recv().is_ok() {}
        while self.native_result_rx.try_recv().is_ok() {}
        self.active_request = None;
        self.preview = None;
        self.in_flight = None;
        self.failure = None;
        self.native_active_request = None;
        self.native_render = None;
        self.native_in_flight = None;
        self.native_failure = None;
        self.source_bytes.clear();
        held
    }

    pub(crate) fn toggle(&mut self, seed: u64, difficulty: Difficulty) {
        self.view = match self.view {
            TileView::Numbered => {
                // Switching back on re-arms a failed request for this exact
                // artwork, which is what makes "press i twice" a retry. Both
                // halves must be re-armed here rather than left to `poll`:
                // two presses inside one 66ms tick never let `poll` observe
                // the Numbered view, so a native failure cleared only on a
                // key transition would stick for the rest of the session.
                let source_index = image_source_index(seed);
                let dimension = super::state::board_dimension(difficulty);
                let matches_artwork = |key: &ImageRequestKey| {
                    key.source_index == source_index && key.dimension == dimension
                };
                self.failure = self.failure.filter(|key| !matches_artwork(key));
                self.native_failure = self
                    .native_failure
                    .take()
                    .filter(|key| !matches_artwork(&key.artwork));
                TileView::Image
            }
            TileView::Image => TileView::Numbered,
        };
    }

    pub(crate) fn poll(
        &mut self,
        seed: u64,
        difficulty: Difficulty,
        geometry: ImageTileGeometry,
        settings: InlineImageRenderSettings,
        protocol: Option<TerminalImageProtocol>,
    ) -> bool {
        let key = image_request_key(seed, difficulty, geometry, settings);
        let mut changed = self.sync_request(key);

        while let Ok((result_key, result)) = self.result_rx.try_recv() {
            changed |= self.apply_result(result_key, result);
        }
        while let Ok((result_key, result)) = self.native_result_rx.try_recv() {
            changed |= self.apply_native_result(result_key, result);
        }

        if tokio::runtime::Handle::try_current().is_ok() && self.claim_preview_request(key) {
            let result_tx = self.result_tx.clone();
            let source = placeholder_image_url(seed).to_string();
            let cached_bytes = self.source_bytes.get(&key.source_index).cloned();
            tokio::spawn(async move {
                let result = render_preview_request(key, source, cached_bytes)
                    .await
                    .map_err(|error| error.to_string());
                let _ = result_tx.send((key, result)).await;
            });
        }

        // Wait for the preview attempt to settle before starting the native
        // encode, so the two never race for the same download — but settle
        // means *finished*, not *succeeded*. The Chafa path fits by aspect
        // ratio and rejects a source that is not square; the native path
        // resizes exactly and does not care, so a preview failure must not
        // take the full-resolution renderer down with it.
        let preview_settled = self.has_preview(key) || self.failure == Some(key);
        let native_key = (self.view == TileView::Image && preview_settled)
            .then_some(protocol)
            .flatten()
            .map(|protocol| NativeImageRequestKey {
                artwork: key,
                protocol,
            });
        changed |= self.sync_native_request(native_key.clone());
        if let Some(native_key) = native_key
            .as_ref()
            .filter(|key| self.should_request_native(key))
            .cloned()
        {
            self.start_native_request(native_key);
        }

        changed
    }

    pub(crate) fn status_for(&self, seed: u64, difficulty: Difficulty) -> ImageStatus {
        if self.view == TileView::Numbered {
            return ImageStatus::Numbered;
        }
        let Some(key) = self.active_request.filter(|key| {
            key.source_index == image_source_index(seed)
                && key.dimension == super::state::board_dimension(difficulty)
        }) else {
            return ImageStatus::Loading;
        };
        // Either renderer having something to show counts as ready, and only
        // both having given up counts as failed. The native path survives a
        // preview failure, so reporting the preview's verdict alone would put
        // "Art unavailable" under a board that is drawing artwork.
        let native_ready = self
            .native_render
            .as_ref()
            .is_some_and(|(rendered, _)| rendered.artwork == key);
        let native_settled = native_ready
            || self
                .native_failure
                .as_ref()
                .is_some_and(|failed| failed.artwork == key);
        if self.has_preview(key) || native_ready {
            ImageStatus::Ready
        } else if self.failure == Some(key) && (native_settled || self.native_in_flight.is_none()) {
            ImageStatus::Failed
        } else {
            ImageStatus::Loading
        }
    }

    fn has_preview(&self, key: ImageRequestKey) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|(cached, _)| *cached == key)
    }

    pub(crate) fn preview_for(
        &self,
        seed: u64,
        difficulty: Difficulty,
    ) -> Option<&InlineImagePreview> {
        let key = self.active_request.filter(|key| {
            key.source_index == image_source_index(seed)
                && key.dimension == super::state::board_dimension(difficulty)
        })?;
        self.preview
            .as_ref()
            .filter(|(cached, _)| *cached == key)
            .map(|(_, preview)| preview)
    }

    pub(crate) fn native_tiles_for(
        &self,
        seed: u64,
        difficulty: Difficulty,
    ) -> Option<&NativePuzzleImageSet> {
        if self.view != TileView::Image {
            return None;
        }
        let active = self.native_active_request.as_ref()?;
        if active.artwork.source_index != image_source_index(seed)
            || active.artwork.dimension != super::state::board_dimension(difficulty)
        {
            return None;
        }
        self.native_render
            .as_ref()
            .filter(|(rendered, _)| rendered == active)
            .map(|(_, images)| images)
    }

    fn sync_request(&mut self, key: ImageRequestKey) -> bool {
        let changed = self.active_request != Some(key);
        self.active_request = Some(key);
        if changed {
            // Only the active request is ever cached, so a new key always
            // supersedes whatever was held.
            self.preview = None;
            self.failure = None;
        }
        changed
    }

    fn should_request(&self, key: ImageRequestKey) -> bool {
        self.view == TileView::Image
            && self.active_request == Some(key)
            && !self.has_preview(key)
            && self.in_flight.is_none()
            && self.failure != Some(key)
    }

    fn claim_preview_request(&mut self, key: ImageRequestKey) -> bool {
        if !self.should_request(key) {
            return false;
        }
        self.in_flight = Some(key);
        true
    }

    fn apply_result(
        &mut self,
        key: ImageRequestKey,
        result: Result<(InlineImagePreview, Arc<Vec<u8>>), String>,
    ) -> bool {
        let result = result.map(|(preview, bytes)| {
            // Kept even when the preview itself is stale: the request that
            // superseded it wants the same artwork at a different size, and
            // should not go back to the network for it.
            self.source_bytes.insert(key.source_index, bytes);
            preview
        });
        self.apply_preview(key, result)
    }

    fn apply_preview(
        &mut self,
        key: ImageRequestKey,
        result: Result<InlineImagePreview, String>,
    ) -> bool {
        if self.in_flight == Some(key) {
            self.in_flight = None;
        }
        if self.active_request != Some(key) {
            return false;
        }
        match result {
            Ok(preview) => {
                self.failure = None;
                self.preview = Some((key, preview));
            }
            Err(_) => {
                self.preview = None;
                self.failure = Some(key);
            }
        }
        true
    }

    fn sync_native_request(&mut self, key: Option<NativeImageRequestKey>) -> bool {
        if self.native_active_request == key {
            return false;
        }
        self.native_active_request = key;
        // The rendered set deliberately outlives the request that produced
        // it. Encoding one costs a blocking job per board cell, and toggling
        // the view off sets this key to `None` every time — dropping it here
        // would make `i` re-encode the whole board on every press. The key
        // comparisons in `native_tiles_for` and `should_request_native` are
        // what decide whether it is still usable.
        self.native_failure = None;
        true
    }

    fn should_request_native(&self, key: &NativeImageRequestKey) -> bool {
        self.native_active_request.as_ref() == Some(key)
            && self
                .native_render
                .as_ref()
                .is_none_or(|(rendered, _)| rendered != key)
            && self.native_in_flight.is_none()
            && self.native_failure.as_ref() != Some(key)
    }

    fn start_native_request(&mut self, key: NativeImageRequestKey) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.native_in_flight = Some(key.clone());
        let result_tx = self.native_result_tx.clone();
        let cached_bytes = self.source_bytes.get(&key.artwork.source_index).cloned();
        tokio::spawn(async move {
            let result = render_native_request(key.clone(), cached_bytes)
                .await
                .map_err(|error| error.to_string());
            let _ = result_tx.send((key, result)).await;
        });
    }

    fn apply_native_result(
        &mut self,
        key: NativeImageRequestKey,
        result: Result<(NativePuzzleImageSet, Arc<Vec<u8>>), String>,
    ) -> bool {
        if self.native_in_flight.as_ref() == Some(&key) {
            self.native_in_flight = None;
        }
        let result = result.map(|(images, bytes)| {
            self.source_bytes.insert(key.artwork.source_index, bytes);
            images
        });
        if self.native_active_request.as_ref() != Some(&key) {
            return false;
        }
        match result {
            Ok(images) => {
                self.native_failure = None;
                self.native_render = Some((key, images));
            }
            Err(_) => {
                self.native_render = None;
                self.native_failure = Some(key);
            }
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn apply_active_result_for_test(
        &mut self,
        result: Result<InlineImagePreview, String>,
    ) {
        let key = self.active_request.expect("active image request");
        self.apply_preview(key, result);
    }

    #[cfg(test)]
    fn set_native_tiles_for_test(
        &mut self,
        artwork: ImageRequestKey,
        images: NativePuzzleImageSet,
    ) {
        let key = NativeImageRequestKey {
            artwork,
            protocol: images.protocol,
        };
        self.view = TileView::Image;
        self.native_active_request = Some(key.clone());
        self.native_render = Some((key, images));
    }
}

/// Renders the Chafa cell preview for `key`, reusing already-downloaded
/// source bytes when the session has them. The bytes come back with the
/// preview so the caller can seed its cache: the same artwork is re-rendered
/// at a new size on every resize and difficulty change, and only the size
/// changes.
async fn render_preview_request(
    key: ImageRequestKey,
    source: String,
    cached_bytes: Option<Arc<Vec<u8>>>,
) -> Result<(InlineImagePreview, Arc<Vec<u8>>)> {
    render_preview_from_directory(
        key,
        source,
        Path::new(LOCAL_ARTWORK_DIRECTORY),
        cached_bytes,
    )
    .await
}

async fn render_preview_from_directory(
    key: ImageRequestKey,
    source: String,
    artwork_directory: &Path,
    cached_bytes: Option<Arc<Vec<u8>>>,
) -> Result<(InlineImagePreview, Arc<Vec<u8>>)> {
    let bytes = match cached_bytes {
        Some(bytes) => bytes,
        None => Arc::new(load_artwork_bytes_from_directory(source, artwork_directory).await?),
    };
    let max_width = key.dimension as u32 * u32::from(key.geometry.width);
    let max_height = key.dimension as u32 * u32::from(key.geometry.height);
    let preview =
        render_image_bytes(bytes.as_ref().clone(), max_width, max_height, key.settings).await?;
    if !valid_preview(&preview, key.dimension, key.geometry) {
        bail!("image preview has unexpected dimensions");
    }
    Ok((preview, bytes))
}

async fn render_native_request(
    key: NativeImageRequestKey,
    cached_bytes: Option<Arc<Vec<u8>>>,
) -> Result<(NativePuzzleImageSet, Arc<Vec<u8>>)> {
    let bytes = match cached_bytes {
        Some(bytes) => bytes,
        None => Arc::new(
            load_artwork_bytes_from_directory(
                PLACEHOLDER_IMAGE_URLS[key.artwork.source_index].to_string(),
                Path::new(LOCAL_ARTWORK_DIRECTORY),
            )
            .await?,
        ),
    };
    let render_bytes = Arc::clone(&bytes);
    let images = tokio::task::spawn_blocking(move || {
        render_terminal_puzzle_tiles(
            render_bytes.as_slice(),
            key.artwork.dimension,
            key.artwork.geometry,
            key.protocol,
            key.artwork.settings.background_rgb,
        )
    })
    .await
    .context("native puzzle image renderer stopped")??;
    Ok((images, bytes))
}

async fn load_artwork_bytes_from_directory(
    source: String,
    artwork_directory: &Path,
) -> Result<Vec<u8>> {
    if let Some(bytes) =
        read_local_image_bytes(&source, artwork_directory, crate::config::MAX_IMAGE_BYTES).await?
    {
        return Ok(bytes);
    }
    crate::app::files::image_upload::download_url_bytes(
        &source,
        std::time::Duration::from_secs(15),
        crate::config::MAX_IMAGE_BYTES,
    )
    .await
}

async fn read_local_image_bytes(
    raw_url: &str,
    artwork_directory: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>> {
    let url = Url::parse(raw_url).context("invalid image source URL")?;
    if url.scheme() != "file" {
        return Ok(None);
    }
    if url.host_str().is_some() {
        bail!("local image file URL must not include a host");
    }

    let path = url
        .to_file_path()
        .map_err(|()| anyhow::anyhow!("invalid local image file URL"))?;
    let artwork_directory = tokio::fs::canonicalize(artwork_directory)
        .await
        .context("local artwork directory is unavailable")?;
    let path = tokio::fs::canonicalize(path)
        .await
        .context("local artwork file is unavailable")?;
    if !path.starts_with(&artwork_directory) {
        bail!("local image file is outside the artwork directory");
    }

    let metadata = tokio::fs::metadata(&path)
        .await
        .context("failed to inspect local artwork file")?;
    if !metadata.is_file() {
        bail!("local image source is not a regular file");
    }
    if metadata.len() > max_bytes as u64 {
        bail!("image is too large (max {max_bytes} bytes)");
    }

    let bytes = tokio::fs::read(path)
        .await
        .context("failed to read local artwork file")?;
    if bytes.len() > max_bytes {
        bail!("image is too large (max {max_bytes} bytes)");
    }
    Ok(Some(bytes))
}

fn placeholder_image_url(seed: u64) -> &'static str {
    PLACEHOLDER_IMAGE_URLS[image_source_index(seed)]
}

fn image_request_key(
    seed: u64,
    difficulty: Difficulty,
    geometry: ImageTileGeometry,
    settings: InlineImageRenderSettings,
) -> ImageRequestKey {
    ImageRequestKey {
        source_index: image_source_index(seed),
        dimension: super::state::board_dimension(difficulty),
        geometry,
        settings,
    }
}

pub(crate) fn image_tile_geometry(
    board_area: Rect,
    difficulty: Difficulty,
) -> Option<ImageTileGeometry> {
    let dimension = super::state::board_dimension(difficulty) as u16;
    let height_from_rows = board_area.height / dimension;
    let height_from_columns = board_area.width / dimension.saturating_mul(2);
    let height = height_from_rows
        .min(height_from_columns)
        .min(MAX_IMAGE_TILE_HEIGHT);

    (height >= MIN_IMAGE_TILE_GEOMETRY.height).then_some(ImageTileGeometry {
        width: height * 2,
        height,
    })
}

/// Pre-encodes every cell image a board can ever need: one per tile value
/// (index 0 is the gap) for an unsolved board, plus one per destination for
/// the solved board, which carries the completion banner instead of labels.
/// A move then only reorders already-encoded cells — see
/// [`NativePuzzleImageSet::cell_image`] — so input never waits on the encoder.
fn render_terminal_puzzle_tiles(
    bytes: &[u8],
    dimension: usize,
    geometry: ImageTileGeometry,
    protocol: TerminalImageProtocol,
    background_rgb: Option<u32>,
) -> Result<NativePuzzleImageSet> {
    if dimension == 0 {
        bail!("puzzle board dimension must not be zero");
    }
    let source = image::load_from_memory(bytes).context("failed to decode native puzzle image")?;
    let (source_width, source_height) = source.dimensions();
    if source_width == 0 || source_height == 0 {
        bail!("native puzzle image has invalid dimensions");
    }
    if u64::from(source_width) * u64::from(source_height) > MAX_DECODED_IMAGE_PIXELS {
        bail!("native puzzle image dimensions are too large");
    }

    let dimension_u16 = u16::try_from(dimension).unwrap_or(u16::MAX);
    let display_cols = dimension_u16.saturating_mul(geometry.width);
    let display_rows = dimension_u16.saturating_mul(geometry.height);
    let (pixel_width, pixel_height) = terminal_image_pixel_dimensions(display_cols, display_rows);
    let tile_pixel_width = pixel_width / dimension as u32;
    let tile_pixel_height = pixel_height / dimension as u32;
    // `image_tile_geometry` never yields a zero side, so this only fires if a
    // future caller hands one over. Reject it here: every pixel routine below
    // indexes with `width - 1` / `height - 1` and would underflow instead.
    if tile_pixel_width == 0 || tile_pixel_height == 0 {
        bail!("puzzle tile geometry is too small to render");
    }
    let background = background_rgb
        .map(|rgb| Rgba([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]))
        .unwrap_or(Rgba([0, 0, 0, 255]));
    let resized = source
        .resize_exact(
            pixel_width,
            pixel_height,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgba8();
    let mut solved = RgbaImage::from_pixel(pixel_width, pixel_height, background);
    image::imageops::overlay(&mut solved, &resized, 0, 0);
    let cell_count = dimension.saturating_mul(dimension);
    let mut unsolved_tiles = Vec::with_capacity(cell_count);

    for tile in 0..cell_count {
        let mut fragment = if tile == 0 {
            let mut gap = RgbaImage::from_pixel(tile_pixel_width, tile_pixel_height, background);
            draw_native_gap(&mut gap, 0, 0, tile_pixel_width, tile_pixel_height);
            gap
        } else {
            let source_index = tile - 1;
            image::imageops::crop_imm(
                &solved,
                (source_index % dimension) as u32 * tile_pixel_width,
                (source_index / dimension) as u32 * tile_pixel_height,
                tile_pixel_width,
                tile_pixel_height,
            )
            .to_image()
        };
        if tile > 0 {
            draw_native_tile_label(
                &mut fragment,
                tile as u8,
                0,
                0,
                tile_pixel_width,
                tile_pixel_height,
            );
        }
        unsolved_tiles.push(terminal_image_from_rgba(
            &fragment,
            geometry.width,
            geometry.height,
            protocol,
        )?);
    }

    let mut solved_board = solved;
    let last_x = (dimension - 1) as u32 * tile_pixel_width;
    let last_y = (dimension - 1) as u32 * tile_pixel_height;
    for y in last_y..last_y + tile_pixel_height {
        for x in last_x..last_x + tile_pixel_width {
            solved_board.put_pixel(x, y, background);
        }
    }
    draw_native_gap(
        &mut solved_board,
        last_x,
        last_y,
        tile_pixel_width,
        tile_pixel_height,
    );
    draw_native_solved_banner(&mut solved_board);
    let mut solved_cells = Vec::with_capacity(cell_count);
    for destination in 0..cell_count {
        let fragment = image::imageops::crop_imm(
            &solved_board,
            (destination % dimension) as u32 * tile_pixel_width,
            (destination / dimension) as u32 * tile_pixel_height,
            tile_pixel_width,
            tile_pixel_height,
        )
        .to_image();
        solved_cells.push(terminal_image_from_rgba(
            &fragment,
            geometry.width,
            geometry.height,
            protocol,
        )?);
    }

    Ok(NativePuzzleImageSet {
        dimension,
        geometry,
        protocol,
        unsolved_tiles,
        solved_cells,
    })
}

fn board_is_solved(board: &[u8]) -> bool {
    board.iter().copied().enumerate().all(|(index, tile)| {
        tile == if index + 1 == board.len() {
            0
        } else {
            (index + 1) as u8
        }
    })
}

fn draw_native_gap(image: &mut RgbaImage, x: u32, y: u32, width: u32, height: u32) {
    let border = (width.min(height) / 32).max(1);
    for offset in 0..border {
        for column in offset..width.saturating_sub(offset) {
            image.put_pixel(x + column, y + offset, NATIVE_GAP_BORDER);
            image.put_pixel(x + column, y + height - 1 - offset, NATIVE_GAP_BORDER);
        }
        for row in offset..height.saturating_sub(offset) {
            image.put_pixel(x + offset, y + row, NATIVE_GAP_BORDER);
            image.put_pixel(x + width - 1 - offset, y + row, NATIVE_GAP_BORDER);
        }
    }
}

fn draw_native_tile_label(
    image: &mut RgbaImage,
    tile: u8,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    let digits = tile.to_string();
    let scale = (height / 24).clamp(2, 4);
    let glyph_units = digits.len() as u32 * 3 + digits.len().saturating_sub(1) as u32;
    let label_width = glyph_units * scale;
    let label_height = 5 * scale;
    let start_x = x + width.saturating_sub(label_width) / 2;
    let start_y = y + height.saturating_sub(label_height) / 2;

    for (digit_index, digit) in digits.bytes().enumerate() {
        let Some(glyph) = DIGIT_GLYPHS.get(usize::from(digit.saturating_sub(b'0'))) else {
            continue;
        };
        let glyph_x = start_x + digit_index as u32 * 4 * scale;
        draw_glyph(image, glyph, DIGIT_GLYPH_COLUMNS, glyph_x, start_y, scale);
    }
}

/// Blits one row-major bitmap glyph — `columns` wide, MSB first, `scale`
/// pixels per bit — with a one-pixel drop shadow so it stays legible over
/// arbitrary artwork.
fn draw_glyph(image: &mut RgbaImage, glyph: &[u8], columns: u32, x: u32, y: u32, scale: u32) {
    for (row, bits) in glyph.iter().copied().enumerate() {
        for column in 0..columns {
            if bits & (1 << (columns - 1 - column)) == 0 {
                continue;
            }
            let block_x = x + column * scale;
            let block_y = y + row as u32 * scale;
            draw_native_label_block(image, block_x + 1, block_y + 1, scale, NATIVE_LABEL_SHADOW);
            draw_native_label_block(image, block_x, block_y, scale, NATIVE_LABEL_AMBER);
        }
    }
}

fn draw_native_solved_banner(image: &mut RgbaImage) {
    let scale = (image.height() / 96).clamp(1, 4);
    let glyph_width = SOLVED_GLYPH_COLUMNS * scale;
    let gap = scale;
    let text_width = SOLVED_GLYPHS.len() as u32 * glyph_width
        + SOLVED_GLYPHS.len().saturating_sub(1) as u32 * gap;
    let text_height = 7 * scale;
    let padding_x = 3 * scale;
    let padding_y = 2 * scale;
    let panel_width = (text_width + 2 * padding_x).min(image.width());
    let panel_height = (text_height + 2 * padding_y).min(image.height());
    let panel_x = image.width().saturating_sub(panel_width) / 2;
    let panel_y = (image.height() / 24).min(image.height().saturating_sub(panel_height));

    for y in panel_y..panel_y + panel_height {
        for x in panel_x..panel_x + panel_width {
            let pixel = image.get_pixel_mut(x, y);
            pixel.0[0] /= 4;
            pixel.0[1] /= 4;
            pixel.0[2] /= 4;
        }
    }

    let start_x = panel_x + panel_width.saturating_sub(text_width) / 2;
    let start_y = panel_y + panel_height.saturating_sub(text_height) / 2;
    for (glyph_index, glyph) in SOLVED_GLYPHS.iter().enumerate() {
        let glyph_x = start_x + glyph_index as u32 * (glyph_width + gap);
        draw_glyph(image, glyph, SOLVED_GLYPH_COLUMNS, glyph_x, start_y, scale);
    }
}

fn draw_native_label_block(image: &mut RgbaImage, x: u32, y: u32, scale: u32, color: Rgba<u8>) {
    for row in y..(y + scale).min(image.height()) {
        for column in x..(x + scale).min(image.width()) {
            image.put_pixel(column, row, color);
        }
    }
}

pub(crate) fn tile_fragment(
    preview: &[Line<'static>],
    dimension: usize,
    tile: u8,
    geometry: ImageTileGeometry,
) -> Option<Vec<Line<'static>>> {
    if tile == 0 || usize::from(tile) >= dimension.saturating_mul(dimension) {
        return None;
    }
    if !valid_preview(preview, dimension, geometry) {
        return None;
    }

    let source = usize::from(tile - 1);
    let source_row = source / dimension;
    let source_column = source % dimension;
    let row_start = source_row * usize::from(geometry.height);
    let column_start = source_column * usize::from(geometry.width);
    let column_end = column_start + usize::from(geometry.width);

    Some(
        preview[row_start..row_start + usize::from(geometry.height)]
            .iter()
            .map(|line| Line::from(line.spans[column_start..column_end].to_vec()))
            .collect(),
    )
}

fn image_source_index(seed: u64) -> usize {
    (seed % PLACEHOLDER_IMAGE_URLS.len() as u64) as usize
}

fn valid_preview(preview: &[Line<'static>], dimension: usize, geometry: ImageTileGeometry) -> bool {
    let expected_width = dimension * usize::from(geometry.width);
    let expected_height = dimension * usize::from(geometry.height);
    preview.len() == expected_height
        && preview
            .iter()
            .all(|line| line.spans.len() == expected_width)
}

#[cfg(test)]
#[path = "image_test.rs"]
mod image_test;
