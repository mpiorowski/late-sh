//! Rendering for the gallery side of the Artboard page: the rail, the
//! listing pane (list plus preview), the full-frame piece, the hang modal,
//! the framing bar, and the splash piece.

use dartboard_core::Canvas;
use late_core::models::artboard_piece::PIECE_TITLE_MAX_CHARS;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::artboard::state::State;
use crate::app::artboard::svc::ArtboardSnapshotKind;
use crate::app::artboard::ui::render_piece_canvas;
use crate::app::common::theme;

use super::frame::Credit;
use super::state::{Focus, GallerySection, GalleryState, HangFlow, RailRow};
use super::svc::{GalleryPiece, applause_label};

/// The rail's width, gap included.
pub const RAIL_WIDTH: u16 = 20;
/// Lines above the entries when the rail shows an archive list: the kind's
/// heading and the key hint.
pub const ARCHIVE_LIST_HEADER_LINES: usize = 2;
/// Smallest listing pane the gallery draws at all (list only).
const MIN_PANE_WIDTH: u16 = 24;
const LIST_MIN_WIDTH: u16 = 28;
/// From this width on the pane fits a preview beside the list; under it
/// the list takes the pane and Enter is how a piece is seen.
const PREVIEW_MIN_PANE_WIDTH: u16 = 56;

/// One line of the rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailLine {
    Row(RailRow),
    Heading(&'static str),
    Blank,
}

/// The rail's lines top to bottom. Shared by the draw path and the hit
/// test so a click lands on what it sees.
pub fn rail_layout(rows: &[RailRow]) -> Vec<RailLine> {
    // A breathing row first, like every page rail; then one heading per
    // group.
    let mut lines = vec![RailLine::Blank];
    for row in rows {
        match row {
            RailRow::Board => lines.push(RailLine::Row(*row)),
            RailRow::Gallery(GallerySection::ThisMonth) => {
                lines.push(RailLine::Blank);
                lines.push(RailLine::Heading("GALLERY"));
                lines.push(RailLine::Row(*row));
            }
            RailRow::Gallery(_) => lines.push(RailLine::Row(*row)),
            RailRow::Hang => {
                lines.push(RailLine::Blank);
                lines.push(RailLine::Row(*row));
            }
            RailRow::Archive(ArtboardSnapshotKind::Daily) => {
                lines.push(RailLine::Blank);
                lines.push(RailLine::Heading("ARCHIVES"));
                lines.push(RailLine::Row(*row));
            }
            RailRow::Archive(_) => lines.push(RailLine::Row(*row)),
        }
    }
    lines
}

pub fn rail_row_for_line(rows: &[RailRow], line: usize) -> Option<RailRow> {
    match rail_layout(rows).get(line) {
        Some(RailLine::Row(row)) => Some(*row),
        Some(RailLine::Heading(_)) | Some(RailLine::Blank) | None => None,
    }
}

