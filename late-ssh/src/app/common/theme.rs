use std::cell::Cell;

use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Late = 0,
    Contrast = 1,
    Purple = 2,
}

#[derive(Clone, Copy)]
pub struct ThemeOption {
    pub kind: ThemeKind,
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Copy)]
struct Palette {
    bg_canvas: Color,
    bg_selection: Color,
    bg_highlight: Color,
    border_dim: Color,
    border: Color,
    border_active: Color,
    text_faint: Color,
    text_dim: Color,
    text_muted: Color,
    text: Color,
    text_bright: Color,
    amber: Color,
    amber_dim: Color,
    amber_glow: Color,
    chat_body: Color,
    chat_author: Color,
    mention: Color,
    success: Color,
    error: Color,
    bot: Color,
    bonsai_sprout: Color,
    bonsai_leaf: Color,
    bonsai_canopy: Color,
    bonsai_bloom: Color,
    badge_bronze: Color,
    badge_silver: Color,
    badge_gold: Color,
}

pub const OPTIONS: &[ThemeOption] = &[
    ThemeOption {
        kind: ThemeKind::Late,
        id: "late",
        label: "Late",
    },
    ThemeOption {
        kind: ThemeKind::Contrast,
        id: "contrast",
        label: "High Contrast",
    },
    ThemeOption {
        kind: ThemeKind::Purple,
        id: "purple",
        label: "Purple Haze",
    },
];

const PALETTE_LATE: Palette = Palette {
    bg_canvas: Color::Rgb(0, 0, 0),
    bg_selection: Color::Rgb(30, 25, 22),
    bg_highlight: Color::Rgb(40, 33, 28),
    border_dim: Color::Rgb(50, 42, 36),
    border: Color::Rgb(68, 56, 46),
    border_active: Color::Rgb(160, 105, 42),
    text_faint: Color::Rgb(78, 65, 54),
    text_dim: Color::Rgb(105, 88, 72),
    text_muted: Color::Rgb(138, 118, 96),
    text: Color::Rgb(175, 158, 138),
    text_bright: Color::Rgb(200, 182, 158),
    amber: Color::Rgb(184, 120, 44),
    amber_dim: Color::Rgb(130, 88, 38),
    amber_glow: Color::Rgb(210, 148, 54),
    chat_body: Color::Rgb(190, 178, 165),
    chat_author: Color::Rgb(140, 160, 175),
    mention: Color::Rgb(228, 196, 78),
    success: Color::Rgb(100, 140, 72),
    error: Color::Rgb(168, 66, 56),
    bot: Color::Indexed(97),
    bonsai_sprout: Color::Rgb(88, 130, 68),
    bonsai_leaf: Color::Rgb(100, 148, 72),
    bonsai_canopy: Color::Rgb(118, 162, 82),
    bonsai_bloom: Color::Rgb(170, 195, 120),
    badge_bronze: Color::Rgb(160, 120, 70),
    badge_silver: Color::Rgb(180, 180, 180),
    badge_gold: Color::Rgb(220, 180, 50),
};

const PALETTE_CONTRAST: Palette = Palette {
    bg_canvas: Color::Rgb(42, 44, 52),
    bg_selection: Color::Rgb(26, 30, 38),
    bg_highlight: Color::Rgb(34, 40, 50),
    border_dim: Color::Rgb(74, 84, 98),
    border: Color::Rgb(115, 130, 150),
    border_active: Color::Rgb(122, 201, 255),
    text_faint: Color::Rgb(126, 138, 155),
    text_dim: Color::Rgb(164, 176, 193),
    text_muted: Color::Rgb(194, 205, 220),
    text: Color::Rgb(226, 234, 245),
    text_bright: Color::Rgb(248, 251, 255),
    amber: Color::Rgb(255, 196, 92),
    amber_dim: Color::Rgb(214, 160, 75),
    amber_glow: Color::Rgb(255, 216, 127),
    chat_body: Color::Rgb(236, 242, 250),
    chat_author: Color::Rgb(144, 207, 255),
    mention: Color::Rgb(255, 229, 122),
    success: Color::Rgb(131, 214, 145),
    error: Color::Rgb(255, 133, 133),
    bot: Color::Rgb(171, 136, 255),
    bonsai_sprout: Color::Rgb(125, 207, 118),
    bonsai_leaf: Color::Rgb(143, 224, 125),
    bonsai_canopy: Color::Rgb(168, 235, 137),
    bonsai_bloom: Color::Rgb(214, 244, 176),
    badge_bronze: Color::Rgb(201, 152, 90),
    badge_silver: Color::Rgb(214, 220, 228),
    badge_gold: Color::Rgb(255, 214, 102),
};

