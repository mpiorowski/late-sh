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
    RosePine = 12,
    TokyoNight = 13,
    Kanagawa = 14,
    Dracula = 15,
    Oxocarbon = 16,
    CopperFresh = 17,
    ExposedCopper = 18,
    WeatheredCopper = 19,
    OxidizedCopper = 20,
    Arachne = 21,
    CyberAcme = 22,
    NuCaloric = 23,
    Sekiguchi = 24,
    Traxus = 25,
    Mida = 26,
    ENA = 27,
    ENADreamBbq = 28,
    Kirii = 29,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeGroup {
    Core,
    Catppuccin,
    Coffee,
    Ports,
    Copper,
    Experimental,
}

impl ThemeGroup {
    pub const ALL: [ThemeGroup; 6] = [
        ThemeGroup::Core,
        ThemeGroup::Catppuccin,
        ThemeGroup::Coffee,
        ThemeGroup::Ports,
        ThemeGroup::Copper,
        ThemeGroup::Experimental,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeGroup::Core => "Core",
            ThemeGroup::Catppuccin => "Catppuccin",
            ThemeGroup::Coffee => "Coffee",
            ThemeGroup::Ports => "Ports",
            ThemeGroup::Copper => "Copper",
            ThemeGroup::Experimental => "Experimental",
        }
    }

    pub fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

