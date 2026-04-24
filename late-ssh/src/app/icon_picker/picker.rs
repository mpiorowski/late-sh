use super::{
    DOUBLE_CLICK_WINDOW_MS, IconPickerState, IconPickerTab,
    catalog::{IconCatalogData, IconEntry, SectionView},
};
use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use std::time::Instant;

/// Total number of selectable (non-header) entries across all sections.
pub fn selectable_count(sections: &[SectionView<'_>]) -> usize {
    sections.iter().map(|section| section.entries.len()).sum()
}

/// Total number of flat rows (1 header + N entries per section).
pub fn flat_len(sections: &[SectionView<'_>]) -> usize {
    sections
        .iter()
        .map(|section| section.entries.len() + 1)
        .sum()
}

/// Map a selectable index -> flat row index.
pub fn selectable_to_flat(sections: &[SectionView<'_>], selectable: usize) -> Option<usize> {
    let mut flat = 0;
    let mut remaining = selectable;
    for section in sections {
        flat += 1;
        let len = section.entries.len();
        if remaining < len {
            return Some(flat + remaining);
        }
        remaining -= len;
        flat += len;
    }
    None
}

/// Map a flat row index -> selectable index. Returns None for header rows.
pub fn flat_to_selectable(sections: &[SectionView<'_>], flat_idx: usize) -> Option<usize> {
    let mut flat = 0;
    let mut selectable = 0;
    for section in sections {
        if flat_idx == flat {
            return None;
        }
        flat += 1;
        let len = section.entries.len();
        if flat_idx < flat + len {
            return Some(selectable + (flat_idx - flat));
        }
        flat += len;
        selectable += len;
    }
    None
}

/// Look up the IconEntry at a given selectable index.
pub fn entry_at_selectable<'a>(
    sections: &'a [SectionView<'a>],
    selectable: usize,
) -> Option<&'a IconEntry> {
    let mut remaining = selectable;
    for section in sections {
        let len = section.entries.len();
        if remaining < len {
            return section.entries.get(remaining).copied();
        }
        remaining -= len;
    }
    None
}

pub fn move_selection(state: &mut IconPickerState, catalog: &IconCatalogData, delta: isize) {
    catalog.with_filtered(state.tab, &state.search_str(), |sections| {
        let max = selectable_count(sections);
        if max == 0 {
            return;
        }
        let cur = state.selected_index as isize;
        let next = cur.saturating_add(delta).clamp(0, (max - 1) as isize) as usize;
        state.selected_index = next;
        apply_scroll_in_sections(state, sections);
    });
}

pub fn selected_icon(state: &IconPickerState, catalog: &IconCatalogData) -> Option<String> {
    catalog.with_filtered(state.tab, &state.search_str(), |sections| {
        entry_at_selectable(sections, state.selected_index).map(|entry| entry.icon.clone())
    })
}

pub fn click_list(state: &mut IconPickerState, catalog: &IconCatalogData, x: u16, y: u16) -> bool {
    let list = state.list_inner.get();
    if list.height == 0 || y < list.y || y >= list.y + list.height || x < list.x {
        return false;
    }
    let offset_in_list = (y - list.y) as usize;
    let flat_idx = state.scroll_offset + offset_in_list;

    catalog.with_filtered(state.tab, &state.search_str(), |sections| {
        let Some(selectable_idx) = flat_to_selectable(sections, flat_idx) else {
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
        apply_scroll_in_sections(state, sections);
        is_double
    })
}

pub const TAB_STRIP_LEAD: u16 = 1;
pub const TAB_STRIP_GAP: u16 = 2;

fn tab_cell_width(label: &str) -> u16 {
    4 + label.chars().count() as u16
}

pub fn tab_at_x(tabs_inner: Rect, x: u16) -> Option<IconPickerTab> {
    if tabs_inner.width == 0 || x < tabs_inner.x {
        return None;
    }
    let rel = x - tabs_inner.x;
    if rel < TAB_STRIP_LEAD {
        return None;
    }
    let mut cursor = TAB_STRIP_LEAD;
    for (index, tab) in IconPickerTab::ALL.iter().enumerate() {
        let width = tab_cell_width(tab.label());
        let cell_end = cursor
            + width
            + if index + 1 < IconPickerTab::ALL.len() {
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

pub fn click_tab(state: &mut IconPickerState, x: u16, y: u16) -> bool {
    let tabs = state.tabs_inner.get();
    if tabs.height == 0 || y < tabs.y || y >= tabs.y + tabs.height {
        return false;
    }
    let Some(tab) = tab_at_x(tabs, x) else {
        return false;
    };
    state.set_tab(tab);
    true
}

pub fn render(f: &mut Frame, area: Rect, state: &IconPickerState, catalog: &IconCatalogData) {
    let height = ((area.height as u32 * 70) / 100) as u16;
    let height = height.clamp(14, area.height);
    let width = 64u16.min(area.width);
    let popup = centered_rect(width, height, area);
    f.render_widget(Clear, popup);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .title(Span::styled(
            " Icon Picker ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(popup);
    f.render_widget(outer, popup);

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(inner);

    render_tabs(f, layout[0], state);
    render_search(f, layout[1], state);
    render_icon_list(f, layout[2], state, catalog);
    render_footer(f, layout[3]);
}

fn render_tabs(f: &mut Frame, area: Rect, state: &IconPickerState) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    for (index, tab) in IconPickerTab::ALL.iter().enumerate() {
        if index > 0 {
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
            " icon set ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    let inner = block.inner(area);
    state.tabs_inner.set(inner);
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_search(f: &mut Frame, area: Rect, state: &IconPickerState) {
    use ratatui::layout::{Constraint, Layout};

    let prompt = Paragraph::new(Line::from(vec![
        Span::styled("  search ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_DIM())),
    ]));
    let split = Layout::horizontal([Constraint::Length(11), Constraint::Fill(1)]).split(area);
    f.render_widget(prompt, split[0]);
    f.render_widget(&state.search_query, split[1]);
}

fn render_icon_list(f: &mut Frame, area: Rect, state: &IconPickerState, catalog: &IconCatalogData) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_DIM()))
        .title(Span::styled(
            " icons ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible_height = inner.height as usize;
    state.visible_height.set(visible_height.max(1));
    state.list_inner.set(inner);
    if visible_height == 0 {
        return;
    }

    catalog.with_filtered(state.tab, &state.search_str(), |sections| {
        let total_flat = flat_len(sections);
        let selected_flat = selectable_to_flat(sections, state.selected_index);
        let scroll = state.scroll_offset;
        let view_end = scroll + visible_height;

        let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
        let mut row = 0usize;
        'outer: for section in sections {
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
                    let Some(entry) = section.entries.get(entry_idx).copied() else {
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

        f.render_widget(Paragraph::new(lines), inner);

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
            f.render_widget(
                Paragraph::new(Span::styled(
                    counter,
                    Style::default().fg(theme::TEXT_DIM()),
                )),
                counter_area,
            );
        }
    });
}

fn header_line(title: &str) -> Line<'static> {
    let dashes = "\u{2500}".repeat(3);
    Line::from(vec![
        Span::styled(
            format!("{dashes}\u{2500}{dashes} "),
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
    let icon = &entry.icon;
    let name = &entry.name;
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
                name.clone(),
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
            Span::styled(name.clone(), Style::default().fg(theme::TEXT())),
        ])
    }
}

fn render_footer(f: &mut Frame, area: Rect) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default().fg(theme::AMBER_DIM());
    let sep = Span::styled(" • ", Style::default().fg(theme::TEXT_FAINT()));
    let mut spans = vec![
        Span::raw("  "),
        Span::styled("\u{23CE}", key),
        Span::styled(" insert", dim),
        sep.clone(),
        Span::styled("Alt+\u{23CE}", key),
        Span::styled(" keep open", dim),
        sep.clone(),
        Span::styled("Tab/S+Tab", key),
        Span::styled(" switch", dim),
    ];
    spans.push(sep);
    spans.push(Span::styled("Esc", key));
    spans.push(Span::styled(" close", dim));

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn apply_scroll_in_sections(state: &mut IconPickerState, sections: &[SectionView<'_>]) {
    let flat_idx = selectable_to_flat(sections, state.selected_index).unwrap_or(0);
    let visible = state.visible_height.get().max(1);
    if flat_idx < state.scroll_offset {
        state.scroll_offset = flat_idx;
    } else if flat_idx >= state.scroll_offset + visible {
        state.scroll_offset = flat_idx.saturating_sub(visible - 1);
    }
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

    fn make_entry(name: &str) -> IconEntry {
        IconEntry {
            icon: "x".to_string(),
            name: name.to_string(),
            name_lower: name.to_lowercase(),
        }
    }

    fn two_section_view() -> (Vec<IconEntry>, Vec<IconEntry>) {
        let a = vec![make_entry("a0"), make_entry("a1")];
        let b = vec![make_entry("b0"), make_entry("b1"), make_entry("b2")];
        (a, b)
    }

    fn views<'a>(a: &'a [IconEntry], b: &'a [IconEntry]) -> Vec<SectionView<'a>> {
        vec![
            SectionView {
                title: "A",
                entries: a.iter().collect(),
            },
            SectionView {
                title: "B",
                entries: b.iter().collect(),
            },
        ]
    }

    #[test]
    fn flat_len_counts_headers_plus_entries() {
        let (a, b) = two_section_view();
        let sections = views(&a, &b);
        assert_eq!(flat_len(&sections), 7);
        assert_eq!(selectable_count(&sections), 5);
    }

    #[test]
    fn selectable_to_flat_skips_headers() {
        let (a, b) = two_section_view();
        let sections = views(&a, &b);
        assert_eq!(selectable_to_flat(&sections, 0), Some(1));
        assert_eq!(selectable_to_flat(&sections, 1), Some(2));
        assert_eq!(selectable_to_flat(&sections, 2), Some(4));
        assert_eq!(selectable_to_flat(&sections, 3), Some(5));
        assert_eq!(selectable_to_flat(&sections, 4), Some(6));
        assert_eq!(selectable_to_flat(&sections, 5), None);
    }

    #[test]
    fn flat_to_selectable_returns_none_for_headers() {
        let (a, b) = two_section_view();
        let sections = views(&a, &b);
        assert_eq!(flat_to_selectable(&sections, 0), None);
        assert_eq!(flat_to_selectable(&sections, 1), Some(0));
        assert_eq!(flat_to_selectable(&sections, 2), Some(1));
        assert_eq!(flat_to_selectable(&sections, 3), None);
        assert_eq!(flat_to_selectable(&sections, 4), Some(2));
        assert_eq!(flat_to_selectable(&sections, 6), Some(4));
        assert_eq!(flat_to_selectable(&sections, 7), None);
    }

    #[test]
    fn flat_selectable_round_trip() {
        let (a, b) = two_section_view();
        let sections = views(&a, &b);
        for selectable in 0..selectable_count(&sections) {
            let flat = selectable_to_flat(&sections, selectable).unwrap();
            assert_eq!(flat_to_selectable(&sections, flat), Some(selectable));
        }
    }

    #[test]
    fn entry_at_selectable_crosses_section_boundary() {
        let (a, b) = two_section_view();
        let sections = views(&a, &b);
        assert_eq!(entry_at_selectable(&sections, 0).unwrap().name, "a0");
        assert_eq!(entry_at_selectable(&sections, 2).unwrap().name, "b0");
        assert_eq!(entry_at_selectable(&sections, 4).unwrap().name, "b2");
        assert!(entry_at_selectable(&sections, 5).is_none());
    }

    #[test]
    fn tab_navigation_cycles_forward_and_back() {
        let mut state = IconPickerState::default();
        state.next_tab();
        assert_eq!(state.tab, IconPickerTab::Unicode);
        state.next_tab();
        assert_eq!(state.tab, IconPickerTab::NerdFont);
        state.next_tab();
        assert_eq!(state.tab, IconPickerTab::Emoji);
        state.prev_tab();
        assert_eq!(state.tab, IconPickerTab::NerdFont);
    }
}
