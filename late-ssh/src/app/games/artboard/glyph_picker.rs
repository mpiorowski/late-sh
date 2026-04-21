//! Glyph picker modal for the Artboard game.
//!
//! The picker is a self-contained overlay: tabs across emoji / unicode /
//! nerd-font, a TextArea-backed search, a scrollable list of matching
//! entries. When confirmed it yields the leading scalar of the selected
//! glyph, which Artboard paints at the cursor via its normal insert path.

use dartboard_picker_core::{
    self as picker, IconCatalogData, IconEntry, SectionSpec, adjust_scroll_offset,
    entry_at_selectable, flat_len, flat_to_selectable, selectable_to_flat, sources,
};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use std::cell::Cell;
use std::time::Instant;

use crate::app::common::theme;

pub const DOUBLE_CLICK_WINDOW_MS: u128 = 400;
pub const DEFAULT_VISIBLE_HEIGHT: usize = 13;

const COMMON_EMOJI: &[&str] = &[
    "👍",
    "👎",
    "🙏",
    "🙌",
    "🙋",
    "🐐",
    "😂",
    "🫡",
    "👀",
    "💀",
    "🎉",
    "🤝",
    "❤\u{fe0f}",
    "✅",
    "🔥",
    "⚡",
    "🚀",
    "🤔",
    "🫠",
    "🌱",
    "🤖",
    "🔧",
    "💎",
    "⭐",
    "🎯",
];

const COMMON_NERD_NAMES: &[&str] = &[
    "cod hubot",
    "md folder",
    "md git",
    "oct zap",
    "md chart bar",
    "cod credit card",
    "md timer",
    "md target",
    "md rocket launch",
    "seti code",
];

const COMMON_UNICODE: &[(&str, &str)] = &[
    ("●", "Black Circle"),
    ("◆", "Black Diamond"),
    ("★", "Black Star"),
    ("→", "Rightwards Arrow"),
    ("│", "Box Drawings Light Vertical"),
    ("■", "Black Square"),
    ("▲", "Black Up-Pointing Triangle"),
    ("○", "White Circle"),
    ("✦", "Black Four Pointed Star"),
    ("⟩", "Mathematical Right Angle Bracket"),
    ("·", "Middle Dot"),
    ("»", "Right-Pointing Double Angle Quotation Mark"),
    ("✓", "Check Mark"),
    ("✗", "Ballot X"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphPickerTab {
    Emoji,
    Unicode,
    NerdFont,
}

impl GlyphPickerTab {
    pub const ALL: [GlyphPickerTab; 3] = [
        GlyphPickerTab::Emoji,
        GlyphPickerTab::Unicode,
        GlyphPickerTab::NerdFont,
    ];

    pub fn index(self) -> usize {
        match self {
            Self::Emoji => 0,
            Self::Unicode => 1,
            Self::NerdFont => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Emoji => "emoji",
            Self::Unicode => "unicode",
            Self::NerdFont => "nerd font",
        }
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let len = Self::ALL.len();
        Self::ALL[(self.index() + len - 1) % len]
    }
}

pub fn load_catalog() -> IconCatalogData {
    let emoji_tab = vec![
        SectionSpec::new("common emoji", sources::emoji_pick(COMMON_EMOJI)),
        SectionSpec::new("all emoji", sources::emoji_all()),
    ];
    let unicode_tab = vec![
        SectionSpec::new("box drawing", sources::unicode_range(0x2500..=0x259F)),
        SectionSpec::new("common", sources::unicode_pick(COMMON_UNICODE)),
        SectionSpec::new("all unicode", sources::unicode_all()),
    ];
    let nerd_tab = vec![
        SectionSpec::new("common", sources::nerd_pick(COMMON_NERD_NAMES)),
        SectionSpec::new("all nerd font", sources::nerd_all()),
    ];
    IconCatalogData::new(vec![emoji_tab, unicode_tab, nerd_tab])
}

pub struct State {
    pub tab: GlyphPickerTab,
    pub search: TextArea<'static>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub visible_height: Cell<usize>,
    pub list_inner: Cell<Rect>,
    pub tabs_inner: Cell<Rect>,
    pub last_click: Option<(Instant, usize)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tab: GlyphPickerTab::Emoji,
            search: new_search_textarea(),
            selected_index: 0,
            scroll_offset: 0,
            visible_height: Cell::new(DEFAULT_VISIBLE_HEIGHT),
            list_inner: Cell::new(Rect::new(0, 0, 0, 0)),
            tabs_inner: Cell::new(Rect::new(0, 0, 0, 0)),
            last_click: None,
        }
    }
}

