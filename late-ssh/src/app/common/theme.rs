use ratatui::style::Color;

// ── late.sh color palette ──────────────────────────────────────────
// Late-night coffee shop: dark wood, warm amber, candlelit parchment.
// Everything muted. Reserve bright accents for what truly matters.

// ── Surface / Background ──
pub const CANVAS: Color = Color::Rgb(0, 0, 0);
pub const BG_SELECTION: Color = Color::Rgb(30, 25, 22);
pub const BG_HIGHLIGHT: Color = Color::Rgb(40, 33, 28);

// ── Borders ──
pub const BORDER_DIM: Color = Color::Rgb(50, 42, 36);
pub const BORDER: Color = Color::Rgb(68, 56, 46);
pub const BORDER_ACTIVE: Color = Color::Rgb(160, 105, 42);

// ── Text ──
pub const TEXT_FAINT: Color = Color::Rgb(78, 65, 54);
pub const TEXT_DIM: Color = Color::Rgb(105, 88, 72);
pub const TEXT_MUTED: Color = Color::Rgb(138, 118, 96);
pub const TEXT: Color = Color::Rgb(175, 158, 138);
pub const TEXT_BRIGHT: Color = Color::Rgb(200, 182, 158);

// ── Accent (Amber) ──
pub const AMBER: Color = Color::Rgb(184, 120, 44);
pub const AMBER_DIM: Color = Color::Rgb(130, 88, 38);
pub const AMBER_GLOW: Color = Color::Rgb(210, 148, 54);

// ── Chat ──
pub const CHAT_BODY: Color = Color::Rgb(190, 178, 165);
pub const CHAT_AUTHOR: Color = Color::Rgb(140, 160, 175);

// ── Highlight ──
pub const MENTION: Color = Color::Rgb(228, 196, 78);

// ── Status ──
pub const SUCCESS: Color = Color::Rgb(100, 140, 72);
pub const ERROR: Color = Color::Rgb(168, 66, 56);
pub const BOT: Color = Color::Indexed(97);
// ── Bonsai greens ──
pub const BONSAI_SPROUT: Color = Color::Rgb(88, 130, 68);
pub const BONSAI_LEAF: Color = Color::Rgb(100, 148, 72);
pub const BONSAI_CANOPY: Color = Color::Rgb(118, 162, 82);
pub const BONSAI_BLOOM: Color = Color::Rgb(170, 195, 120);

// ── Badges (streak tiers) ──
pub const BADGE_BRONZE: Color = Color::Rgb(160, 120, 70);
pub const BADGE_SILVER: Color = Color::Rgb(180, 180, 180);
pub const BADGE_GOLD: Color = Color::Rgb(220, 180, 50);
