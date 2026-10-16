//! The target table: every territory you could act on, as a list rather than
//! a map. Nearest first, so your borders are at the top and the far-flung
//! gambles sink to the bottom, each with the hops it would cost, who holds
//! it, and the odds the engine would actually roll.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

use super::map_ui::compact;
use super::resolver::ActionRejected;
use super::state::{RealmState, TargetRow, points_left};

/// Which columns fit, and how wide the name gets. A table cannot wrap
/// without losing its alignment, so instead it sheds what it can afford to:
/// population first, then who holds it. The odds and the distance are the
/// two things the table exists to show, so they are never dropped.
#[derive(Clone, Copy)]
struct Columns {
    name: usize,
    holder: Option<usize>,
    pop: bool,
    /// Distance and odds are measured from the viewer's own land, so they
    /// mean nothing to someone who is only watching.
    hops: bool,
    odds: bool,
    /// Walls, when there is room for them.
    walls: bool,
}

impl Columns {
    /// Fixed costs: marker (2) + glyph (2) + hops (6) + odds (7).
    const CHROME: usize = 17;
    const HOLDER: usize = 14;
    const POP: usize = 10;
    const WALLS: usize = 7;

    fn fit(width: usize, acts: bool) -> Self {
        // A watcher's table drops the two player-relative columns, which
        // buys the rest of it room.
        let chrome = if acts { Self::CHROME } else { 4 };
        let full = chrome + Self::HOLDER + Self::POP + Self::WALLS;
        let base = Self {
            name: 8,
            holder: None,
            pop: false,
            hops: acts,
            odds: acts,
            walls: false,
        };
        if width >= full + 20 {
            Self {
                name: width - full,
                holder: Some(Self::HOLDER),
                pop: true,
                walls: true,
                ..base
            }
        } else if width >= chrome + Self::HOLDER + Self::WALLS + 14 {
            Self {
                name: width - chrome - Self::HOLDER - Self::WALLS,
                holder: Some(Self::HOLDER),
                pop: false,
                walls: true,
                ..base
            }
        } else if width >= chrome + Self::HOLDER + 14 {
            Self {
                name: width - chrome - Self::HOLDER,
                holder: Some(Self::HOLDER),
                pop: false,
                ..base
            }
        } else {
            Self {
                name: width.saturating_sub(chrome).max(8),
                ..base
            }
        }
    }
}

