use std::fs;

use anyhow::{Context, Result, anyhow};
use kdl::KdlNode;
use ratatui::style::Color;

use super::{config::LayerConfig, kdl_parse};

#[derive(Debug, Clone)]
pub struct ReefWorld {
    pub surface: WorldLayer,
    pub floor: WorldLayer,
    pub viewport_x: i32,
}

impl ReefWorld {
    pub fn new(surface: WorldLayer, floor: WorldLayer) -> Self {
        Self {
            surface,
            floor,
            viewport_x: 0,
        }
    }

    pub fn visible_bounds(&self, visible_width: u16) -> WorldBounds {
        WorldBounds {
            start: self.viewport_x,
            end: self.viewport_x.saturating_add(visible_width as i32),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldBounds {
    pub start: i32,
    pub end: i32,
}

#[derive(Debug, Clone)]
pub struct WorldLayer {
    chunks: Vec<LayerChunk>,
    pattern: Vec<usize>,
    pattern_width: i32,
    pub color: Color,
    pub height: u16,
}

impl WorldLayer {
    pub fn cell_at(&self, world_x: i32, row: u16) -> Option<char> {
        let mut offset = world_x.rem_euclid(self.pattern_width);
        for chunk_index in &self.pattern {
            let chunk = &self.chunks[*chunk_index];
            if offset < chunk.width as i32 {
                let line = chunk.lines.get(row as usize)?;
                let symbol = line.chars().nth(offset as usize)?;

                return if symbol == ' ' { None } else { Some(symbol) };
            }
            offset -= chunk.width as i32;
        }

        None
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

#[derive(Debug, Clone)]
struct LayerChunk {
    lines: Vec<String>,
    width: u16,
    height: u16,
}

pub fn load_world_layer(config: &LayerConfig) -> Result<WorldLayer> {
    let source = if let Some(source) = embedded_world_layer(config.file.to_string_lossy().as_ref())
    {
        source.to_string()
    } else {
        fs::read_to_string(&config.file)
            .with_context(|| format!("reading {}", config.file.display()))?
    };
    let doc = kdl_parse::parse_document(&config.file, &source)?;

    let chunks_node = doc
        .get("chunks")
        .ok_or_else(|| anyhow!("{} is missing `chunks`", config.file.display()))?;
    let chunks = chunks_node
        .children()
        .ok_or_else(|| anyhow!("{} `chunks` node has no children", config.file.display()))?
        .nodes()
        .iter()
        .map(parse_chunk_node)
        .collect::<Vec<_>>();

    if chunks.is_empty() {
        return Err(anyhow!("{} has no world chunks", config.file.display()));
    }

    let height = chunks
        .iter()
        .map(|chunk| chunk.height)
        .max()
        .unwrap_or_default()
        .max(1);
    Ok(WorldLayer::new(chunks, config.color, height))
}

fn embedded_world_layer(path: &str) -> Option<&'static str> {
    match path {
        "art/world/floor1.kdl" | "world/floor1.kdl" | "assets/aquarium/world/floor1.kdl" => {
            Some(include_str!("../../../../assets/aquarium/world/floor1.kdl"))
        }
        "art/world/surface1.kdl" | "world/surface1.kdl" | "assets/aquarium/world/surface1.kdl" => {
            Some(include_str!(
                "../../../../assets/aquarium/world/surface1.kdl"
            ))
        }
        _ => None,
    }
}

fn build_pattern(chunks: &[LayerChunk]) -> (Vec<usize>, i32) {
    const PATTERN_CHUNKS: i32 = 64;

    let pattern = (0..PATTERN_CHUNKS)
        .map(|block| stable_index(block, chunks.len()))
        .collect::<Vec<_>>();
    let pattern_width = pattern
        .iter()
        .map(|chunk_index| chunks[*chunk_index].width.max(1) as i32)
        .sum::<i32>()
        .max(1);

    (pattern, pattern_width)
}

impl WorldLayer {
    fn new(chunks: Vec<LayerChunk>, color: Color, height: u16) -> Self {
        let (pattern, pattern_width) = build_pattern(&chunks);
        Self {
            chunks,
            pattern,
            pattern_width,
            color,
            height,
        }
    }
}

fn parse_chunk_node(node: &KdlNode) -> LayerChunk {
    let source = node
        .get(0)
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| node.name().value());
    let lines = source
        .trim_matches('\n')
        .lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect::<Vec<_>>();
    let width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or_default()
        .min(u16::MAX as usize) as u16;
    let height = lines.len().min(u16::MAX as usize) as u16;

    LayerChunk {
        lines,
        width: width.max(1),
        height,
    }
}

fn stable_index(block: i32, len: usize) -> usize {
    let mut value = block as i64;
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51afd7ed558ccd_u64 as i64);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ceb9fe1a85ec53_u64 as i64);
    value ^= value >> 33;
    value.rem_euclid(len as i64) as usize
}