/// The rail in its column, with the full-height rule on its right edge,
/// like the room rail and the Games hub. `area` is `RAIL_WIDTH + 1` wide.
pub fn draw_rail(frame: &mut Frame, area: Rect, state: &State) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let area = inner;
    let gallery = state.gallery();
    gallery.set_rail_area(area);
    if let Some(kind) = state.browsed_archive_kind() {
        draw_archive_list(frame, area, state, kind);
        return;
    }
    let rows = gallery.rows();
    let selected = gallery.selected_row();
    let rail_focused = matches!(gallery.focus(), Focus::Rail);

    let mut lines = Vec::new();
    for entry in rail_layout(&rows) {
        let row = match entry {
            RailLine::Row(row) => row,
            RailLine::Heading(text) => {
                lines.push(Line::from(Span::styled(
                    format!(" {text}"),
                    heading_style(),
                )));
                continue;
            }
            RailLine::Blank => {
                lines.push(Line::from(""));
                continue;
            }
        };
        let is_selected = row == selected;
        let style = rail_row_style(is_selected, rail_focused);
        let marker = if is_selected { ">" } else { " " };
        let label = match row {
            RailRow::Board => {
                let tail = if state.is_archive_view_active() {
                    "archive".to_string()
                } else {
                    format!("{} on", state.snapshot.peers.len().max(1))
                };
                rail_label(marker, "Board", &tail)
            }
            RailRow::Gallery(section) => {
                let tail = gallery
                    .section_count(section)
                    .map(|count| count.to_string())
                    .unwrap_or_default();
                rail_label(marker, section.label(), &tail)
            }
            RailRow::Hang => rail_label("+", "Hang a piece", ""),
            RailRow::Archive(kind) => {
                let tail = state
                    .archive_count(kind)
                    .map(|count| count.to_string())
                    .unwrap_or_default();
                rail_label(marker, kind.title(), &tail)
            }
        };
        lines.push(Line::from(Span::styled(label, style)));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The rail as one archive kind's key list. The board behind it shows the
/// key under the cursor as soon as it loads.
fn draw_archive_list(frame: &mut Frame, area: Rect, state: &State, kind: ArtboardSnapshotKind) {
    let visible = (area.height as usize).saturating_sub(ARCHIVE_LIST_HEADER_LINES);
    state.set_archive_visible_height(visible);
    let entries = state.archive_entries(kind);
    let selected = state.archive_selected(kind);
    let scroll = state.archive_scroll(kind);
    let active_key = state
        .active_archive_snapshot()
        .map(|snapshot| snapshot.board_key.as_str());
    let loading_key = state.archive_loading_key();

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {}", kind.title().to_ascii_uppercase()),
            heading_style(),
        )),
        Line::from(Span::styled(
            " j/k travel · Esc back",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    if state.archive_loading(kind) && entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  loading…",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else if let Some(error) = state.archive_error(kind) {
        lines.push(Line::from(Span::styled(
            format!("  {error}"),
            Style::default().fg(theme::AMBER_DIM()),
        )));
    } else if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  no {} archive yet", kind.label()),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (offset, entry) in entries.iter().skip(scroll).take(visible).enumerate() {
        let index = scroll + offset;
        let is_selected = index == selected;
        let marker = if is_selected { ">" } else { " " };
        let tail = if loading_key == Some(entry.board_key.as_str()) {
            "…"
        } else if active_key == Some(entry.board_key.as_str()) {
            "•"
        } else {
            ""
        };
        lines.push(Line::from(Span::styled(
            rail_label(marker, &entry.label, tail),
            rail_row_style(is_selected, true),
        )));
    }
    if let Some(error) = state.archive_load_error()
        && lines.len() < area.height as usize
    {
        lines.push(Line::from(Span::styled(
            format!("  {error}"),
            Style::default().fg(theme::AMBER_DIM()),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn heading_style() -> Style {
    Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD)
}

fn rail_row_style(is_selected: bool, focused: bool) -> Style {
    if is_selected && focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_HIGHLIGHT())
            .add_modifier(Modifier::BOLD)
    } else if is_selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    }
}

fn rail_label(marker: &str, label: &str, tail: &str) -> String {
    let width = (RAIL_WIDTH as usize).saturating_sub(4);
    let label: String = label
        .chars()
        .take(width.saturating_sub(tail.len() + 1))
        .collect();
    let pad = width.saturating_sub(label.chars().count() + tail.len());
    format!(" {marker} {label}{}{tail}", " ".repeat(pad))
}

