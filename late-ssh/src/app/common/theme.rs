use ratatui::style::Color;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Late = 0,
    Contrast = 1,
    Purple = 2,
    Mocha = 3,
    Macchiato = 4,
    Frappe = 5,
    Latte = 6,
    EggCoffee = 7,
    Americano = 8,
    Espresso = 9,
    GruvboxDark = 10,
    OneDarkPro = 11,
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
    ThemeOption {
        kind: ThemeKind::Mocha,
        id: "mocha",
        label: "Catppuccin Mocha",
    },
    ThemeOption {
        kind: ThemeKind::Macchiato,
        id: "macchiato",
        label: "Catppuccin Macchiato",
    },
    ThemeOption {
        kind: ThemeKind::Frappe,
        id: "frappe",
        label: "Catppuccin Frappé",
    },
    ThemeOption {
        kind: ThemeKind::Latte,
        id: "latte",
        label: "Catppuccin Latte",
    },
    ThemeOption {
        kind: ThemeKind::EggCoffee,
        id: "egg_coffee",
        label: "Egg Coffee",
    },
    ThemeOption {
        kind: ThemeKind::Americano,
        id: "americano",
        label: "Americano",
    },
    ThemeOption {
        kind: ThemeKind::Espresso,
        id: "espresso",
        label: "Espresso",
    },
    ThemeOption {
        kind: ThemeKind::GruvboxDark,
        id: "gruvboxdark",
        label: "Gruvbox Dark",
    },
    ThemeOption {
        kind: ThemeKind::OneDarkPro,
        id: "onedarkpro",
        label: "One Dark Pro",
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

const PALETTE_MOCHA: Palette = Palette {
    bg_canvas: Color::Rgb(30, 30, 46),
    bg_selection: Color::Rgb(69, 71, 90),
    bg_highlight: Color::Rgb(24, 24, 37),
    border_dim: Color::Rgb(49, 50, 68),
    border: Color::Rgb(88, 91, 112),
    border_active: Color::Rgb(203, 166, 247),
    text_faint: Color::Rgb(108, 112, 134),
    text_dim: Color::Rgb(147, 153, 178),
    text_muted: Color::Rgb(166, 173, 200),
    text: Color::Rgb(205, 214, 244),
    text_bright: Color::Rgb(245, 224, 220),
    amber: Color::Rgb(250, 179, 135),
    amber_dim: Color::Rgb(200, 129, 35),
    amber_glow: Color::Rgb(249, 226, 175),
    chat_body: Color::Rgb(205, 214, 244),
    chat_author: Color::Rgb(137, 180, 250),
    mention: Color::Rgb(245, 194, 231),
    success: Color::Rgb(166, 227, 161),
    error: Color::Rgb(243, 139, 168),
    bot: Color::Rgb(180, 190, 254),
    bonsai_sprout: Color::Rgb(148, 226, 213),
    bonsai_leaf: Color::Rgb(166, 227, 161),
    bonsai_canopy: Color::Rgb(137, 220, 235),
    bonsai_bloom: Color::Rgb(203, 166, 247),
    badge_bronze: Color::Rgb(235, 160, 120),
    badge_silver: Color::Rgb(186, 194, 222),
    badge_gold: Color::Rgb(249, 226, 175),
};

const PALETTE_MACCHIATO: Palette = Palette {
    bg_canvas: Color::Rgb(36, 39, 58),
    bg_selection: Color::Rgb(65, 69, 89),
    bg_highlight: Color::Rgb(30, 32, 48),
    border_dim: Color::Rgb(46, 49, 71),
    border: Color::Rgb(73, 77, 100),
    border_active: Color::Rgb(198, 160, 246),
    text_faint: Color::Rgb(110, 115, 141),
    text_dim: Color::Rgb(165, 173, 203),
    text_muted: Color::Rgb(184, 192, 224),
    text: Color::Rgb(202, 211, 245),
    text_bright: Color::Rgb(244, 219, 214),
    amber: Color::Rgb(245, 169, 127),
    amber_dim: Color::Rgb(195, 119, 77),
    amber_glow: Color::Rgb(238, 212, 159),
    chat_body: Color::Rgb(202, 211, 245),
    chat_author: Color::Rgb(138, 173, 244),
    mention: Color::Rgb(245, 189, 230),
    success: Color::Rgb(166, 218, 149),
    error: Color::Rgb(237, 135, 150),
    bot: Color::Rgb(183, 189, 248),
    bonsai_sprout: Color::Rgb(145, 215, 227),
    bonsai_leaf: Color::Rgb(166, 218, 149),
    bonsai_canopy: Color::Rgb(145, 215, 227),
    bonsai_bloom: Color::Rgb(198, 160, 246),
    badge_bronze: Color::Rgb(238, 153, 114),
    badge_silver: Color::Rgb(174, 182, 211),
    badge_gold: Color::Rgb(238, 212, 159),
};

const PALETTE_FRAPPE: Palette = Palette {
    bg_canvas: Color::Rgb(48, 52, 70),
    bg_selection: Color::Rgb(81, 87, 109),
    bg_highlight: Color::Rgb(41, 44, 60),
    border_dim: Color::Rgb(65, 69, 89),
    border: Color::Rgb(98, 104, 128),
    border_active: Color::Rgb(202, 158, 230),
    text_faint: Color::Rgb(115, 121, 148),
    text_dim: Color::Rgb(165, 172, 196),
    text_muted: Color::Rgb(181, 191, 226),
    text: Color::Rgb(198, 208, 245),
    text_bright: Color::Rgb(242, 213, 207),
    amber: Color::Rgb(239, 159, 118),
    amber_dim: Color::Rgb(189, 109, 68),
    amber_glow: Color::Rgb(229, 200, 144),
    chat_body: Color::Rgb(198, 208, 245),
    chat_author: Color::Rgb(140, 170, 238),
    mention: Color::Rgb(244, 184, 228),
    success: Color::Rgb(166, 209, 137),
    error: Color::Rgb(231, 130, 132),
    bot: Color::Rgb(186, 187, 241),
    bonsai_sprout: Color::Rgb(129, 200, 190),
    bonsai_leaf: Color::Rgb(166, 209, 137),
    bonsai_canopy: Color::Rgb(153, 209, 219),
    bonsai_bloom: Color::Rgb(202, 158, 230),
    badge_bronze: Color::Rgb(231, 145, 106),
    badge_silver: Color::Rgb(173, 184, 216),
    badge_gold: Color::Rgb(229, 200, 144),
};

const PALETTE_LATTE: Palette = Palette {
    bg_canvas: Color::Rgb(239, 241, 245),
    bg_selection: Color::Rgb(172, 176, 190),
    bg_highlight: Color::Rgb(188, 192, 204),
    border_dim: Color::Rgb(204, 208, 218),
    border: Color::Rgb(156, 160, 176),
    border_active: Color::Rgb(136, 57, 239),
    text_faint: Color::Rgb(140, 143, 161),
    text_dim: Color::Rgb(92, 95, 119),
    text_muted: Color::Rgb(76, 79, 105),
    text: Color::Rgb(76, 79, 105),
    text_bright: Color::Rgb(220, 138, 120),
    amber: Color::Rgb(254, 100, 11),
    amber_dim: Color::Rgb(204, 50, 0),
    amber_glow: Color::Rgb(223, 142, 29),
    chat_body: Color::Rgb(76, 79, 105),
    chat_author: Color::Rgb(30, 102, 245),
    mention: Color::Rgb(234, 118, 203),
    success: Color::Rgb(64, 160, 43),
    error: Color::Rgb(210, 15, 57),
    bot: Color::Rgb(114, 135, 253),
    bonsai_sprout: Color::Rgb(23, 146, 153),
    bonsai_leaf: Color::Rgb(64, 160, 43),
    bonsai_canopy: Color::Rgb(4, 165, 229),
    bonsai_bloom: Color::Rgb(136, 57, 239),
    badge_bronze: Color::Rgb(230, 69, 83),
    badge_silver: Color::Rgb(156, 160, 176),
    badge_gold: Color::Rgb(223, 142, 29),
};

const PALETTE_EGG_COFFEE: Palette = Palette {
    bg_canvas: Color::Rgb(26, 15, 13),
    bg_selection: Color::Rgb(61, 38, 32),
    bg_highlight: Color::Rgb(157, 110, 59),
    border_dim: Color::Rgb(64, 45, 40),
    border: Color::Rgb(92, 70, 64),
    border_active: Color::Rgb(236, 184, 84),
    text_faint: Color::Rgb(120, 105, 100),
    text_dim: Color::Rgb(170, 155, 150),
    text_muted: Color::Rgb(210, 195, 190),
    text: Color::Rgb(249, 241, 231),
    text_bright: Color::Rgb(255, 252, 249),
    amber: Color::Rgb(236, 184, 84),
    amber_dim: Color::Rgb(180, 140, 65),
    amber_glow: Color::Rgb(255, 220, 140),
    chat_body: Color::Rgb(235, 225, 215),
    chat_author: Color::Rgb(236, 184, 84),
    mention: Color::Rgb(255, 240, 180),
    success: Color::Rgb(236, 184, 84),
    error: Color::Rgb(158, 50, 40),
    bot: Color::Rgb(150, 140, 230),
    bonsai_sprout: Color::Rgb(140, 160, 120),
    bonsai_leaf: Color::Rgb(120, 140, 100),
    bonsai_canopy: Color::Rgb(100, 120, 80),
    bonsai_bloom: Color::Rgb(236, 184, 84),
    badge_bronze: Color::Rgb(177, 140, 89),
    badge_silver: Color::Rgb(180, 185, 190),
    badge_gold: Color::Rgb(236, 184, 84),
};

const PALETTE_AMERICANO: Palette = Palette {
    bg_canvas: Color::Rgb(20, 18, 18),
    bg_selection: Color::Rgb(35, 32, 32),
    bg_highlight: Color::Rgb(42, 38, 38),
    border_dim: Color::Rgb(55, 50, 50),
    border: Color::Rgb(85, 78, 78),
    border_active: Color::Rgb(141, 125, 119),
    text_faint: Color::Rgb(100, 95, 95),
    text_dim: Color::Rgb(140, 135, 135),
    text_muted: Color::Rgb(180, 175, 175),
    text: Color::Rgb(210, 205, 205),
    text_bright: Color::Rgb(236, 239, 244),
    amber: Color::Rgb(161, 136, 127),
    amber_dim: Color::Rgb(121, 85, 72),
    amber_glow: Color::Rgb(188, 170, 164),
    chat_body: Color::Rgb(200, 195, 195),
    chat_author: Color::Rgb(141, 125, 119),
    mention: Color::Rgb(174, 213, 129),
    success: Color::Rgb(141, 125, 119),
    error: Color::Rgb(191, 97, 106),
    bot: Color::Rgb(180, 190, 254),
    bonsai_sprout: Color::Rgb(163, 190, 140),
    bonsai_leaf: Color::Rgb(143, 168, 120),
    bonsai_canopy: Color::Rgb(118, 140, 98),
    bonsai_bloom: Color::Rgb(236, 239, 244),
    badge_bronze: Color::Rgb(141, 125, 119),
    badge_silver: Color::Rgb(184, 192, 204),
    badge_gold: Color::Rgb(235, 203, 139),
};

const PALETTE_ESPRESSO: Palette = Palette {
    bg_canvas: Color::Rgb(10, 8, 7),
    bg_selection: Color::Rgb(36, 26, 23),
    bg_highlight: Color::Rgb(31, 26, 24),
    border_dim: Color::Rgb(58, 40, 35),
    border: Color::Rgb(84, 58, 50),
    border_active: Color::Rgb(210, 105, 30),
    text_faint: Color::Rgb(100, 85, 80),
    text_dim: Color::Rgb(150, 135, 130),
    text_muted: Color::Rgb(200, 190, 185),
    text: Color::Rgb(245, 245, 245),
    text_bright: Color::Rgb(255, 255, 255),
    amber: Color::Rgb(210, 105, 30),
    amber_dim: Color::Rgb(139, 69, 19),
    amber_glow: Color::Rgb(255, 165, 0),
    chat_body: Color::Rgb(240, 240, 240),
    chat_author: Color::Rgb(210, 105, 30),
    mention: Color::Rgb(255, 215, 0),
    success: Color::Rgb(210, 105, 30),
    error: Color::Rgb(255, 70, 70),
    bot: Color::Rgb(180, 160, 255),
    bonsai_sprout: Color::Rgb(107, 142, 35),
    bonsai_leaf: Color::Rgb(85, 107, 47),
    bonsai_canopy: Color::Rgb(139, 69, 19),
    bonsai_bloom: Color::Rgb(210, 105, 30),
    badge_bronze: Color::Rgb(139, 69, 19),
    badge_silver: Color::Rgb(192, 192, 192),
    badge_gold: Color::Rgb(255, 215, 0),
};

const PALETTE_GRUVBOX_DARK: Palette = Palette {
    bg_canvas: Color::Rgb(40, 40, 40),
    bg_selection: Color::Rgb(60, 56, 54),
    bg_highlight: Color::Rgb(29, 32, 33),
    border_dim: Color::Rgb(80, 73, 69),
    border: Color::Rgb(102, 92, 84),
    border_active: Color::Rgb(214, 93, 14),
    text_faint: Color::Rgb(146, 131, 116),
    text_dim: Color::Rgb(168, 153, 132),
    text_muted: Color::Rgb(189, 174, 147),
    text: Color::Rgb(235, 219, 178),
    text_bright: Color::Rgb(251, 241, 199),
    amber: Color::Rgb(215, 153, 33),
    amber_dim: Color::Rgb(175, 124, 12),
    amber_glow: Color::Rgb(250, 189, 47),
    chat_body: Color::Rgb(235, 219, 178),
    chat_author: Color::Rgb(184, 187, 38),
    mention: Color::Rgb(211, 134, 155),
    success: Color::Rgb(184, 187, 38),
    error: Color::Rgb(251, 73, 52),
    bot: Color::Rgb(131, 165, 152),
    bonsai_sprout: Color::Rgb(142, 192, 124),
    bonsai_leaf: Color::Rgb(184, 187, 38),
    bonsai_canopy: Color::Rgb(131, 165, 152),
    bonsai_bloom: Color::Rgb(251, 73, 52),
    badge_bronze: Color::Rgb(214, 93, 14),
    badge_silver: Color::Rgb(168, 153, 132),
    badge_gold: Color::Rgb(250, 189, 47),
};

const PALETTE_ONE_DARK_PRO: Palette = Palette {
    bg_canvas: Color::Rgb(30, 33, 39),
    bg_selection: Color::Rgb(44, 50, 60),
    bg_highlight: Color::Rgb(24, 26, 31),
    border_dim: Color::Rgb(55, 63, 75),
    border: Color::Rgb(75, 83, 98),
    border_active: Color::Rgb(77, 181, 255),
    text_faint: Color::Rgb(84, 91, 105),
    text_dim: Color::Rgb(140, 150, 170),
    text_muted: Color::Rgb(171, 178, 191),
    text: Color::Rgb(219, 226, 239),
    text_bright: Color::Rgb(239, 89, 111),
    amber: Color::Rgb(235, 186, 91),
    amber_dim: Color::Rgb(180, 140, 60),
    amber_glow: Color::Rgb(235, 186, 91),
    chat_body: Color::Rgb(219, 226, 239),
    chat_author: Color::Rgb(213, 95, 222),
    mention: Color::Rgb(78, 188, 202),
    success: Color::Rgb(141, 193, 101),
    error: Color::Rgb(239, 89, 111),
    bot: Color::Rgb(77, 181, 255),
    bonsai_sprout: Color::Rgb(78, 188, 202),
    bonsai_leaf: Color::Rgb(141, 193, 101),
    bonsai_canopy: Color::Rgb(77, 181, 255),
    bonsai_bloom: Color::Rgb(213, 95, 222),
    badge_bronze: Color::Rgb(224, 152, 90),
    badge_silver: Color::Rgb(219, 226, 239),
    badge_gold: Color::Rgb(235, 186, 91),
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
        ThemeKind::Mocha => &PALETTE_MOCHA,
        ThemeKind::Macchiato => &PALETTE_MACCHIATO,
        ThemeKind::Frappe => &PALETTE_FRAPPE,
        ThemeKind::Latte => &PALETTE_LATTE,
        ThemeKind::EggCoffee => &PALETTE_EGG_COFFEE,
        ThemeKind::Americano => &PALETTE_AMERICANO,
        ThemeKind::Espresso => &PALETTE_ESPRESSO,
        ThemeKind::GruvboxDark => &PALETTE_GRUVBOX_DARK,
        ThemeKind::OneDarkPro => &PALETTE_ONE_DARK_PRO,
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
        assert_eq!(cycle_id("onedarkpro", true), "late");
        assert_eq!(cycle_id("late", false), "onedarkpro");
    }
}