impl State {
    pub fn search_str(&self) -> String {
        self.search.lines().join("")
    }

    pub fn reset_selection(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.last_click = None;
    }

    pub fn set_tab(&mut self, tab: GlyphPickerTab) {
        if self.tab != tab {
            self.tab = tab;
            self.reset_selection();
        }
    }

    pub fn next_tab(&mut self) {
        self.set_tab(self.tab.next());
    }

    pub fn prev_tab(&mut self) {
        self.set_tab(self.tab.prev());
    }

    pub fn search_insert_char(&mut self, ch: char) {
        self.search.insert_char(ch);
        self.reset_selection();
    }

    pub fn search_delete_char(&mut self) {
        self.search.delete_char();
        self.reset_selection();
    }

    pub fn search_delete_next_char(&mut self) {
        self.search.delete_next_char();
        self.reset_selection();
    }

    pub fn search_delete_word_left(&mut self) {
        self.search.delete_word();
        self.reset_selection();
    }

    pub fn search_delete_word_right(&mut self) {
        self.search.delete_next_word();
        self.reset_selection();
    }

    pub fn search_cursor_left(&mut self) {
        self.search.move_cursor(CursorMove::Back);
    }

    pub fn search_cursor_right(&mut self) {
        self.search.move_cursor(CursorMove::Forward);
    }

    pub fn search_cursor_word_left(&mut self) {
        self.search.move_cursor(CursorMove::WordBack);
    }

    pub fn search_cursor_word_right(&mut self) {
        self.search.move_cursor(CursorMove::WordForward);
    }

    pub fn search_cursor_home(&mut self) {
        self.search.move_cursor(CursorMove::Head);
    }

    pub fn search_cursor_end(&mut self) {
        self.search.move_cursor(CursorMove::End);
    }

    pub fn search_paste(&mut self) {
        self.search.paste();
        self.reset_selection();
    }

    pub fn search_undo(&mut self) {
        self.search.undo();
        self.reset_selection();
    }
}

fn new_search_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_cursor_style(
        Style::default()
            .fg(theme::BG_SELECTION())
            .bg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    );
    ta.set_style(Style::default().fg(theme::TEXT_BRIGHT()));
    ta.set_wrap_mode(WrapMode::None);
    ta
}

pub fn move_selection(state: &mut State, catalog: &IconCatalogData, delta: isize) {
    let sections = catalog.sections(state.tab.index(), &state.search_str());
    let max = picker::selectable_count(&sections);
    if max == 0 {
        return;
    }
    let cur = state.selected_index as isize;
    let next = cur.saturating_add(delta).clamp(0, (max - 1) as isize) as usize;
    state.selected_index = next;
    apply_scroll(state, catalog);
}

pub fn apply_scroll(state: &mut State, catalog: &IconCatalogData) {
    let sections = catalog.sections(state.tab.index(), &state.search_str());
    let flat_idx = selectable_to_flat(&sections, state.selected_index).unwrap_or(0);
    let visible = state.visible_height.get().max(1);
    state.scroll_offset = adjust_scroll_offset(state.scroll_offset, visible, flat_idx);
}

/// Resolve the current selection into the owned icon string. Artboard only
/// paints a single leading scalar, but the whole cluster is returned so the
/// caller can decide.
pub fn selected_glyph(state: &State, catalog: &IconCatalogData) -> Option<String> {
    let sections = catalog.sections(state.tab.index(), &state.search_str());
    let entry = entry_at_selectable(&sections, state.selected_index)?;
    Some(entry.icon.clone())
}

