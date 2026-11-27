// Find the terminal's block cursor in a screenshot: NetHack parks it on the
// hero after every move, so it tells where the hero is.

#[derive(Debug, PartialEq, Eq)]
pub enum PpmError {
    NotBinaryPpm,
    BadHeader,
    Truncated,
}

/// A screenshot reduced to whiteness: the darkest channel of each pixel, so
/// a colored highlight (the tab bar, a mouse hover) doesn't look like the
/// white cursor.
pub struct Image {
    width: usize,
    height: usize,
    white: Vec<u8>,
}

/// Parse a binary PPM (`P6`, maxval 255), as `grim -t ppm` writes it.
pub fn parse_ppm(bytes: &[u8]) -> Result<Image, PpmError> {
    if !bytes.starts_with(b"P6") {
        return Err(PpmError::NotBinaryPpm);
    }
    let mut fields = vec![];
    let mut i = 2;
    while fields.len() < 3 {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'#') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let field = std::str::from_utf8(&bytes[start..i]).ok().and_then(|f| f.parse::<usize>().ok());
        match field {
            Some(v) => fields.push(v),
            None => return Err(PpmError::BadHeader),
        }
    }
    let (width, height) = (fields[0], fields[1]);
    if fields[2] != 255 {
        return Err(PpmError::BadHeader);
    }
    let pixels = bytes.get(i + 1..).ok_or(PpmError::Truncated)?;
    if pixels.len() < width * height * 3 {
        return Err(PpmError::Truncated);
    }
    let white = pixels.chunks_exact(3).take(width * height).map(|p| p[0].min(p[1]).min(p[2])).collect();
    Ok(Image { width, height, white })
}

/// Size of one terminal cell in screenshot pixels.
#[derive(Clone, Copy, Debug)]
pub struct CellSize {
    pub width: f64,
    pub height: f64,
}

/// A pixel this white (darkest channel, of 255) or more counts as lit. The
/// cursor block (186,194,222) is above it; the blue `hilite_pile` block
/// (137,180,250), the other solid cell on the map, is below.
const LIT: u8 = 160;

/// Pixel center of the terminal's block cursor, or `None` when there is
/// none. The cursor is the only solid cell-sized rectangle on screen: its
/// top and bottom rows are lit edge to edge (the hero glyph inside never
/// reaches them) and the columns just left and right of it are dark. Glyphs
/// and emoji fail the edge rows; a highlighted bar (the curses name field,
/// a menu heading) fails the dark sides.
pub fn cursor(img: &Image, cell: CellSize) -> Option<(f64, f64)> {
    // a box inside one cell whatever its phase: a 12.95px wide cell is 12 or
    // 13 pixels on screen, so the right side is checked one column out.
    let bw = (cell.width.floor() as usize).saturating_sub(1).max(1);
    let bh = (cell.height.floor() as usize).saturating_sub(2).max(1);
    if img.width < bw + 3 || img.height < bh {
        return None;
    }
    // summed-area table of lit pixels, one extra row and column of zeros.
    let w1 = img.width + 1;
    let mut sum = vec![0u32; w1 * (img.height + 1)];
    for y in 0..img.height {
        let mut row = 0u32;
        for x in 0..img.width {
            row += (img.white[y * img.width + x] >= LIT) as u32;
            sum[(y + 1) * w1 + x + 1] = sum[y * w1 + x + 1] + row;
        }
    }
    let lit = |x: usize, y: usize, w: usize, h: usize| -> usize {
        (sum[(y + h) * w1 + x + w] + sum[y * w1 + x] - sum[y * w1 + x + w] - sum[(y + h) * w1 + x]) as usize
    };
    let edge = bw * 9 / 10;
    let side = bh / 5;
    let mut best: Option<(usize, usize, usize)> = None;
    for y in 0..=img.height - bh {
        for x in 1..img.width - bw - 1 {
            if lit(x, y, bw, 1) < edge || lit(x, y + bh - 1, bw, 1) < edge {
                continue;
            }
            if lit(x - 1, y, 1, bh) > side || lit(x + bw + 1, y, 1, bh) > side {
                continue;
            }
            let fill = lit(x, y, bw, bh);
            if best.is_none_or(|(f, _, _)| fill > f) {
                best = Some((fill, x, y));
            }
        }
    }
    best.map(|(_, x, y)| (x as f64 + bw as f64 / 2.0, y as f64 + bh as f64 / 2.0))
}

/// Where map cell (0, 0) sits on screen, learned from one cursor sighting.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    cell: CellSize,
    origin: (f64, f64),
}

impl Grid {
    /// The cursor at pixel `at` is on map cell `hero`.
    pub fn calibrate(cell: CellSize, at: (f64, f64), hero: (usize, usize)) -> Grid {
        let origin = (at.0 - hero.0 as f64 * cell.width, at.1 - hero.1 as f64 * cell.height);
        Grid { cell, origin }
    }

    /// The map cell under pixel `at`; may lie off the map (a prompt line).
    pub fn cell_at(&self, at: (f64, f64)) -> (i64, i64) {
        (((at.0 - self.origin.0) / self.cell.width).round() as i64, ((at.1 - self.origin.1) / self.cell.height).round() as i64)
    }
}

#[cfg(test)]
#[path = "screen_test.rs"]
mod screen_test;