#[derive(Clone, Copy)]
pub struct ThemeOption {
    pub kind: ThemeKind,
    pub group: ThemeGroup,
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Copy)]
pub struct ThemePreview {
    pub bg_canvas: Color,
    pub bg_selection: Color,
    pub border_active: Color,
    pub text: Color,
    pub text_bright: Color,
    pub amber: Color,
    pub chat_author: Color,
    pub mention: Color,
    pub success: Color,
    pub error: Color,
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
        group: ThemeGroup::Core,
        id: "late",
        label: "Late",
    },
    ThemeOption {
        kind: ThemeKind::Contrast,
        group: ThemeGroup::Core,
        id: "contrast",
        label: "High Contrast",
    },
    ThemeOption {
        kind: ThemeKind::Purple,
        group: ThemeGroup::Core,
        id: "purple",
        label: "Purple Haze",
    },
    ThemeOption {
        kind: ThemeKind::Mocha,
        group: ThemeGroup::Catppuccin,
        id: "mocha",
        label: "Catppuccin Mocha",
    },
    ThemeOption {
        kind: ThemeKind::Macchiato,
        group: ThemeGroup::Catppuccin,
        id: "macchiato",
        label: "Catppuccin Macchiato",
    },
    ThemeOption {
        kind: ThemeKind::Frappe,
        group: ThemeGroup::Catppuccin,
        id: "frappe",
        label: "Catppuccin Frappé",
    },
    ThemeOption {
        kind: ThemeKind::Latte,
        group: ThemeGroup::Catppuccin,
        id: "latte",
        label: "Catppuccin Latte",
    },
    ThemeOption {
        kind: ThemeKind::EggCoffee,
        group: ThemeGroup::Coffee,
        id: "egg_coffee",
        label: "Egg Coffee",
    },
    ThemeOption {
        kind: ThemeKind::Americano,
        group: ThemeGroup::Coffee,
        id: "americano",
        label: "Americano",
    },
    ThemeOption {
        kind: ThemeKind::Espresso,
        group: ThemeGroup::Coffee,
        id: "espresso",
        label: "Espresso",
    },
    ThemeOption {
        kind: ThemeKind::GruvboxDark,
        group: ThemeGroup::Ports,
        id: "gruvboxdark",
        label: "Gruvbox Dark",
    },
    ThemeOption {
        kind: ThemeKind::OneDarkPro,
        group: ThemeGroup::Ports,
        id: "onedarkpro",
        label: "One Dark Pro",
    },
    ThemeOption {
        kind: ThemeKind::RosePine,
        group: ThemeGroup::Ports,
        id: "rosepine",
        label: "Rose Pine",
    },
    ThemeOption {
        kind: ThemeKind::TokyoNight,
        group: ThemeGroup::Ports,
        id: "tokyonight",
        label: "Tokyo Night",
    },
    ThemeOption {
        kind: ThemeKind::Kanagawa,
        group: ThemeGroup::Ports,
        id: "kanagawa",
        label: "Kanagawa",
    },
    ThemeOption {
        kind: ThemeKind::Dracula,
        group: ThemeGroup::Ports,
        id: "dracula",
        label: "Dracula",
    },
    ThemeOption {
        kind: ThemeKind::Oxocarbon,
        group: ThemeGroup::Ports,
        id: "oxocarbon",
        label: "Oxocarbon",
    },
    ThemeOption {
        kind: ThemeKind::CopperFresh,
        group: ThemeGroup::Copper,
        id: "copperfresh",
        label: "Copper Fresh",
    },
    ThemeOption {
        kind: ThemeKind::ExposedCopper,
        group: ThemeGroup::Copper,
        id: "exposedcopper",
        label: "Exposed Copper",
    },
    ThemeOption {
        kind: ThemeKind::WeatheredCopper,
        group: ThemeGroup::Copper,
        id: "weatheredcopper",
        label: "Weathered Copper",
    },
    ThemeOption {
        kind: ThemeKind::OxidizedCopper,
        group: ThemeGroup::Copper,
        id: "oxidizedcopper",
        label: "Oxidized Copper",
    },
    ThemeOption {
        kind: ThemeKind::Arachne,
        group: ThemeGroup::Experimental,
        id: "arachne",
        label: "Arachne",
    },
    ThemeOption {
        kind: ThemeKind::CyberAcme,
        group: ThemeGroup::Experimental,
        id: "cyberacme",
        label: "CyberAcme",
    },
    ThemeOption {
        kind: ThemeKind::NuCaloric,
        group: ThemeGroup::Experimental,
        id: "nucaloric",
        label: "NuCaloric",
    },
    ThemeOption {
        kind: ThemeKind::Sekiguchi,
        group: ThemeGroup::Experimental,
        id: "sekiguchi",
        label: "Sekiguchi",
    },
    ThemeOption {
        kind: ThemeKind::Traxus,
        group: ThemeGroup::Experimental,
        id: "traxus",
        label: "Traxus",
    },
    ThemeOption {
        kind: ThemeKind::Mida,
        group: ThemeGroup::Experimental,
        id: "mida",
        label: "Mida",
    },
    ThemeOption {
        kind: ThemeKind::ENA,
        group: ThemeGroup::Experimental,
        id: "ena",
        label: "ENA",
    },
    ThemeOption {
        kind: ThemeKind::ENADreamBbq,
        group: ThemeGroup::Experimental,
        id: "enadreambbq",
        label: "ENA Dream BBQ",
    },
    ThemeOption {
        kind: ThemeKind::Kirii,
        group: ThemeGroup::Experimental,
        id: "kirii",
        label: "Kirii",
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

const PALETTE_ROSE_PINE: Palette = Palette {
    bg_canvas: Color::Rgb(25, 23, 36),
    bg_selection: Color::Rgb(64, 61, 82),
    bg_highlight: Color::Rgb(31, 29, 46),
    border_dim: Color::Rgb(110, 106, 134),
    border: Color::Rgb(156, 153, 175),
    border_active: Color::Rgb(235, 188, 186),
    text_faint: Color::Rgb(110, 106, 134),
    text_dim: Color::Rgb(156, 153, 175),
    text_muted: Color::Rgb(224, 222, 244),
    text: Color::Rgb(224, 222, 244),
    text_bright: Color::Rgb(235, 111, 146),
    amber: Color::Rgb(246, 193, 119),
    amber_dim: Color::Rgb(196, 143, 69),
    amber_glow: Color::Rgb(246, 193, 119),
    chat_body: Color::Rgb(224, 222, 244),
    chat_author: Color::Rgb(196, 167, 231),
    mention: Color::Rgb(235, 188, 186),
    success: Color::Rgb(49, 116, 143),
    error: Color::Rgb(235, 111, 146),
    bot: Color::Rgb(156, 207, 216),
    bonsai_sprout: Color::Rgb(156, 207, 216),
    bonsai_leaf: Color::Rgb(49, 116, 143),
    bonsai_canopy: Color::Rgb(156, 207, 216),
    bonsai_bloom: Color::Rgb(196, 167, 231),
    badge_bronze: Color::Rgb(235, 111, 146),
    badge_silver: Color::Rgb(224, 222, 244),
    badge_gold: Color::Rgb(246, 193, 119),
};

const PALETTE_TOKYO_NIGHT: Palette = Palette {
    bg_canvas: Color::Rgb(36, 40, 59),
    bg_selection: Color::Rgb(41, 46, 66),
    bg_highlight: Color::Rgb(26, 27, 38),
    border_dim: Color::Rgb(59, 66, 97),
    border: Color::Rgb(86, 95, 137),
    border_active: Color::Rgb(122, 162, 247),
    text_faint: Color::Rgb(86, 95, 137),
    text_dim: Color::Rgb(169, 177, 214),
    text_muted: Color::Rgb(192, 202, 245),
    text: Color::Rgb(192, 202, 245),
    text_bright: Color::Rgb(187, 154, 247),
    amber: Color::Rgb(224, 175, 104),
    amber_dim: Color::Rgb(184, 135, 64),
    amber_glow: Color::Rgb(224, 175, 104),
    chat_body: Color::Rgb(192, 202, 245),
    chat_author: Color::Rgb(42, 195, 222),
    mention: Color::Rgb(255, 158, 100),
    success: Color::Rgb(158, 206, 106),
    error: Color::Rgb(247, 118, 142),
    bot: Color::Rgb(122, 162, 247),
    bonsai_sprout: Color::Rgb(115, 218, 202),
    bonsai_leaf: Color::Rgb(158, 206, 106),
    bonsai_canopy: Color::Rgb(122, 162, 247),
    bonsai_bloom: Color::Rgb(187, 154, 247),
    badge_bronze: Color::Rgb(255, 158, 100),
    badge_silver: Color::Rgb(192, 202, 245),
    badge_gold: Color::Rgb(224, 175, 104),
};

const PALETTE_KANAGAWA: Palette = Palette {
    bg_canvas: Color::Rgb(31, 31, 40),
    bg_selection: Color::Rgb(45, 45, 59),
    bg_highlight: Color::Rgb(22, 22, 29),
    border_dim: Color::Rgb(54, 54, 75),
    border: Color::Rgb(84, 82, 122),
    border_active: Color::Rgb(126, 150, 189),
    text_faint: Color::Rgb(114, 113, 133),
    text_dim: Color::Rgb(156, 156, 156),
    text_muted: Color::Rgb(210, 201, 166),
    text: Color::Rgb(210, 201, 166),
    text_bright: Color::Rgb(230, 180, 80),
    amber: Color::Rgb(255, 160, 102),
    amber_dim: Color::Rgb(196, 112, 60),
    amber_glow: Color::Rgb(255, 160, 102),
    chat_body: Color::Rgb(210, 201, 166),
    chat_author: Color::Rgb(152, 187, 108),
    mention: Color::Rgb(149, 123, 171),
    success: Color::Rgb(152, 187, 108),
    error: Color::Rgb(196, 114, 114),
    bot: Color::Rgb(126, 150, 189),
    bonsai_sprout: Color::Rgb(122, 162, 152),
    bonsai_leaf: Color::Rgb(152, 187, 108),
    bonsai_canopy: Color::Rgb(126, 150, 189),
    bonsai_bloom: Color::Rgb(149, 123, 171),
    badge_bronze: Color::Rgb(196, 112, 60),
    badge_silver: Color::Rgb(114, 113, 133),
    badge_gold: Color::Rgb(230, 180, 80),
};

const PALETTE_DRACULA: Palette = Palette {
    bg_canvas: Color::Rgb(40, 42, 54),
    bg_selection: Color::Rgb(68, 71, 90),
    bg_highlight: Color::Rgb(33, 34, 44),
    border_dim: Color::Rgb(68, 71, 90),
    border: Color::Rgb(98, 114, 164),
    border_active: Color::Rgb(189, 147, 249),
    text_faint: Color::Rgb(98, 114, 164),
    text_dim: Color::Rgb(139, 151, 189),
    text_muted: Color::Rgb(248, 248, 242),
    text: Color::Rgb(248, 248, 242),
    text_bright: Color::Rgb(255, 121, 198),
    amber: Color::Rgb(241, 250, 140),
    amber_dim: Color::Rgb(191, 200, 90),
    amber_glow: Color::Rgb(241, 250, 140),
    chat_body: Color::Rgb(248, 248, 242),
    chat_author: Color::Rgb(139, 233, 253),
    mention: Color::Rgb(255, 184, 108),
    success: Color::Rgb(80, 250, 123),
    error: Color::Rgb(255, 85, 85),
    bot: Color::Rgb(189, 147, 249),
    bonsai_sprout: Color::Rgb(139, 233, 253),
    bonsai_leaf: Color::Rgb(80, 250, 123),
    bonsai_canopy: Color::Rgb(189, 147, 249),
    bonsai_bloom: Color::Rgb(255, 121, 198),
    badge_bronze: Color::Rgb(255, 184, 108),
    badge_silver: Color::Rgb(248, 248, 242),
    badge_gold: Color::Rgb(241, 250, 140),
};

const PALETTE_OXOCARBON: Palette = Palette {
    bg_canvas: Color::Rgb(22, 22, 22),
    bg_selection: Color::Rgb(38, 38, 38),
    bg_highlight: Color::Rgb(14, 14, 14),
    border_dim: Color::Rgb(57, 57, 57),
    border: Color::Rgb(82, 82, 82),
    border_active: Color::Rgb(61, 184, 255),
    text_faint: Color::Rgb(82, 82, 82),
    text_dim: Color::Rgb(182, 182, 182),
    text_muted: Color::Rgb(242, 244, 248),
    text: Color::Rgb(242, 244, 248),
    text_bright: Color::Rgb(255, 126, 182),
    amber: Color::Rgb(190, 149, 255),
    amber_dim: Color::Rgb(140, 99, 205),
    amber_glow: Color::Rgb(190, 149, 255),
    chat_body: Color::Rgb(242, 244, 248),
    chat_author: Color::Rgb(61, 184, 255),
    mention: Color::Rgb(51, 255, 184),
    success: Color::Rgb(61, 184, 255),
    error: Color::Rgb(255, 126, 182),
    bot: Color::Rgb(190, 149, 255),
    bonsai_sprout: Color::Rgb(51, 255, 184),
    bonsai_leaf: Color::Rgb(61, 184, 255),
    bonsai_canopy: Color::Rgb(190, 149, 255),
    bonsai_bloom: Color::Rgb(255, 126, 182),
    badge_bronze: Color::Rgb(255, 126, 182),
    badge_silver: Color::Rgb(242, 244, 248),
    badge_gold: Color::Rgb(190, 149, 255),
};

const PALETTE_COPPER_FRESH: Palette = Palette {
    bg_canvas: Color::Rgb(20, 12, 10),
    bg_selection: Color::Rgb(45, 25, 20),
    bg_highlight: Color::Rgb(12, 8, 7),
    border_dim: Color::Rgb(60, 35, 30),
    border: Color::Rgb(100, 55, 45),
    border_active: Color::Rgb(255, 125, 80),
    text_faint: Color::Rgb(90, 60, 55),
    text_dim: Color::Rgb(160, 120, 110),
    text_muted: Color::Rgb(210, 180, 170),
    text: Color::Rgb(240, 225, 220),
    text_bright: Color::Rgb(255, 180, 140),
    amber: Color::Rgb(255, 140, 60),
    amber_dim: Color::Rgb(150, 70, 30),
    amber_glow: Color::Rgb(255, 180, 100),
    chat_body: Color::Rgb(240, 225, 220),
    chat_author: Color::Rgb(255, 125, 80),
    mention: Color::Rgb(255, 200, 150),
    success: Color::Rgb(210, 100, 60),
    error: Color::Rgb(190, 40, 30),
    bot: Color::Rgb(210, 110, 90),
    bonsai_sprout: Color::Rgb(180, 90, 70),
    bonsai_leaf: Color::Rgb(140, 60, 45),
    bonsai_canopy: Color::Rgb(20, 12, 10),
    bonsai_bloom: Color::Rgb(255, 180, 140),
    badge_bronze: Color::Rgb(140, 70, 50),
    badge_silver: Color::Rgb(190, 190, 190),
    badge_gold: Color::Rgb(255, 180, 140),
};

const PALETTE_EXPOSED_COPPER: Palette = Palette {
    bg_canvas: Color::Rgb(18, 14, 12),
    bg_selection: Color::Rgb(40, 30, 25),
    bg_highlight: Color::Rgb(10, 8, 7),
    border_dim: Color::Rgb(55, 40, 35),
    border: Color::Rgb(85, 65, 55),
    border_active: Color::Rgb(180, 110, 85),
    text_faint: Color::Rgb(80, 70, 65),
    text_dim: Color::Rgb(140, 125, 120),
    text_muted: Color::Rgb(200, 190, 185),
    text: Color::Rgb(225, 215, 210),
    text_bright: Color::Rgb(230, 160, 130),
    amber: Color::Rgb(160, 90, 70),
    amber_dim: Color::Rgb(110, 60, 45),
    amber_glow: Color::Rgb(200, 130, 100),
    chat_body: Color::Rgb(225, 215, 210),
    chat_author: Color::Rgb(180, 110, 85),
    mention: Color::Rgb(190, 140, 120),
    success: Color::Rgb(100, 110, 90),
    error: Color::Rgb(160, 50, 45),
    bot: Color::Rgb(140, 115, 105),
    bonsai_sprout: Color::Rgb(110, 120, 100),
    bonsai_leaf: Color::Rgb(85, 95, 80),
    bonsai_canopy: Color::Rgb(18, 14, 12),
    bonsai_bloom: Color::Rgb(230, 160, 130),
    badge_bronze: Color::Rgb(110, 60, 45),
    badge_silver: Color::Rgb(170, 170, 170),
    badge_gold: Color::Rgb(230, 160, 130),
};

const PALETTE_WEATHERED_COPPER: Palette = Palette {
    bg_canvas: Color::Rgb(12, 18, 16),
    bg_selection: Color::Rgb(30, 45, 40),
    bg_highlight: Color::Rgb(8, 12, 10),
    border_dim: Color::Rgb(45, 60, 55),
    border: Color::Rgb(70, 90, 85),
    border_active: Color::Rgb(80, 180, 160),
    text_faint: Color::Rgb(75, 85, 80),
    text_dim: Color::Rgb(130, 150, 145),
    text_muted: Color::Rgb(190, 210, 205),
    text: Color::Rgb(215, 230, 225),
    text_bright: Color::Rgb(120, 255, 220),
    amber: Color::Rgb(70, 140, 120),
    amber_dim: Color::Rgb(40, 90, 80),
    amber_glow: Color::Rgb(100, 200, 180),
    chat_body: Color::Rgb(215, 230, 225),
    chat_author: Color::Rgb(80, 180, 160),
    mention: Color::Rgb(120, 200, 190),
    success: Color::Rgb(80, 200, 160),
    error: Color::Rgb(180, 70, 70),
    bot: Color::Rgb(90, 150, 140),
    bonsai_sprout: Color::Rgb(110, 160, 150),
    bonsai_leaf: Color::Rgb(75, 120, 110),
    bonsai_canopy: Color::Rgb(12, 18, 16),
    bonsai_bloom: Color::Rgb(120, 255, 220),
    badge_bronze: Color::Rgb(12, 18, 16),
    badge_silver: Color::Rgb(160, 160, 160),
    badge_gold: Color::Rgb(120, 255, 220),
};

const PALETTE_OXIDIZED_COPPER: Palette = Palette {
    bg_canvas: Color::Rgb(8, 15, 15),
    bg_selection: Color::Rgb(20, 40, 40),
    bg_highlight: Color::Rgb(5, 10, 10),
    border_dim: Color::Rgb(35, 60, 60),
    border: Color::Rgb(60, 100, 100),
    border_active: Color::Rgb(0, 255, 220),
    text_faint: Color::Rgb(60, 90, 90),
    text_dim: Color::Rgb(130, 180, 180),
    text_muted: Color::Rgb(190, 230, 230),
    text: Color::Rgb(220, 250, 250),
    text_bright: Color::Rgb(100, 255, 240),
    amber: Color::Rgb(50, 220, 190),
    amber_dim: Color::Rgb(30, 140, 120),
    amber_glow: Color::Rgb(100, 255, 230),
    chat_body: Color::Rgb(220, 250, 250),
    chat_author: Color::Rgb(0, 255, 220),
    mention: Color::Rgb(150, 255, 245),
    success: Color::Rgb(0, 255, 180),
    error: Color::Rgb(220, 80, 80),
    bot: Color::Rgb(80, 220, 200),
    bonsai_sprout: Color::Rgb(130, 255, 210),
    bonsai_leaf: Color::Rgb(50, 180, 160),
    bonsai_canopy: Color::Rgb(8, 15, 15),
    bonsai_bloom: Color::Rgb(100, 255, 240),
    badge_bronze: Color::Rgb(8, 15, 15),
    badge_silver: Color::Rgb(200, 220, 220),
    badge_gold: Color::Rgb(100, 255, 240),
};

const PALETTE_ARACHNE: Palette = Palette {
    bg_canvas: Color::Rgb(5, 5, 5),
    bg_selection: Color::Rgb(40, 0, 0),
    bg_highlight: Color::Rgb(15, 0, 0),
    border_dim: Color::Rgb(108, 0, 4),
    border: Color::Rgb(176, 0, 0),
    border_active: Color::Rgb(206, 7, 36),
    text_faint: Color::Rgb(80, 20, 20),
    text_dim: Color::Rgb(150, 50, 50),
    text_muted: Color::Rgb(209, 209, 209),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(206, 7, 36),
    amber: Color::Rgb(176, 0, 0),
    amber_dim: Color::Rgb(108, 0, 4),
    amber_glow: Color::Rgb(206, 7, 36),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(206, 7, 36),
    mention: Color::Rgb(206, 7, 36),
    success: Color::Rgb(176, 0, 0),
    error: Color::Rgb(206, 7, 36),
    bot: Color::Rgb(108, 0, 4),
    bonsai_sprout: Color::Rgb(176, 0, 0),
    bonsai_leaf: Color::Rgb(108, 0, 4),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(206, 7, 36),
    badge_bronze: Color::Rgb(108, 0, 4),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(206, 7, 36),
};

const PALETTE_CYBER_ACME: Palette = Palette {
    bg_canvas: Color::Rgb(5, 5, 5),
    bg_selection: Color::Rgb(10, 20, 50),
    bg_highlight: Color::Rgb(5, 10, 25),
    border_dim: Color::Rgb(26, 18, 237),
    border: Color::Rgb(38, 234, 91),
    border_active: Color::Rgb(91, 249, 254),
    text_faint: Color::Rgb(26, 18, 237),
    text_dim: Color::Rgb(38, 234, 91),
    text_muted: Color::Rgb(209, 209, 209),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(91, 249, 254),
    amber: Color::Rgb(38, 234, 91),
    amber_dim: Color::Rgb(26, 18, 237),
    amber_glow: Color::Rgb(91, 249, 254),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(38, 234, 91),
    mention: Color::Rgb(91, 249, 254),
    success: Color::Rgb(38, 234, 91),
    error: Color::Rgb(26, 18, 237),
    bot: Color::Rgb(91, 249, 254),
    bonsai_sprout: Color::Rgb(91, 249, 254),
    bonsai_leaf: Color::Rgb(38, 234, 91),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(26, 18, 237),
    badge_bronze: Color::Rgb(26, 18, 237),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(91, 249, 254),
};

const PALETTE_NU_CALORIC: Palette = Palette {
    bg_canvas: Color::Rgb(5, 5, 5),
    bg_selection: Color::Rgb(50, 10, 20),
    bg_highlight: Color::Rgb(25, 5, 10),
    border_dim: Color::Rgb(166, 4, 36),
    border: Color::Rgb(250, 97, 150),
    border_active: Color::Rgb(255, 228, 142),
    text_faint: Color::Rgb(166, 4, 36),
    text_dim: Color::Rgb(250, 97, 150),
    text_muted: Color::Rgb(209, 209, 209),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(255, 228, 142),
    amber: Color::Rgb(250, 97, 150),
    amber_dim: Color::Rgb(166, 4, 36),
    amber_glow: Color::Rgb(255, 228, 142),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(250, 97, 150),
    mention: Color::Rgb(255, 228, 142),
    success: Color::Rgb(250, 97, 150),
    error: Color::Rgb(166, 4, 36),
    bot: Color::Rgb(255, 228, 142),
    bonsai_sprout: Color::Rgb(255, 228, 142),
    bonsai_leaf: Color::Rgb(250, 97, 150),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(166, 4, 36),
    badge_bronze: Color::Rgb(166, 4, 36),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(255, 228, 142),
};

const PALETTE_SEKIGUCHI: Palette = Palette {
    bg_canvas: Color::Rgb(5, 5, 5),
    bg_selection: Color::Rgb(25, 10, 40),
    bg_highlight: Color::Rgb(12, 5, 20),
    border_dim: Color::Rgb(84, 23, 155),
    border: Color::Rgb(117, 86, 181),
    border_active: Color::Rgb(130, 245, 187),
    text_faint: Color::Rgb(84, 23, 155),
    text_dim: Color::Rgb(117, 86, 181),
    text_muted: Color::Rgb(209, 209, 209),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(130, 245, 187),
    amber: Color::Rgb(117, 86, 181),
    amber_dim: Color::Rgb(84, 23, 155),
    amber_glow: Color::Rgb(130, 245, 187),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(117, 86, 181),
    mention: Color::Rgb(130, 245, 187),
    success: Color::Rgb(130, 245, 187),
    error: Color::Rgb(84, 23, 155),
    bot: Color::Rgb(117, 86, 181),
    bonsai_sprout: Color::Rgb(130, 245, 187),
    bonsai_leaf: Color::Rgb(117, 86, 181),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(84, 23, 155),
    badge_bronze: Color::Rgb(84, 23, 155),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(130, 245, 187),
};

const PALETTE_TRAXUS: Palette = Palette {
    bg_canvas: Color::Rgb(5, 5, 5),
    bg_selection: Color::Rgb(60, 30, 0),
    bg_highlight: Color::Rgb(25, 12, 0),
    border_dim: Color::Rgb(144, 55, 3),
    border: Color::Rgb(238, 127, 0),
    border_active: Color::Rgb(255, 158, 0),
    text_faint: Color::Rgb(144, 55, 3),
    text_dim: Color::Rgb(238, 127, 0),
    text_muted: Color::Rgb(209, 209, 209),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(255, 158, 0),
    amber: Color::Rgb(255, 158, 0),
    amber_dim: Color::Rgb(238, 127, 0),
    amber_glow: Color::Rgb(255, 200, 100),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(255, 158, 0),
    mention: Color::Rgb(255, 158, 0),
    success: Color::Rgb(238, 127, 0),
    error: Color::Rgb(144, 55, 3),
    bot: Color::Rgb(255, 158, 0),
    bonsai_sprout: Color::Rgb(255, 158, 0),
    bonsai_leaf: Color::Rgb(238, 127, 0),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(144, 55, 3),
    badge_bronze: Color::Rgb(144, 55, 3),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(255, 158, 0),
};

const PALETTE_MIDA: Palette = Palette {
    bg_canvas: Color::Rgb(5, 5, 5),
    bg_selection: Color::Rgb(15, 25, 40),
    bg_highlight: Color::Rgb(8, 12, 20),
    border_dim: Color::Rgb(34, 92, 132),
    border: Color::Rgb(203, 109, 246),
    border_active: Color::Rgb(204, 214, 179),
    text_faint: Color::Rgb(34, 92, 132),
    text_dim: Color::Rgb(203, 109, 246),
    text_muted: Color::Rgb(209, 209, 209),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(204, 214, 179),
    amber: Color::Rgb(227, 128, 65),
    amber_dim: Color::Rgb(34, 92, 132),
    amber_glow: Color::Rgb(204, 214, 179),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(203, 109, 246),
    mention: Color::Rgb(227, 128, 65),
    success: Color::Rgb(204, 214, 179),
    error: Color::Rgb(227, 128, 65),
    bot: Color::Rgb(203, 109, 246),
    bonsai_sprout: Color::Rgb(204, 214, 179),
    bonsai_leaf: Color::Rgb(203, 109, 246),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(227, 128, 65),
    badge_bronze: Color::Rgb(34, 92, 132),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(204, 214, 179),
};

const PALETTE_ENA: Palette = Palette {
    bg_canvas: Color::Rgb(5, 5, 5),
    bg_selection: Color::Rgb(41, 95, 247),
    bg_highlight: Color::Rgb(15, 20, 35),
    border_dim: Color::Rgb(41, 95, 247),
    border: Color::Rgb(253, 231, 1),
    border_active: Color::Rgb(227, 207, 182),
    text_faint: Color::Rgb(41, 95, 247),
    text_dim: Color::Rgb(253, 231, 1),
    text_muted: Color::Rgb(180, 180, 180),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(227, 207, 182),
    amber: Color::Rgb(253, 231, 1),
    amber_dim: Color::Rgb(41, 95, 247),
    amber_glow: Color::Rgb(227, 207, 182),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(253, 231, 1),
    mention: Color::Rgb(227, 207, 182),
    success: Color::Rgb(253, 231, 1),
    error: Color::Rgb(255, 0, 50),
    bot: Color::Rgb(227, 207, 182),
    bonsai_sprout: Color::Rgb(227, 207, 182),
    bonsai_leaf: Color::Rgb(41, 95, 247),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(253, 231, 1),
    badge_bronze: Color::Rgb(41, 95, 247),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(227, 207, 182),
};

const PALETTE_ENA_DREAM_BBQ: Palette = Palette {
    bg_canvas: Color::Rgb(5, 5, 5),
    bg_selection: Color::Rgb(21, 94, 85),
    bg_highlight: Color::Rgb(10, 35, 32),
    border_dim: Color::Rgb(91, 134, 148),
    border: Color::Rgb(230, 131, 140),
    border_active: Color::Rgb(241, 230, 198),
    text_faint: Color::Rgb(21, 94, 85),
    text_dim: Color::Rgb(143, 183, 198),
    text_muted: Color::Rgb(209, 209, 209),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(241, 230, 198),
    amber: Color::Rgb(225, 63, 61),
    amber_dim: Color::Rgb(143, 183, 198),
    amber_glow: Color::Rgb(241, 230, 198),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(230, 131, 140),
    mention: Color::Rgb(241, 230, 198),
    success: Color::Rgb(143, 183, 198),
    error: Color::Rgb(225, 63, 61),
    bot: Color::Rgb(241, 230, 198),
    bonsai_sprout: Color::Rgb(241, 230, 198),
    bonsai_leaf: Color::Rgb(21, 94, 85),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(225, 63, 61),
    badge_bronze: Color::Rgb(21, 94, 85),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(241, 230, 198),
};

const PALETTE_KIRII: Palette = Palette {
    bg_canvas: Color::Rgb(15, 1, 89),
    bg_selection: Color::Rgb(13, 0, 99),
    bg_highlight: Color::Rgb(28, 15, 74),
    border_dim: Color::Rgb(41, 37, 255),
    border: Color::Rgb(183, 141, 255),
    border_active: Color::Rgb(255, 153, 221),
    text_faint: Color::Rgb(41, 37, 255),
    text_dim: Color::Rgb(183, 141, 255),
    text_muted: Color::Rgb(180, 180, 180),
    text: Color::Rgb(209, 209, 209),
    text_bright: Color::Rgb(212, 255, 250),
    amber: Color::Rgb(255, 153, 221),
    amber_dim: Color::Rgb(41, 37, 255),
    amber_glow: Color::Rgb(212, 255, 250),
    chat_body: Color::Rgb(209, 209, 209),
    chat_author: Color::Rgb(255, 153, 221),
    mention: Color::Rgb(212, 255, 250),
    success: Color::Rgb(212, 255, 250),
    error: Color::Rgb(255, 0, 80),
    bot: Color::Rgb(183, 141, 255),
    bonsai_sprout: Color::Rgb(255, 153, 221),
    bonsai_leaf: Color::Rgb(41, 37, 255),
    bonsai_canopy: Color::Rgb(5, 5, 5),
    bonsai_bloom: Color::Rgb(212, 255, 250),
    badge_bronze: Color::Rgb(13, 0, 99),
    badge_silver: Color::Rgb(209, 209, 209),
    badge_gold: Color::Rgb(255, 153, 221),
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
    CURRENT_THEME.with(|current| palette_for_kind(current.get()))
}

fn palette_for_kind(kind: ThemeKind) -> &'static Palette {
    match kind {
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
        ThemeKind::RosePine => &PALETTE_ROSE_PINE,
        ThemeKind::TokyoNight => &PALETTE_TOKYO_NIGHT,
        ThemeKind::Kanagawa => &PALETTE_KANAGAWA,
        ThemeKind::Dracula => &PALETTE_DRACULA,
        ThemeKind::Oxocarbon => &PALETTE_OXOCARBON,
        ThemeKind::CopperFresh => &PALETTE_COPPER_FRESH,
        ThemeKind::ExposedCopper => &PALETTE_EXPOSED_COPPER,
        ThemeKind::WeatheredCopper => &PALETTE_WEATHERED_COPPER,
        ThemeKind::OxidizedCopper => &PALETTE_OXIDIZED_COPPER,
        ThemeKind::Arachne => &PALETTE_ARACHNE,
        ThemeKind::CyberAcme => &PALETTE_CYBER_ACME,
        ThemeKind::NuCaloric => &PALETTE_NU_CALORIC,
        ThemeKind::Sekiguchi => &PALETTE_SEKIGUCHI,
        ThemeKind::Traxus => &PALETTE_TRAXUS,
        ThemeKind::Mida => &PALETTE_MIDA,
        ThemeKind::ENA => &PALETTE_ENA,
        ThemeKind::ENADreamBbq => &PALETTE_ENA_DREAM_BBQ,
        ThemeKind::Kirii => &PALETTE_KIRII,
        ThemeKind::Late => &PALETTE_LATE,
    }
}

pub fn preview_for_id(id: &str) -> ThemePreview {
    preview_for_option(option_by_id(id))
}

pub fn preview_for_option(option: ThemeOption) -> ThemePreview {
    let palette = palette_for_kind(option.kind);
    ThemePreview {
        bg_canvas: palette.bg_canvas,
        bg_selection: palette.bg_selection,
        border_active: palette.border_active,
        text: palette.text,
        text_bright: palette.text_bright,
        amber: palette.amber,
        chat_author: palette.chat_author,
        mention: palette.mention,
        success: palette.success,
        error: palette.error,
    }
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
        assert_eq!(cycle_id("kirii", true), "late");
        assert_eq!(cycle_id("late", false), "kirii");
    }
}