/// Left-click on the list area. Returns true if a double-click was detected
/// (caller should treat it as Enter).
pub fn click_list(state: &mut State, catalog: &IconCatalogData, x: u16, y: u16) -> bool {
    let list = state.list_inner.get();
    if list.height == 0 || y < list.y || y >= list.y + list.height || x < list.x {
        return false;
    }
    let offset_in_list = (y - list.y) as usize;
    let flat_idx = state.scroll_offset + offset_in_list;

    let sections = catalog.sections(state.tab.index(), &state.search_str());
    let Some(selectable_idx) = flat_to_selectable(&sections, flat_idx) else {
        return false;
    };

    let now = Instant::now();
    let is_double = match state.last_click {
        Some((prev, prev_idx)) => {
            prev_idx == selectable_idx
                && now.duration_since(prev).as_millis() <= DOUBLE_CLICK_WINDOW_MS
        }
        None => false,
    };

    state.selected_index = selectable_idx;
    state.last_click = if is_double {
        None
    } else {
        Some((now, selectable_idx))
    };
    apply_scroll(state, catalog);
    is_double
}

pub const TAB_STRIP_LEAD: u16 = 1;
pub const TAB_STRIP_GAP: u16 = 2;

fn tab_cell_width(label: &str) -> u16 {
    4 + label.chars().count() as u16
}

pub fn tab_at_x(tabs_inner: Rect, x: u16) -> Option<GlyphPickerTab> {
    if tabs_inner.width == 0 || x < tabs_inner.x {
        return None;
    }
    let rel = x - tabs_inner.x;
    if rel < TAB_STRIP_LEAD {
        return None;
    }
    let mut cursor = TAB_STRIP_LEAD;
    for (i, tab) in GlyphPickerTab::ALL.iter().enumerate() {
        let w = tab_cell_width(tab.label());
        let cell_end = cursor
            + w
            + if i + 1 < GlyphPickerTab::ALL.len() {
                TAB_STRIP_GAP
            } else {
                0
            };
        if rel < cell_end {
            return Some(*tab);
        }
        cursor = cell_end;
    }
    None
}

pub fn render(frame: &mut Frame, area: Rect, state: &State, catalog: &IconCatalogData) {
    let height = ((area.height as u32 * 70) / 100) as u16;
    let height = height.clamp(14, area.height);
    let width = 56u16.min(area.width);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .title(Span::styled(
            " Glyph Picker ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
        .title(
            Line::from(vec![
                Span::styled("esc", Style::default().fg(theme::AMBER_DIM())),
                Span::raw(" "),
                Span::styled("cancel ", Style::default().fg(theme::TEXT_DIM())),
            ])
            .right_aligned(),
        );
    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(inner);

    render_tabs(frame, layout[0], state);
    render_search(frame, layout[1], state);
    render_list(frame, layout[2], state, catalog);
    render_footer(frame, layout[3]);
}

fn render_tabs(frame: &mut Frame, area: Rect, state: &State) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    for (i, tab) in GlyphPickerTab::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default().fg(theme::TEXT_DIM())));
        }
        let selected = state.tab == *tab;
        let indicator = if selected { "•" } else { " " };
        let style = if selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(
            format!("[{indicator}] {}", tab.label()),
            style,
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_DIM()))
        .title(Span::styled(
            " glyph set ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    let inner = block.inner(area);
    state.tabs_inner.set(inner);
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_search(frame: &mut Frame, area: Rect, state: &State) {
    let prompt = Paragraph::new(Line::from(vec![
        Span::styled("  search ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_DIM())),
    ]));
    let split = Layout::horizontal([Constraint::Length(11), Constraint::Fill(1)]).split(area);
    frame.render_widget(prompt, split[0]);
    frame.render_widget(&state.search, split[1]);
}

fn render_list(frame: &mut Frame, area: Rect, state: &State, catalog: &IconCatalogData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_DIM()))
        .title(Span::styled(
            " glyphs ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_height = inner.height as usize;
    state.visible_height.set(visible_height.max(1));
    state.list_inner.set(inner);
    if visible_height == 0 {
        return;
    }

    let sections = catalog.sections(state.tab.index(), &state.search_str());
    let total_flat = flat_len(&sections);
    let selected_flat = selectable_to_flat(&sections, state.selected_index);
    let scroll = state.scroll_offset;
    let view_end = scroll + visible_height;

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut row = 0usize;
    'outer: for section in &sections {
        if row >= view_end {
            break;
        }
        if row >= scroll && row < view_end {
            lines.push(header_line(section.title));
            if lines.len() == visible_height {
                break 'outer;
            }
        }
        row += 1;
        let entries_len = section.entries.len();
        let entries_end = row + entries_len;
        let vis_start = scroll.max(row);
        let vis_end = view_end.min(entries_end);
        if vis_start < vis_end {
            for flat_row in vis_start..vis_end {
                let entry_idx = flat_row - row;
                let Some(entry) = section.entries.get(entry_idx) else {
                    break;
                };
                let is_selected = Some(flat_row) == selected_flat;
                lines.push(entry_line(entry, is_selected, inner.width));
                if lines.len() == visible_height {
                    break 'outer;
                }
            }
        }
        row = entries_end;
    }

    frame.render_widget(Paragraph::new(lines), inner);

    if total_flat > 0 {
        let total_pages = total_flat.div_ceil(visible_height);
        let current_page = scroll / visible_height + 1;
        let counter = format!(" page {}/{} ", current_page, total_pages);
        let counter_width = counter.len() as u16;
        let counter_area = Rect {
            x: area.x + area.width.saturating_sub(counter_width + 1),
            y: area.y + area.height - 1,
            width: counter_width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                counter,
                Style::default().fg(theme::TEXT_DIM()),
            )),
            counter_area,
        );
    }
}