/// The listing pane: list on the left, the selected piece on the right.
pub fn draw_gallery_pane(frame: &mut Frame, area: Rect, state: &State) {
    let gallery = state.gallery();
    let Some(section) = gallery.viewed_section() else {
        return;
    };
    if area.width < MIN_PANE_WIDTH || area.height < 6 {
        crate::app::common::primitives::draw_too_small(frame, area, "Gallery", MIN_PANE_WIDTH, 6);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // hint
        Constraint::Length(1), // notice
        Constraint::Min(0),    // body
    ])
    .split(area);
    frame.render_widget(Paragraph::new(section_heading(section.label())), rows[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(section.hint(), Style::default().fg(theme::TEXT_DIM())),
        ])),
        rows[1],
    );
    frame.render_widget(Paragraph::new(notice_line(gallery)), rows[2]);

    if area.width < PREVIEW_MIN_PANE_WIDTH {
        let body = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(rows[3]);
        draw_list(frame, body[0], gallery, section);
        frame.render_widget(
            Paragraph::new(key_hint_line(&[("Enter", "view"), ("v", "applaud")])),
            body[1],
        );
        return;
    }
    let list_width = (area.width / 5 * 2).clamp(LIST_MIN_WIDTH, area.width.saturating_sub(20));
    let columns = Layout::horizontal([
        Constraint::Length(list_width),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(rows[3]);
    draw_list(frame, columns[0], gallery, section);
    if let Some(piece) = gallery.selected_piece() {
        draw_preview(frame, columns[2], piece, gallery.focus());
    }
}

fn draw_list(frame: &mut Frame, area: Rect, gallery: &GalleryState, section: GallerySection) {
    gallery.set_list_area(area, area.height as usize);
    let pieces = gallery.section_pieces(section);
    let selected = gallery.section_selected(section);
    let scroll = gallery.section_scroll(section);
    let list_focused = matches!(gallery.focus(), Focus::List | Focus::Piece);
    let mut lines = Vec::new();
    if gallery.section_loading(section) && pieces.is_empty() {
        lines.push(Line::from(Span::styled(
            "  loading…",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else if let Some(error) = gallery.section_error(section) {
        lines.push(Line::from(Span::styled(
            format!("  {error}"),
            Style::default().fg(theme::AMBER_DIM()),
        )));
    } else if pieces.is_empty() {
        lines.push(Line::from(Span::styled(
            match section {
                GallerySection::ThisMonth => "  nothing hung this month yet",
                GallerySection::Newest => "  nothing hung yet",
                GallerySection::HallOfFame => "  no month has a winner yet",
                GallerySection::Mine => "  you have not hung a piece yet",
            },
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    let visible = area.height as usize;
    for (offset, piece) in pieces.iter().skip(scroll).take(visible).enumerate() {
        let index = scroll + offset;
        let is_selected = index == selected;
        let marker = if is_selected { ">" } else { " " };
        let clap = if piece.applauded_by_viewer {
            "✓"
        } else {
            " "
        };
        let title_width = (area.width as usize).saturating_sub(12);
        let title: String = piece.title.chars().take(title_width).collect();
        let text = format!(" {marker} {:>3} {clap} {title}", piece.applause);
        let style = if is_selected && list_focused {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_preview(frame: &mut Frame, area: Rect, piece: &GalleryPiece, focus: Focus) {
    if area.width < 10 || area.height < 4 {
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1), // caption
        Constraint::Length(1), // credits
        Constraint::Min(1),    // canvas
        Constraint::Length(1), // keys
    ])
    .split(area);
    frame.render_widget(Paragraph::new(caption_line(piece)), rows[0]);
    frame.render_widget(
        Paragraph::new(credits_line(
            &piece.credits,
            &piece.username,
            piece.period_month,
        )),
        rows[1],
    );
    draw_piece_canvas(frame, rows[2], piece);
    let keys: &[(&str, &str)] = match focus {
        Focus::List => &[("v", "applaud"), ("Enter", "full frame"), ("Esc", "rail")],
        Focus::Rail => &[("Enter/→", "browse")],
        Focus::Canvas | Focus::Piece | Focus::Archive => &[],
    };
    frame.render_widget(Paragraph::new(key_hint_line(keys)), rows[3]);
}

/// One piece, full frame, over the whole detail pane.
pub fn draw_piece_view(frame: &mut Frame, area: Rect, state: &State) {
    let gallery = state.gallery();
    let Some(piece) = gallery.selected_piece() else {
        return;
    };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(Paragraph::new(caption_line(piece)), rows[0]);
    frame.render_widget(
        Paragraph::new(credits_line(
            &piece.credits,
            &piece.username,
            piece.period_month,
        )),
        rows[1],
    );
    frame.render_widget(Paragraph::new(notice_line(gallery)), rows[2]);
    draw_piece_canvas(frame, rows[3], piece);
    // The id is what `/mod artboard remove <id-prefix>` takes, and this is
    // the only place it is shown; without it a mod has nothing to type.
    // The first two groups (12 hex digits) are enough to be unique and
    // short enough to copy by eye.
    let mut keys = key_hint_line(&[("v", "applaud"), ("j/k", "next/prev"), ("Esc", "back")]);
    keys.spans.push(Span::styled(
        format!("   id {}", piece_id_prefix(piece.id)),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    frame.render_widget(Paragraph::new(keys), rows[4]);
}

/// The part of a piece id the full-frame view prints for mods: the first
/// two groups, `0192abcd-1234`, which is over
/// [`late_core::models::artboard_piece::PIECE_ID_PREFIX_MIN_CHARS`].
pub fn piece_id_prefix(id: uuid::Uuid) -> String {
    id.to_string().chars().take(13).collect()
}

/// The piece's canvas, top-left aligned inside `area`, cropped to fit with a
/// note when it does not.
pub fn draw_piece_canvas(frame: &mut Frame, area: Rect, piece: &GalleryPiece) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let inner = area.inner(Margin::new(1, 0));
    let width = (piece.width as u16).min(inner.width);
    let height = (piece.height as u16).min(inner.height);
    let target = Rect::new(inner.x, inner.y, width, height);
    render_piece_canvas(frame.buffer_mut(), target, &piece.canvas);
    if (piece.width as u16) > inner.width || (piece.height as u16) > inner.height {
        let note = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(
                    "  (cropped: the piece is {}x{}, the pane {}x{})",
                    piece.width, piece.height, inner.width, inner.height
                ),
                Style::default().fg(theme::TEXT_FAINT()),
            ))),
            note,
        );
    }
}

/// The splash: last month's winner over the door, if it fits under the
/// caption. Returns false when the terminal is too small for it, so the
/// caller keeps the coffee cup.
pub fn draw_splash_piece(frame: &mut Frame, area: Rect, piece: &GalleryPiece) -> bool {
    let needed_height = piece.height as u16 + 4;
    let needed_width = (piece.width as u16).max(40) + 2;
    if area.height < needed_height + 2 || area.width < needed_width {
        return false;
    }
    let block_height = needed_height;
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(block_height),
        Constraint::Fill(1),
    ])
    .split(area);
    let columns = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(needed_width),
        Constraint::Fill(1),
    ])
    .split(rows[1]);
    let block = columns[1];
    let canvas_area = Rect::new(
        block.x + (block.width.saturating_sub(piece.width as u16)) / 2,
        block.y,
        piece.width as u16,
        piece.height as u16,
    );
    render_piece_canvas(frame.buffer_mut(), canvas_area, &piece.canvas);
    let caption_y = block.y + piece.height as u16 + 1;
    let caption = Line::from(vec![
        Span::styled(
            format!("piece of {}: ", month_label(piece.period_month)),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(
            format!("\"{}\"", piece.title),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " by @{} · {}",
                piece.username,
                applause_label(piece.applause)
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(caption).centered(),
        Rect::new(block.x, caption_y, block.width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "hung in the Artboard gallery, page 4",
            Style::default().fg(theme::TEXT_FAINT()),
        )))
        .centered(),
        Rect::new(block.x, caption_y + 1, block.width, 1),
    );
    true
}

/// The one-line bar over the board while framing: the notice and the frame
/// size so far.
pub fn draw_framing_bar(frame: &mut Frame, canvas_area: Rect, state: &State) {
    if canvas_area.height == 0 {
        return;
    }
    let bar = Rect::new(
        canvas_area.x,
        canvas_area.bottom().saturating_sub(1),
        canvas_area.width,
        1,
    );
    let size = state
        .selection_view()
        .map(|selection| {
            let width = selection.anchor.x.abs_diff(selection.cursor.x) + 1;
            let height = selection.anchor.y.abs_diff(selection.cursor.y) + 1;
            format!(" frame {width}x{height} ")
        })
        .unwrap_or_else(|| " no frame yet ".to_string());
    let notice = state.gallery().notice().unwrap_or_default();
    frame.render_widget(Clear, bar);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                size,
                Style::default()
                    .fg(theme::BG_CANVAS())
                    .bg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {notice}"),
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .bg(theme::BG_HIGHLIGHT()),
            ),
        ]))
        .style(Style::default().bg(theme::BG_HIGHLIGHT())),
        bar,
    );
}