const PALETTE_PURPLE: Palette = Palette {
    bg_canvas: Color::Rgb(55, 57, 76),
    bg_selection: Color::Rgb(44, 26, 66),
    bg_highlight: Color::Rgb(58, 35, 84),
    border_dim: Color::Rgb(92, 72, 122),
    border: Color::Rgb(126, 101, 166),
    border_active: Color::Rgb(255, 171, 247),
    text_faint: Color::Rgb(176, 157, 199),
    text_dim: Color::Rgb(201, 184, 222),
    text_muted: Color::Rgb(220, 207, 236),
    text: Color::Rgb(238, 231, 247),
    text_bright: Color::Rgb(252, 248, 255),
    amber: Color::Rgb(255, 184, 108),
    amber_dim: Color::Rgb(214, 141, 93),
    amber_glow: Color::Rgb(255, 208, 145),
    chat_body: Color::Rgb(244, 238, 250),
    chat_author: Color::Rgb(156, 233, 208),
    mention: Color::Rgb(255, 223, 130),
    success: Color::Rgb(149, 223, 170),
    error: Color::Rgb(255, 148, 181),
    bot: Color::Rgb(194, 149, 255),
    bonsai_sprout: Color::Rgb(130, 210, 142),
    bonsai_leaf: Color::Rgb(147, 227, 159),
    bonsai_canopy: Color::Rgb(174, 238, 170),
    bonsai_bloom: Color::Rgb(220, 248, 196),
    badge_bronze: Color::Rgb(205, 157, 110),
    badge_silver: Color::Rgb(229, 223, 239),
    badge_gold: Color::Rgb(255, 219, 122),
};

thread_local! {
    static CURRENT_THEME: Cell<ThemeKind> = const { Cell::new(ThemeKind::Late) };
}

pub fn normalize_id(id: &str) -> &'static str {
    option_by_id(id).id
}

pub fn set_current_by_id(id: &str) {
    CURRENT_THEME.with(|current| current.set(option_by_id(id).kind));
}

pub fn cycle_id(current_id: &str, forward: bool) -> &'static str {
    let current = option_by_id(current_id).kind;
    let idx = OPTIONS
        .iter()
        .position(|option| option.kind == current)
        .unwrap_or(0);
    let next = if forward {
        (idx + 1) % OPTIONS.len()
    } else {
        (idx + OPTIONS.len() - 1) % OPTIONS.len()
    };
    OPTIONS[next].id
}

pub fn label_for_id(id: &str) -> &'static str {
    option_by_id(id).label
}

pub fn help_text() -> String {
    OPTIONS
        .iter()
        .map(|option| option.label)
        .collect::<Vec<_>>()
        .join(" / ")
}

fn option_by_id(id: &str) -> ThemeOption {
    OPTIONS
        .iter()
        .copied()
        .find(|option| option.id.eq_ignore_ascii_case(id))
        .unwrap_or(OPTIONS[0])
}

fn current_palette() -> &'static Palette {
    CURRENT_THEME.with(|current| match current.get() {
        ThemeKind::Contrast => &PALETTE_CONTRAST,
        ThemeKind::Purple => &PALETTE_PURPLE,
        ThemeKind::Late => &PALETTE_LATE,
    })
}