fn header_line(title: &str) -> Line<'static> {
    let dashes = "─".repeat(3);
    Line::from(vec![
        Span::styled(
            format!("{dashes}─{dashes} "),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {dashes}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn entry_line(entry: &IconEntry, is_selected: bool, width: u16) -> Line<'static> {
    let icon = entry.icon.clone();
    let name = entry.name.clone();
    if is_selected {
        let pad = (width as usize).saturating_sub(icon.chars().count() + name.chars().count() + 3);
        Line::from(vec![
            Span::styled(
                format!(" {icon} "),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .bg(theme::BG_HIGHLIGHT()),
            ),
            Span::styled(
                name,
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .bg(theme::BG_HIGHLIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ".repeat(pad), Style::default().bg(theme::BG_HIGHLIGHT())),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                format!(" {icon} "),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
            Span::styled(name, Style::default().fg(theme::TEXT())),
        ])
    }
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default().fg(theme::AMBER_DIM());
    let hint = Line::from(vec![
        Span::raw("  "),
        Span::styled("\u{23CE}", key),
        Span::styled(" insert   ", dim),
        Span::styled("Alt+\u{23CE}", key),
        Span::styled(" keep open   ", dim),
        Span::styled("Tab", key),
        Span::styled(" next set   ", dim),
        Span::styled("Esc", key),
        Span::styled(" close", dim),
    ]);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(hint), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center);
    let [vert] = vertical.areas(area);
    let [rect] = horizontal.areas(vert);
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_three_tabs() {
        let catalog = load_catalog();
        assert_eq!(catalog.tab_count(), 3);
    }

    #[test]
    fn common_emoji_resolves_multi_codepoint_heart() {
        let catalog = load_catalog();
        let sections = catalog.sections(GlyphPickerTab::Emoji.index(), "");
        let common = sections.iter().find(|s| s.title == "common emoji").unwrap();
        let mut found_heart = false;
        for i in 0..common.entries.len() {
            if common.entries.get(i).unwrap().icon == "❤\u{fe0f}" {
                found_heart = true;
                break;
            }
        }
        assert!(found_heart, "heart-fe0f missing from common emoji");
    }

    #[test]
    fn tab_navigation_cycles_forward_and_back() {
        let mut state = State::default();
        state.next_tab();
        assert_eq!(state.tab, GlyphPickerTab::Unicode);
        state.next_tab();
        assert_eq!(state.tab, GlyphPickerTab::NerdFont);
        state.next_tab();
        assert_eq!(state.tab, GlyphPickerTab::Emoji);
        state.prev_tab();
        assert_eq!(state.tab, GlyphPickerTab::NerdFont);
    }

    #[test]
    fn move_selection_clamps_within_section_bounds() {
        let catalog = load_catalog();
        let mut state = State::default();
        move_selection(&mut state, &catalog, 5);
        assert_eq!(state.selected_index, 5);
        move_selection(&mut state, &catalog, -100);
        assert_eq!(state.selected_index, 0);
    }
}