/// The naming modal: the crop, its numbers, and the title field.
pub fn draw_hang_modal(frame: &mut Frame, area: Rect, state: &State) {
    let gallery = state.gallery();
    let (framed, title, submitting) = match gallery.hang() {
        HangFlow::Confirm { framed, title } => (Some(framed.as_ref()), title.as_str(), false),
        HangFlow::Submitting => (None, "", true),
        HangFlow::Idle | HangFlow::Framing => return,
    };
    let piece_width = framed.map(|f| f.width as u16).unwrap_or(20);
    let piece_height = framed.map(|f| f.height as u16).unwrap_or(4);
    let width = (piece_width + 4).clamp(44, area.width.saturating_sub(2).max(44));
    let height = (piece_height + 9).min(area.height.saturating_sub(2).max(9));
    let popup = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width.min(area.width),
        height.min(area.height),
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Hang it in the gallery ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 6 {
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1), // numbers
        Constraint::Length(1), // credits
        Constraint::Min(1),    // preview
        Constraint::Length(1), // notice
        Constraint::Length(1), // title field
        Constraint::Length(1), // keys
    ])
    .split(inner.inner(Margin::new(1, 0)));

    if let Some(framed) = framed {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{}x{}", framed.width, framed.height),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                ),
                Span::styled(
                    format!(
                        " · {} glyphs · {}% yours",
                        framed.glyph_count, framed.own_share_percent
                    ),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ])),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(credits_line(
                &framed.credits,
                &state.snapshot.your_name,
                chrono::Utc::now().date_naive(),
            )),
            rows[1],
        );
        let preview_width = (framed.width as u16).min(rows[2].width);
        let preview_height = (framed.height as u16).min(rows[2].height);
        render_piece_canvas(
            frame.buffer_mut(),
            Rect::new(rows[2].x, rows[2].y, preview_width, preview_height),
            &framed.canvas,
        );
    }
    frame.render_widget(Paragraph::new(notice_line(gallery)), rows[3]);
    let field = if submitting {
        Line::from(Span::styled(
            "hanging…",
            Style::default().fg(theme::TEXT_DIM()),
        ))
    } else {
        Line::from(vec![
            Span::styled("Title: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(theme::AMBER())),
            Span::styled(
                format!("  ({}/{PIECE_TITLE_MAX_CHARS})", title.chars().count()),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(field), rows[4]);
    frame.render_widget(
        Paragraph::new(key_hint_line(&[("Enter", "hang"), ("Esc", "cancel")])),
        rows[5],
    );
}

fn caption_line(piece: &GalleryPiece) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("\"{}\"", piece.title),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" by @{}", piece.username),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled(
            format!(" · {}", applause_label(piece.applause)),
            Style::default().fg(if piece.applauded_by_viewer {
                theme::SUCCESS()
            } else {
                theme::TEXT_DIM()
            }),
        ),
        Span::styled(
            if piece.applauded_by_viewer {
                " (yours too)"
            } else {
                ""
            },
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

/// "with @bob 6%, @ann 3% · hung Sep 3" style credits: every hand but the
/// hanger's, then the month.
fn credits_line(credits: &[Credit], hanger: &str, month: chrono::NaiveDate) -> Line<'static> {
    let total: usize = credits
        .iter()
        .map(|credit| credit.glyphs)
        .sum::<usize>()
        .max(1);
    let others: Vec<String> = credits
        .iter()
        .filter(|credit| credit.username != hanger)
        .take(4)
        .map(|credit| format!("@{} {}%", credit.username, credit.glyphs * 100 / total))
        .collect();
    let mut spans = Vec::new();
    if others.is_empty() {
        spans.push(Span::styled(
            "all their own work",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    } else {
        spans.push(Span::styled(
            format!("with {}", others.join(", ")),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    spans.push(Span::styled(
        format!(" · {}", month_label(month)),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    Line::from(spans)
}

fn month_label(month: chrono::NaiveDate) -> String {
    month.format("%b %Y").to_string()
}

fn notice_line(gallery: &GalleryState) -> Line<'static> {
    match gallery.notice() {
        Some(notice) => Line::from(Span::styled(
            format!("  {notice}"),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::ITALIC),
        )),
        None => Line::from(""),
    }
}

fn section_heading(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {text}"),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

fn key_hint_line(keys: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    for (index, (key, desc)) in keys.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {desc}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

/// Lines of plain glyphs for a piece, for surfaces that draw text rather
/// than cells (The Late Edition).
pub fn piece_text_lines(canvas: &Canvas, width: usize, height: usize) -> Vec<String> {
    (0..height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < width {
                let pos = dartboard_core::Pos { x, y };
                match canvas.glyph_at(pos) {
                    Some(glyph) if glyph.pos == pos => {
                        line.push(glyph.ch);
                        x += glyph.width.max(1);
                    }
                    Some(_) => x += 1,
                    None => {
                        line.push(' ');
                        x += 1;
                    }
                }
            }
            line.trim_end().to_string()
        })
        .collect()
}
