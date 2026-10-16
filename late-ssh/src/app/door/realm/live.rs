//! The realm on the live strip (`app/live/`): the last thing that happened
//! on the standing world, as the row's own `news` records it. A world rose,
//! somebody marched in, somebody took a country from somebody, somebody
//! fell, the world fell. The stamp is the news's own instant, written by the
//! write that made it, so every replica's strip shows the same moment at
//! the same time, and a claim on empty land, which is not news, never
//! knocks a real moment off the strip.

use chrono::{DateTime, Utc};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    live::{
        pick::{LiveCandidate, LiveSource},
        ui::{HintPart, PICTURE_COLS, PICTURE_ROWS, StripBody, key_hint_spans, truncate_chars},
    },
};

use super::resolver::{RealmNews, RealmNewsKind, RealmPlayerStatus};
use super::state::PLAYER_PALETTES;
use super::svc::{RealmGameItem, RealmSnapshot, countdown_label, muster_left};

/// The realm as the live strip paints it: the world and its last moment.
#[derive(Clone, Debug)]
pub struct RealmStripView {
    pub item: RealmGameItem,
    pub news: RealmNews,
    pub now: DateTime<Utc>,
}

/// The standing world while it has news, and the last finished one while
/// it is still listed (its news is its end). The strip's own horizon drops
/// a stamp that is old news.
pub(crate) fn candidates(snapshot: &RealmSnapshot) -> Vec<LiveCandidate> {
    snapshot
        .live()
        .into_iter()
        .chain(snapshot.last_finished())
        .filter_map(|item| {
            item.news.as_ref().map(|news| LiveCandidate {
                source: LiveSource::Realm(item.id),
                updated: news.at,
                aimed_at: None,
            })
        })
        .collect()
}

/// One world as the strip paints it. `None` once it left the listing.
pub(crate) fn view(
    snapshot: &RealmSnapshot,
    game_id: Uuid,
    now: DateTime<Utc>,
) -> Option<RealmStripView> {
    snapshot
        .live()
        .into_iter()
        .chain(snapshot.last_finished())
        .find(|item| item.id == game_id)
        .and_then(|item| {
            item.news.as_ref().map(|news| RealmStripView {
                item: item.clone(),
                news: news.clone(),
                now,
            })
        })
}

pub(crate) fn body(budget: usize, strip: &RealmStripView) -> StripBody {
    StripBody {
        picture: picture(),
        words: word_rows(budget, strip),
        hint: key_hint_spans(
            budget,
            &[
                HintPart::Key("o"),
                HintPart::Text(" or click to enter the realm"),
            ],
        ),
        glow: glow(strip),
    }
}

/// Lit while the world is being fought over; a fallen world is a result.
pub(crate) fn glow(strip: &RealmStripView) -> bool {
    !matches!(strip.news.kind, RealmNewsKind::Ended { .. })
}

/// `realm season 7 · mat took France from odolha`, after the rule label.
pub(crate) fn compact_spans(rest: u16, strip: &RealmStripView) -> Vec<Span<'static>> {
    let rest = usize::from(rest);
    let lead = format!("realm {} · ", strip.item.name.to_lowercase());
    let lead = truncate_chars(&lead, rest);
    let left = rest.saturating_sub(lead.chars().count());
    vec![
        Span::styled(lead, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            truncate_chars(&headline(strip), left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

/// The moment in a sentence.
pub(crate) fn headline(strip: &RealmStripView) -> String {
    match &strip.news.kind {
        RealmNewsKind::Opened => match muster_left(strip.item.created, strip.now) {
            Some(left) => format!(
                "a new world rises, the gates open in {}",
                countdown_label(left)
            ),
            None => "a new world rises".to_string(),
        },
        RealmNewsKind::Joined {
            username, color, ..
        } => {
            let colour = PLAYER_PALETTES
                .get(usize::from(*color))
                .map(|p| p.name)
                .unwrap_or("a colour");
            format!("{username} marched in as {colour}")
        }
        RealmNewsKind::Took {
            username,
            from_username,
            target_name,
            ..
        } => format!("{username} took {target_name} from {from_username}"),
        RealmNewsKind::Fell {
            username,
            by_username,
            ..
        } => format!("{username} fell to {by_username}"),
        RealmNewsKind::Ended {
            winner_username, ..
        } => match winner_username {
            Some(winner) => format!("the world fell to {winner}"),
            None => "the world fell silent".to_string(),
        },
    }
}

/// The words beside the picture, one entry per picture row: the world, the
/// moment, and where the war stands.
fn word_rows(budget: usize, strip: &RealmStripView) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = (0..PICTURE_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = vec![Span::styled(
        truncate_chars(&format!("Realm · {}", strip.item.name), budget),
        Style::default()
            .fg(theme::TEXT())
            .add_modifier(Modifier::BOLD),
    )];
    rows[2] = vec![Span::styled(
        truncate_chars(&headline(strip), budget),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )];
    let alive = strip
        .item
        .players
        .iter()
        .filter(|p| p.status == RealmPlayerStatus::Alive)
        .count();
    let standing = match (alive, strip.item.joinable) {
        (0, _) => "nobody on it yet, the gates are open".to_string(),
        (n, true) => format!(
            "{n} alive · {:.0}% claimed · gates open",
            strip.item.claimed * 100.0
        ),
        (n, false) => format!("{n} alive · {:.0}% claimed", strip.item.claimed * 100.0),
    };
    rows[3] = vec![Span::styled(
        truncate_chars(&standing, budget),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    rows
}

/// A scrap of coastline with two empires on it: the realm's picture, drawn
/// rather than built, because the strip must never build a world.
const WORLD: [&str; 5] = [
    "  ▄▀▀▄   ▄▄▀▀▀▀▄   ",
    " ▐▓▓▓▓▌ ▐░░░▓▓▓▓▌  ",
    "  ▀▄▓▓▀▄ ▀▄░░▓▄▀   ",
    "    ▀▀  ▄▀▀▄ ▀     ",
    "       ▐░░░▌  ▄▀▄  ",
];

fn picture() -> Vec<Line<'static>> {
    let inner = usize::from(PICTURE_COLS) - 2;
    let frame = Style::default().fg(theme::BORDER_DIM());
    let edge = |left: &str, right: &str| {
        Line::from(Span::styled(
            format!("{left}{}{right}", "\u{2500}".repeat(inner)),
            frame,
        ))
    };
    let mut lines = vec![edge("\u{256d}", "\u{256e}")];
    for row in WORLD {
        let mut spans = vec![Span::styled("\u{2502}".to_string(), frame)];
        spans.extend(
            row.chars()
                .map(|ch| Span::styled(ch.to_string(), glyph(ch))),
        );
        spans.push(Span::styled("\u{2502}".to_string(), frame));
        lines.push(Line::from(spans));
    }
    lines.push(edge("\u{2570}", "\u{256f}"));
    lines
}

fn glyph(ch: char) -> Style {
    match ch {
        '\u{2593}' => Style::default().fg(theme::AMBER()),
        '\u{2591}' => Style::default().fg(theme::CHAT_AUTHOR()),
        ' ' => Style::default(),
        _ => Style::default().fg(theme::TEXT_FAINT()),
    }
}
