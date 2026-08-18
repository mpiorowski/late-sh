//! Compact earned-award preview for profile overview.
//!
//! Profile awards are stored permanently, but the overview intentionally shows
//! only a short preview so the profile still reads quickly.

use late_core::models::profile_award::ProfileAward;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::theme;

pub(crate) const PREVIEW_LIMIT: usize = 6;

pub(crate) fn preview_lines(awards: &[ProfileAward]) -> Vec<Line<'static>> {
    if awards.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let badge_style = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());

    let mut spans = Vec::new();
    for award in awards.iter().take(PREVIEW_LIMIT) {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            format!("[{} {}]", award.badge(), award.month_label()),
            badge_style,
        ));
    }

    let remaining = awards.len().saturating_sub(PREVIEW_LIMIT);
    if remaining > 0 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(format!("+{remaining} more"), dim));
    }

    lines.push(Line::from(spans));
    lines
}

/// The full badge guide: what each code means, how it is earned, and whether
/// it pays chips. Lives here because the badges themselves are `ProfileAward`
/// data (`late_core::models::profile_award`); rendered on the Leaderboards
/// page (`leaderboard::ui::draw_detail`), which has the room a one-line
/// profile legend never did.
pub(crate) fn guide_lines() -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let heading = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let code = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT());

    let mut lines = vec![
        Line::from(Span::styled(
            "Every code below also shows in your profile's Badges list and in your chat username stack.",
            dim,
        )),
        Line::from(""),
        Line::from(Span::styled("Monthly awards", heading)),
        Line::from(Span::styled(
            "Snapshotted at month end from last month's totals on the boards to the left. Top 3 only, \
             rank digit 1-3 (AW1 is that month's #1). Prestige only, no chips of their own.",
            dim,
        )),
        Line::from(""),
    ];
    for (item_code, name, source) in [
        (
            "CHIP",
            "Top Chips",
            "last month's net chip earnings (Top Chips board, shop spend ignored)",
        ),
        (
            "AW",
            "Arcade Wins",
            "last month's daily-puzzle points (Arcade Wins board)",
        ),
        ("LA", "Lateris", "best Tetris score last month"),
        ("24#", "2048", "best 2048 score last month"),
        ("SN", "Snake", "best Snake score last month"),
    ] {
        lines.push(entry_line(item_code, name, source, code, text, dim));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("One-off feats", heading)));
    lines.push(Line::from(Span::styled(
        "Earned once per account and kept forever, no rank digit. Each pays a one-time chip \
         reward on first grant, except Lateania's two deepest crowns, which are prestige only.",
        dim,
    )));
    lines.push(Line::from(""));
    for (item_code, name, source) in [
        (
            "LMG",
            "Lateania Archdemon",
            "slay the Archdemon Mal'gareth (pays chips)",
        ),
        (
            "LKN",
            "Lateania Frontier King",
            "slay the King Who Was Promised Nothing (pays chips)",
        ),
        (
            "LYS",
            "Lateania Sundering Deep",
            "slay Yssgar, the Sundering Deep (no chips, badge only)",
        ),
        (
            "LKA",
            "Kaethyr Ascendant",
            "slay Kaethyr Ascendant in Kaelmyr (no chips, badge only)",
        ),
        (
            "NHA",
            "NetHack Amulet",
            "pick up the Amulet of Yendor (pays chips)",
        ),
        (
            "NHY",
            "NetHack Ascension",
            "ascend to demigodhood (pays chips)",
        ),
        (
            "DCO",
            "DCSS Orb of Zot",
            "pick up the Orb of Zot (pays chips)",
        ),
        (
            "DCW",
            "DCSS Escape",
            "escape the dungeon with the Orb (pays chips)",
        ),
        (
            "BRE",
            "Brogue Escape",
            "escape the Dungeons of Doom (pays chips)",
        ),
        (
            "BRM",
            "Brogue Mastery",
            "the Dungeons of Doom's super-victory (pays chips)",
        ),
        (
            "GDS",
            "Green Dragon Slayer",
            "slay the green dragon, first kill only (pays chips)",
        ),
    ] {
        lines.push(entry_line(item_code, name, source, code, text, dim));
    }

    lines
}

fn entry_line(
    item_code: &'static str,
    name: &'static str,
    source: &'static str,
    code_style: Style,
    name_style: Style,
    dim_style: Style,
) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{item_code:<4}"), code_style),
        Span::styled(format!(" {name} "), name_style),
        Span::styled(format!("— {source}"), dim_style),
    ])
}