pub fn draw_targets_view(frame: &mut Frame, area: Rect, realm: &RealmState) {
    let Some(board) = realm.board.as_ref() else {
        return;
    };
    if board.detail.is_none() {
        return;
    }
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let bright = Style::default().fg(theme::AMBER());

    let rows = realm.target_rows();
    let mut lines: Vec<Line> = Vec::new();

    let role = realm.viewer_role();
    let mine = rows.iter().filter(|r| r.mine).count();
    let title = if role.acts() {
        let points = points_left(board, realm.user_id)
            .map(|(left, cap)| format!("{left}/{cap} points left today"))
            .unwrap_or_default();
        format!("{} yours · {} to take · {points}", mine, rows.len() - mine)
    } else {
        // Nothing here is "to take" for someone who cannot act.
        format!("{} · {} territories", role.label(), rows.len())
    };
    lines.push(Line::from(Span::styled(
        title,
        bright.add_modifier(Modifier::BOLD),
    )));
    let columns = Columns::fit(area.width as usize, role.acts());
    let mut header = format!("{:<4}{:<width$}", "", "territory", width = columns.name);
    if let Some(holder) = columns.holder {
        header.push_str(&format!("{:<holder$}", "held by", holder = holder));
    }
    if columns.hops {
        header.push_str(&format!("{:>6}", "reach"));
    }
    if columns.walls {
        header.push_str(&format!("{:>7}", "walls"));
    }
    if columns.pop {
        header.push_str(&format!("{:>10}", "pop"));
    }
    if columns.odds {
        header.push_str(&format!("{:>7}", "odds"));
    }
    lines.push(Line::from(Span::styled(header, faint)));

    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "nothing here — the map is yours, or the game is over",
            dim,
        )));
    }

    // Keep the cursor row on screen: the table scrolls as a whole.
    let body_height = area.height.saturating_sub(3) as usize;
    let cursor = board.targets_cursor.min(rows.len().saturating_sub(1));
    let start = cursor.saturating_sub(body_height / 2).min(
        rows.len()
            .saturating_sub(body_height.max(1))
            .min(rows.len()),
    );
    for (idx, row) in rows.iter().enumerate().skip(start).take(body_height) {
        lines.push(target_line(
            row,
            idx == cursor,
            columns,
            area.width as usize,
        ));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// One row of the table. `width` is the table's, so the selected row's fill
/// runs to the edge: a highlight that stops at the last column reads as
/// "these characters" rather than "this row", and the row is what is
/// selected.
fn target_line(row: &TargetRow, selected: bool, columns: Columns, width: usize) -> Line<'static> {
    let marker = if selected { "▸ " } else { "  " };
    let name_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else if row.mine {
        Style::default().fg(theme::AMBER_DIM())
    } else {
        Style::default().fg(theme::TEXT())
    };
    // Hops carry the whole distance rule, so they are coloured like a
    // warning once they stop being a border move.
    // The route cost is the whole distance rule in one number, so it is
    // coloured like a warning once it stops being a border move.
    let hop_style = if row.reach.cost <= 1.0001 {
        Style::default().fg(theme::TEXT_DIM())
    } else if row.reach.cost <= 3.0 {
        Style::default().fg(theme::AMBER_DIM())
    } else {
        Style::default().fg(theme::ERROR())
    };
    let hops = if row.mine {
        format!("{:>6}", "—")
    } else if row.reach.by_sea() {
        // `~` for the water it crossed.
        format!("{:>5}~", format!("{:.1}", row.reach.cost))
    } else {
        format!("{:>6}", format!("{:.1}", row.reach.cost))
    };
    let glyph = if row.mine {
        // Your own ground: the action here is to dig in.
        "⛨"
    } else if row.attack {
        "⚔"
    } else {
        "·"
    };
    // The last column is the odds, unless a rule is standing in the way.
    let (odds_text, odds_style) = match (&row.locked, row.probability) {
        (Some(locked), _) => (
            format!("{:>7}", lock_label(locked)),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        (None, Some(p)) => {
            let odds = (p * 100.0).round() as i64;
            let style = if odds >= 50 {
                Style::default().fg(theme::SUCCESS())
            } else if odds >= 15 {
                Style::default().fg(theme::AMBER())
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            (format!("{odds:>6}%"), style)
        }
        (None, None) if row.fort_level >= 5 => (
            format!("{:>7}", "walled"),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        // `f` digs here; Enter shows it on the map.
        (None, None) => (
            format!("{:>7}", "f digs"),
            Style::default().fg(theme::AMBER_DIM()),
        ),
    };

    let mut spans = vec![
        Span::styled(marker, Style::default().fg(theme::AMBER())),
        Span::styled(format!("{glyph} "), hop_style),
        Span::styled(
            format!(
                "{:<width$}",
                truncate(&row.name, columns.name),
                width = columns.name
            ),
            name_style,
        ),
    ];
    if let Some(width) = columns.holder {
        spans.push(match (&row.owner_name, row.color) {
            _ if row.mine => Span::styled(
                format!("{:<width$}", "you", width = width),
                Style::default().fg(theme::AMBER()),
            ),
            (Some(name), Some((r, g, b))) => Span::styled(
                format!("{:<width$}", truncate(name, width), width = width),
                Style::default().fg(Color::Rgb(r, g, b)),
            ),
            _ => Span::styled(
                format!("{:<width$}", "free", width = width),
                Style::default().fg(theme::SUCCESS()),
            ),
        });
    }
    if columns.hops {
        spans.push(Span::styled(hops, hop_style));
    }
    if columns.walls {
        // Bars rather than a number: at a glance you want "how solid", not
        // arithmetic.
        let level = row.fort_level;
        let bars = super::map_ui::fort_bar(level, 5);
        spans.push(Span::styled(
            format!("{bars:>7}"),
            Style::default().fg(if level > 1 {
                theme::AMBER_DIM()
            } else {
                theme::TEXT_FAINT()
            }),
        ));
    }
    if columns.pop {
        spans.push(Span::styled(
            format!("{:>10}", compact(row.population)),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if columns.odds {
        spans.push(Span::styled(odds_text, odds_style));
    }
    pad_to(&mut spans, width);
    Line::from(spans).style(theme::row_style(selected))
}

/// Fill the rest of the row with blanks, so a row style covers the row rather
/// than the words in it.
fn pad_to(spans: &mut Vec<Span<'static>>, width: usize) {
    use unicode_width::UnicodeWidthStr;
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    if used < width {
        spans.push(Span::raw(" ".repeat(width - used)));
    }
}

/// Short why-not for a locked attack, in the odds column.
fn lock_label(locked: &ActionRejected) -> String {
    match locked {
        ActionRejected::OutOfRange => "too far".to_string(),
        ActionRejected::NoRoute => "no route".to_string(),
        ActionRejected::AttacksLocked(days) => format!("war {days}d"),
        ActionRejected::DistantAttacksLocked(days) => format!("near {days}d"),
        _ => "locked".to_string(),
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_sheds_columns_before_it_overflows() {
        // A wide pane shows everything.
        let wide = Columns::fit(110, true);
        assert!(wide.pop && wide.holder.is_some() && wide.walls);
        assert!(wide.name >= 20);

        // Tighter: population goes first, because the odds and the distance
        // are what the table is for.
        let medium = Columns::fit(50, true);
        assert!(!medium.pop && medium.holder.is_some());

        // Tighter still: who holds it goes too.
        let narrow = Columns::fit(30, true);
        assert!(!narrow.pop && narrow.holder.is_none());
        assert!(narrow.name >= 8, "a name column has to stay readable");

        // And no layout ever claims more room than it has.
        for width in 20..=120usize {
            let cols = Columns::fit(width, true);
            let used = Columns::CHROME
                + cols.name
                + cols.holder.unwrap_or(0)
                + if cols.pop { Columns::POP } else { 0 }
                + if cols.walls { Columns::WALLS } else { 0 };
            assert!(
                used <= width.max(Columns::CHROME + 8),
                "at {width} columns the table wants {used}"
            );
        }
    }
}

#[cfg(test)]
#[path = "targets_ui_test.rs"]
mod targets_ui_test;