#[allow(non_snake_case)]
pub fn BG_CANVAS() -> Color {
    current_palette().bg_canvas
}

pub fn color_to_hex(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        Color::Black => "#000000".to_string(),
        Color::DarkGray => "#545454".to_string(),
        Color::Gray => "#a8a8a8".to_string(),
        Color::White => "#ffffff".to_string(),
        // Fallback for others; dynamic themes primarily use Rgb
        _ => "#000000".to_string(),
    }
}

#[allow(non_snake_case)]
pub fn BG_SELECTION() -> Color {
    current_palette().bg_selection
}
#[allow(non_snake_case)]
pub fn BG_HIGHLIGHT() -> Color {
    current_palette().bg_highlight
}
#[allow(non_snake_case)]
pub fn BORDER_DIM() -> Color {
    current_palette().border_dim
}
#[allow(non_snake_case)]
pub fn BORDER() -> Color {
    current_palette().border
}
#[allow(non_snake_case)]
pub fn BORDER_ACTIVE() -> Color {
    current_palette().border_active
}
#[allow(non_snake_case)]
pub fn TEXT_FAINT() -> Color {
    current_palette().text_faint
}
#[allow(non_snake_case)]
pub fn TEXT_DIM() -> Color {
    current_palette().text_dim
}
#[allow(non_snake_case)]
pub fn TEXT_MUTED() -> Color {
    current_palette().text_muted
}
#[allow(non_snake_case)]
pub fn TEXT() -> Color {
    current_palette().text
}
#[allow(non_snake_case)]
pub fn TEXT_BRIGHT() -> Color {
    current_palette().text_bright
}
#[allow(non_snake_case)]
pub fn AMBER() -> Color {
    current_palette().amber
}
#[allow(non_snake_case)]
pub fn AMBER_DIM() -> Color {
    current_palette().amber_dim
}
#[allow(non_snake_case)]
pub fn AMBER_GLOW() -> Color {
    current_palette().amber_glow
}
#[allow(non_snake_case)]
pub fn CHAT_BODY() -> Color {
    current_palette().chat_body
}
#[allow(non_snake_case)]
pub fn CHAT_AUTHOR() -> Color {
    current_palette().chat_author
}
#[allow(non_snake_case)]
pub fn MENTION() -> Color {
    current_palette().mention
}
#[allow(non_snake_case)]
pub fn SUCCESS() -> Color {
    current_palette().success
}
#[allow(non_snake_case)]
pub fn ERROR() -> Color {
    current_palette().error
}
#[allow(non_snake_case)]
pub fn BOT() -> Color {
    current_palette().bot
}
#[allow(non_snake_case)]
pub fn BONSAI_SPROUT() -> Color {
    current_palette().bonsai_sprout
}
#[allow(non_snake_case)]
pub fn BONSAI_LEAF() -> Color {
    current_palette().bonsai_leaf
}
#[allow(non_snake_case)]
pub fn BONSAI_CANOPY() -> Color {
    current_palette().bonsai_canopy
}
#[allow(non_snake_case)]
pub fn BONSAI_BLOOM() -> Color {
    current_palette().bonsai_bloom
}
#[allow(non_snake_case)]
pub fn BADGE_BRONZE() -> Color {
    current_palette().badge_bronze
}
#[allow(non_snake_case)]
pub fn BADGE_SILVER() -> Color {
    current_palette().badge_silver
}
#[allow(non_snake_case)]
pub fn BADGE_GOLD() -> Color {
    current_palette().badge_gold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_unknown_theme_to_default() {
        assert_eq!(normalize_id("wat"), "late");
    }

    #[test]
    fn cycle_theme_wraps() {
        assert_eq!(cycle_id("purple", true), "late");
        assert_eq!(cycle_id("late", false), "purple");
    }
}
