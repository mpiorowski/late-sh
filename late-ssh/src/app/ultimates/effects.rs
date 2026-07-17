use ratatui::{
    buffer::{Buffer, CellDiffOption},
    style::Color,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UltimateEffectKind {
    Wonderland,
    Thematrix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UltimateThemeEffect {
    pub kind: UltimateEffectKind,
    pub seed: u64,
    pub elapsed_ms: u64,
}

pub const THEMATRIX_MAIN_PHASE_MS: u64 = 10_000;
pub const THEMATRIX_FADEOUT_MS: u64 = 3_000;
pub const THEMATRIX_TOTAL_MS: u64 = THEMATRIX_MAIN_PHASE_MS + THEMATRIX_FADEOUT_MS;

pub fn apply_ultimate_postprocess(buffer: &mut Buffer, effect: UltimateThemeEffect) {
    match effect.kind {
        UltimateEffectKind::Wonderland => apply_wonderland_postprocess(buffer, effect),
        UltimateEffectKind::Thematrix => apply_thematrix_postprocess(buffer, effect),
    }
}

fn apply_thematrix_postprocess(buffer: &mut Buffer, effect: UltimateThemeEffect) {
    let area = *buffer.area();
    if area.width == 0 || area.height == 0 || effect.elapsed_ms >= THEMATRIX_TOTAL_MS {
        return;
    }

    let elapsed_secs = effect.elapsed_ms as f32 / 1000.0;
    let fade = thematrix_fade_factor(effect.elapsed_ms);
    let mut lines = Vec::new();
    for col in 0..area.width {
        if let Some(line) = thematrix_line_for_col(area.height, effect.seed, col, elapsed_secs) {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        return;
    }

    if effect.elapsed_ms <= THEMATRIX_MAIN_PHASE_MS {
        for row in 0..area.height {
            for col in 0..area.width {
                let Some(cell) = buffer.cell_mut((area.x + col, area.y + row)) else {
                    continue;
                };
                if cell.diff_option == CellDiffOption::Skip {
                    continue;
                }
                cell.set_bg(Color::Rgb(0, 4, 0));
            }
        }
    }

    lines.sort_by_key(|line| line.z);

    for line in lines {
        for row in 0..area.height {
            let distance_from_head = line.head_y - row as f32;
            if !(0.0..line.height).contains(&distance_from_head) {
                continue;
            }

            let trail = 1.0 - distance_from_head / line.height;
            let flicker =
                0.82 + thematrix_rand(effect.seed, line.col, line.cycle, 64 + row as i32) * 0.18;
            let brightness = (trail * line.base_brightness * flicker * fade).clamp(0.0, 1.0);
            let color = thematrix_green(brightness, line.z_factor);
            for dx in 0..line.width {
                let col = line.col + dx;
                if col >= area.width {
                    continue;
                }
                let Some(cell) = buffer.cell_mut((area.x + col, area.y + row)) else {
                    continue;
                };
                if cell.diff_option != CellDiffOption::Skip {
                    cell.set_bg(color);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ThematrixLine {
    col: u16,
    cycle: i32,
    z: u8,
    z_factor: f32,
    height: f32,
    head_y: f32,
    base_brightness: f32,
    width: u16,
}

const THEMATRIX_Z_MAX: u8 = 7;
const THEMATRIX_SPAWN_CYCLE_SPEED: f32 = 16.0;

fn thematrix_line_for_col(
    area_height: u16,
    seed: u64,
    col: u16,
    elapsed_secs: f32,
) -> Option<ThematrixLine> {
    if thematrix_rand(seed, col, 0, 0) < 0.18 {
        return None;
    }

    let main_phase_secs = THEMATRIX_MAIN_PHASE_MS as f32 / 1000.0;
    let spawn_eval_secs = elapsed_secs.min(main_phase_secs);
    let fadeout_secs = (elapsed_secs - main_phase_secs).max(0.0);
    let lane_gap = 0.6 + thematrix_rand(seed, col, 0, 2) * 2.6;
    let spawn_line_height = area_height.clamp(8, 24) as f32;
    let spawn_cycle_secs =
        (area_height as f32 + spawn_line_height) / THEMATRIX_SPAWN_CYCLE_SPEED + lane_gap;
    let phase_offset = thematrix_rand(seed, col, 0, 3) * spawn_cycle_secs;
    let cycle = ((spawn_eval_secs + phase_offset) / spawn_cycle_secs).floor() as i32;
    let progress_at_spawn_eval = (spawn_eval_secs + phase_offset) - cycle as f32 * spawn_cycle_secs;

    let z = thematrix_z(seed, col, cycle, THEMATRIX_Z_MAX);
    let z_factor = thematrix_z_factor(z, THEMATRIX_Z_MAX);
    let lane_speed = thematrix_speed(seed, col, z_factor);
    let max_line_height = lerp(
        area_height.clamp(4, 12) as f32,
        area_height.clamp(8, 24) as f32,
        z_factor,
    );
    let line_height = thematrix_line_height(seed, col, cycle, z_factor, max_line_height);
    let active_secs = (area_height as f32 + line_height) / lane_speed;
    if progress_at_spawn_eval > active_secs {
        return None;
    }

    let progress = progress_at_spawn_eval + fadeout_secs;
    Some(ThematrixLine {
        col,
        cycle,
        z,
        z_factor,
        height: line_height,
        head_y: progress * lane_speed - line_height,
        base_brightness: thematrix_base_brightness(seed, col, cycle, z_factor),
        width: thematrix_line_width(z, THEMATRIX_Z_MAX),
    })
}

fn thematrix_fade_factor(elapsed_ms: u64) -> f32 {
    if elapsed_ms <= THEMATRIX_MAIN_PHASE_MS {
        return 1.0;
    }
    let fade_elapsed = elapsed_ms.saturating_sub(THEMATRIX_MAIN_PHASE_MS);
    let remaining = THEMATRIX_FADEOUT_MS.saturating_sub(fade_elapsed);
    remaining as f32 / THEMATRIX_FADEOUT_MS as f32
}

fn thematrix_z(seed: u64, col: u16, cycle: i32, z_max: u8) -> u8 {
    (thematrix_rand(seed, col, cycle, 6) * f32::from(z_max + 1))
        .floor()
        .min(f32::from(z_max)) as u8
}

fn thematrix_z_factor(z: u8, z_max: u8) -> f32 {
    if z_max == 0 {
        1.0
    } else {
        f32::from(z) / f32::from(z_max)
    }
}

fn thematrix_speed(seed: u64, col: u16, z_factor: f32) -> f32 {
    let min_speed = lerp(4.0, 12.0, z_factor);
    let max_speed = lerp(10.0, 28.0, z_factor);
    min_speed + thematrix_rand(seed, col, 0, 1) * (max_speed - min_speed)
}

fn thematrix_line_height(
    seed: u64,
    col: u16,
    cycle: i32,
    z_factor: f32,
    max_line_height: f32,
) -> f32 {
    let min_height = lerp(3.0, 6.0, z_factor);
    let max_height = max_line_height.max(min_height + 1.0);
    min_height + thematrix_rand(seed, col, cycle, 4) * (max_height - min_height)
}

fn thematrix_base_brightness(seed: u64, col: u16, cycle: i32, z_factor: f32) -> f32 {
    let min_brightness = lerp(0.08, 0.76, z_factor);
    let max_brightness = lerp(0.24, 1.0, z_factor);
    min_brightness + thematrix_rand(seed, col, cycle, 5) * (max_brightness - min_brightness)
}

fn thematrix_line_width(z: u8, z_max: u8) -> u16 {
    if z == z_max { 2 } else { 1 }
}

fn thematrix_rand(seed: u64, col: u16, cycle: i32, salt: i32) -> f32 {
    lattice(
        seed.wrapping_add((salt as i64 as u64).wrapping_mul(0xA24B_AED4_963E_E407)),
        col as i32,
        cycle,
    )
}

fn thematrix_green(brightness: f32, z_factor: f32) -> Color {
    let z_factor = z_factor.clamp(0.0, 1.0);
    let brightness = (brightness * lerp(0.36, 1.0, z_factor)).clamp(0.0, 1.0);
    Color::Rgb(
        (lerp(1.0, 24.0, z_factor) * brightness).round() as u8,
        (lerp(18.0, 48.0, z_factor) + lerp(112.0, 207.0, z_factor) * brightness).round() as u8,
        (lerp(2.0, 18.0, z_factor) + lerp(18.0, 76.0, z_factor) * brightness).round() as u8,
    )
}

fn apply_wonderland_postprocess(buffer: &mut Buffer, effect: UltimateThemeEffect) {
    let area = *buffer.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut heights = Vec::with_capacity(area.width as usize * area.height as usize);
    let mut min_height = f32::INFINITY;
    let mut max_height = f32::NEG_INFINITY;

    for row in 0..area.height {
        for col in 0..area.width {
            let (x, y) = wonderland_map_coords(col, row, area.width, area.height);
            let height = wonderland_height(effect.seed, effect.elapsed_ms, x, y);
            min_height = min_height.min(height);
            max_height = max_height.max(height);
            heights.push(height);
        }
    }

    let height_range = max_height - min_height;
    if height_range <= f32::EPSILON {
        return;
    }

    for row in 0..area.height {
        for col in 0..area.width {
            let idx = row as usize * area.width as usize + col as usize;
            let Some(cell) = buffer.cell_mut((area.x + col, area.y + row)) else {
                continue;
            };
            if cell.diff_option == CellDiffOption::Skip || cell.symbol() == " " {
                continue;
            }
            let normalized = (heights[idx] - min_height) / height_range;
            cell.set_fg(rainbow_gradient(normalized));
        }
    }
}

fn wonderland_map_coords(col: u16, row: u16, width: u16, height: u16) -> (f32, f32) {
    let nx = if width > 1 {
        col as f32 / (width - 1) as f32
    } else {
        0.5
    };
    let ny = if height > 1 {
        row as f32 / (height - 1) as f32
    } else {
        0.5
    };
    let aspect = width as f32 / height.max(1) as f32;
    ((nx - 0.5) * 2.0 * aspect, (ny - 0.5) * 2.0)
}

fn wonderland_height(seed: u64, elapsed_ms: u64, x: f32, y: f32) -> f32 {
    let t = elapsed_ms as f32 / 1000.0;
    let rotation = (t * 0.7).sin() * 1.4;
    let zoom = 1.4 + (t * 0.9).sin() * 0.55;
    let pan_x = (t * 0.31 + seed as f32 * 0.000_001).sin() * 1.8;
    let pan_y = (t * 0.27 + seed as f32 * 0.000_002).cos() * 1.8;
    let (sin_r, cos_r) = rotation.sin_cos();
    let rx = (x * cos_r - y * sin_r) * zoom + pan_x;
    let ry = (x * sin_r + y * cos_r) * zoom + pan_y;
    fractal_noise(seed, rx, ry)
}

fn fractal_noise(seed: u64, x: f32, y: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut norm = 0.0;
    for octave in 0..4 {
        total += perlin_noise(seed.wrapping_add(octave), x * frequency, y * frequency) * amplitude;
        norm += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    (total / norm).clamp(0.0, 1.0)
}

fn perlin_noise(seed: u64, x: f32, y: f32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let xf = x - x.floor();
    let yf = y - y.floor();
    let u = smoothstep(xf);
    let v = smoothstep(yf);
    let a = gradient_dot(seed, x0, y0, xf, yf);
    let b = gradient_dot(seed, x0 + 1, y0, xf - 1.0, yf);
    let c = gradient_dot(seed, x0, y0 + 1, xf, yf - 1.0);
    let d = gradient_dot(seed, x0 + 1, y0 + 1, xf - 1.0, yf - 1.0);
    ((lerp(lerp(a, b, u), lerp(c, d, u), v) + 1.0) * 0.5).clamp(0.0, 1.0)
}

fn gradient_dot(seed: u64, x: i32, y: i32, dx: f32, dy: f32) -> f32 {
    let angle = lattice(seed, x, y) * std::f32::consts::TAU;
    dx * angle.cos() + dy * angle.sin()
}

fn lattice(seed: u64, x: i32, y: i32) -> f32 {
    let mut n = seed
        ^ ((x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ ((y as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    n ^= n >> 30;
    n = n.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94D0_49BB_1331_11EB);
    n ^= n >> 31;
    (n as f64 / u64::MAX as f64) as f32
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn rainbow_gradient(value: f32) -> Color {
    const STOPS: [(f32, f32, f32); 7] = [
        (255.0, 36.0, 36.0),
        (255.0, 137.0, 24.0),
        (255.0, 235.0, 59.0),
        (60.0, 215.0, 80.0),
        (50.0, 150.0, 255.0),
        (92.0, 72.0, 255.0),
        (178.0, 80.0, 255.0),
    ];
    let scaled = value.clamp(0.0, 1.0) * (STOPS.len() - 1) as f32;
    let idx = scaled.floor() as usize;
    let next = (idx + 1).min(STOPS.len() - 1);
    let t = scaled - idx as f32;
    let (r0, g0, b0) = STOPS[idx];
    let (r1, g1, b1) = STOPS[next];
    Color::Rgb(
        lerp(r0, r1, t).round() as u8,
        lerp(g0, g1, t).round() as u8,
        lerp(b0, b1, t).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wonderland_postprocess_maps_glyph_positions_to_varied_colors() {
        let mut buffer = Buffer::with_lines([
            "################",
            "################",
            "################",
            "################",
        ]);
        apply_ultimate_postprocess(
            &mut buffer,
            UltimateThemeEffect {
                kind: UltimateEffectKind::Wonderland,
                seed: 42,
                elapsed_ms: 1_250,
            },
        );

        let mut colors = Vec::new();
        for cell in buffer.content() {
            if !colors.contains(&cell.fg) {
                colors.push(cell.fg);
            }
        }
        assert!(
            colors.len() > 3,
            "expected varied glyph colors, got {colors:?}"
        );
    }

    #[test]
    fn wonderland_postprocess_skips_blank_cells() {
        let mut buffer = Buffer::with_lines(["# #"]);
        apply_ultimate_postprocess(
            &mut buffer,
            UltimateThemeEffect {
                kind: UltimateEffectKind::Wonderland,
                seed: 7,
                elapsed_ms: 500,
            },
        );

        assert_ne!(buffer.cell((0, 0)).expect("left glyph").fg, Color::Reset);
        assert_eq!(buffer.cell((1, 0)).expect("blank cell").fg, Color::Reset);
        assert_ne!(buffer.cell((2, 0)).expect("right glyph").fg, Color::Reset);
    }

    #[test]
    fn thematrix_postprocess_uses_background_colors_only() {
        let mut buffer = Buffer::with_lines(["################", "################"]);
        for x in 0..16u16 {
            buffer
                .cell_mut((x, 0))
                .expect("top cell")
                .set_fg(Color::Red);
        }

        apply_ultimate_postprocess(
            &mut buffer,
            UltimateThemeEffect {
                kind: UltimateEffectKind::Thematrix,
                seed: 42,
                elapsed_ms: 1_750,
            },
        );

        assert_eq!(buffer.cell((0, 0)).expect("top left").fg, Color::Red);
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| cell.fg == Color::Red || cell.fg == Color::Reset)
        );
        assert!(buffer.content().iter().all(|cell| cell.bg != Color::Reset));
    }

    #[test]
    fn thematrix_postprocess_creates_varied_background_brightness() {
        let mut buffer = Buffer::with_lines([
            "########################",
            "########################",
            "########################",
            "########################",
            "########################",
            "########################",
            "########################",
            "########################",
        ]);

        apply_ultimate_postprocess(
            &mut buffer,
            UltimateThemeEffect {
                kind: UltimateEffectKind::Thematrix,
                seed: 99,
                elapsed_ms: 2_250,
            },
        );

        let mut backgrounds = Vec::new();
        for cell in buffer.content() {
            if !backgrounds.contains(&cell.bg) {
                backgrounds.push(cell.bg);
            }
        }
        assert!(
            backgrounds.len() > 3,
            "expected varied The Matrix backgrounds, got {backgrounds:?}"
        );
    }

    #[test]
    fn thematrix_z_range_and_top_width_are_bounded() {
        for col in 0..64 {
            for cycle in -4..12 {
                let z = thematrix_z(123, col, cycle, THEMATRIX_Z_MAX);
                assert!(z <= THEMATRIX_Z_MAX);
                assert_eq!(
                    thematrix_line_width(z, THEMATRIX_Z_MAX),
                    if z == THEMATRIX_Z_MAX { 2 } else { 1 }
                );
            }
        }
    }

    #[test]
    fn thematrix_z_extremes_have_distinct_green_intensity() {
        let Color::Rgb(_, low_green, _) = thematrix_green(1.0, 0.0) else {
            panic!("expected rgb color");
        };
        let Color::Rgb(_, high_green, _) = thematrix_green(1.0, 1.0) else {
            panic!("expected rgb color");
        };

        assert!(
            high_green.saturating_sub(low_green) >= 160,
            "expected z max to be much brighter than z min: low={low_green}, high={high_green}"
        );
    }

    #[test]
    fn thematrix_fadeout_keeps_cutoff_lines_without_spawning_new_cycles() {
        let cutoff_secs = THEMATRIX_MAIN_PHASE_MS as f32 / 1000.0;
        let (col, cutoff_line) = (0..80)
            .find_map(|col| {
                thematrix_line_for_col(12, 99, col, cutoff_secs).map(|line| (col, line))
            })
            .expect("expected at least one active cutoff line");

        let fadeout_line =
            thematrix_line_for_col(12, 99, col, cutoff_secs + 0.5).expect("fadeout line");

        assert_eq!(fadeout_line.cycle, cutoff_line.cycle);
        assert!(fadeout_line.head_y > cutoff_line.head_y);
    }

    #[test]
    fn thematrix_fadeout_reaches_zero_at_total_duration() {
        assert_eq!(thematrix_fade_factor(THEMATRIX_MAIN_PHASE_MS), 1.0);
        assert_eq!(thematrix_fade_factor(THEMATRIX_TOTAL_MS), 0.0);
    }
}
