use super::{
    IconPickerState,
    catalog::{IconCatalogData, IconEntry, SectionView},
};
use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Total number of selectable (non-header) entries across all sections.
pub fn selectable_count(sections: &[SectionView<'_>]) -> usize {
    sections.iter().map(|s| s.entries.len()).sum()
}

/// Total number of flat rows (1 header + N entries per section).
pub fn flat_len(sections: &[SectionView<'_>]) -> usize {
    sections.iter().map(|s| s.entries.len() + 1).sum()
}

/// Map a selectable index → flat row index.
pub fn selectable_to_flat(sections: &[SectionView<'_>], sel: usize) -> Option<usize> {
    let mut flat = 0;
    let mut remaining = sel;
    for s in sections {
        flat += 1; // header
        let len = s.entries.len();
        if remaining < len {
            return Some(flat + remaining);
        }
        remaining -= len;
        flat += len;
    }
    None
}

/// Map a flat row index → selectable index. Returns None for header rows.
pub fn flat_to_selectable(sections: &[SectionView<'_>], flat_idx: usize) -> Option<usize> {
    let mut flat = 0;
    let mut selectable = 0;
    for s in sections {
        if flat_idx == flat {
            return None; // header row
        }
        flat += 1;
        let len = s.entries.len();
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
    sel: usize,
) -> Option<&'a IconEntry> {
    let mut remaining = sel;
    for s in sections {
        let len = s.entries.len();
        if remaining < len {
            return s.entries.get(remaining).copied();
        }
        remaining -= len;
    }
    None
}

pub fn render(f: &mut Frame, area: Rect, state: &IconPickerState, catalog: &IconCatalogData) {
    let height = ((area.height as u32 * 70) / 100) as u16;
    let height = height.clamp(12, area.height);
    let popup = centered_rect(56, height, area);
    f.render_widget(Clear, popup);

    let outer_block = Block::default()
        .title(" Icon Picker ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));

    let inner = outer_block.inner(popup);
    f.render_widget(outer_block, popup);

    // search (1) · list (fill) · divider + hint (2)
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(inner);

    render_search(f, layout[0], state);
    render_icon_list(f, layout[1], state, catalog);
    render_footer(f, layout[2]);
}

fn render_search(f: &mut Frame, area: Rect, state: &IconPickerState) {
    let mut spans = vec![
        Span::styled("  search ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_DIM())),
    ];
    spans.extend(render_text_with_cursor(
        &state.search_query,
        state.search_cursor,
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Return spans rendering a text field with a block cursor at the given
/// character position.
fn render_text_with_cursor(text: &str, cursor_pos: usize) -> Vec<Span<'static>> {
    let before: String = text.chars().take(cursor_pos).collect();
    let cursor_char: String = text
        .chars()
        .nth(cursor_pos)
        .map_or("\u{2588}".to_string(), |c| c.to_string());
    let after: String = text.chars().skip(cursor_pos + 1).collect();

    let cursor_style = if cursor_pos < text.chars().count() {
        Style::default()
            .fg(theme::BG_SELECTION())
            .bg(theme::AMBER_GLOW())
    } else {
        Style::default().fg(theme::AMBER_GLOW())
    };

    vec![
        Span::styled(before, Style::default().fg(theme::TEXT_BRIGHT())),
        Span::styled(cursor_char, cursor_style),
        Span::styled(after, Style::default().fg(theme::TEXT_BRIGHT())),
    ]
}

fn render_icon_list(f: &mut Frame, area: Rect, state: &IconPickerState, catalog: &IconCatalogData) {
    let sections = catalog.filtered(&state.search_query);

    let inner = area;
    let visible_height = inner.height as usize;
    state.visible_height.set(visible_height.max(1));
    state.list_inner.set(inner);
    if visible_height == 0 {
        return;
    }

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

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);
    let _ = total_flat;
}

fn header_line(title: &'static str) -> Line<'static> {
    let dashes = "\u{2500}".repeat(3);
    Line::from(vec![
        Span::styled(
            format!("{dashes}\u{2500}{dashes} "),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(
            title,
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
    let hint = Line::from(vec![
        Span::raw("  "),
        Span::styled("\u{23CE}", key),
        Span::styled(" insert   ", dim),
        Span::styled("Alt+\u{23CE}", key),
        Span::styled(" keep open   ", dim),
        Span::styled("Esc", key),
        Span::styled(" close", dim),
    ]);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(hint), inner);
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
        // 1 header + 2 entries + 1 header + 3 entries = 7
        assert_eq!(flat_len(&sections), 7);
        assert_eq!(selectable_count(&sections), 5);
    }

    #[test]
    fn selectable_to_flat_skips_headers() {
        let (a, b) = two_section_view();
        let sections = views(&a, &b);
        // Layout: [0]hdrA [1]a0 [2]a1 [3]hdrB [4]b0 [5]b1 [6]b2
        assert_eq!(selectable_to_flat(&sections, 0), Some(1)); // a0
        assert_eq!(selectable_to_flat(&sections, 1), Some(2)); // a1
        assert_eq!(selectable_to_flat(&sections, 2), Some(4)); // b0
        assert_eq!(selectable_to_flat(&sections, 3), Some(5));
        assert_eq!(selectable_to_flat(&sections, 4), Some(6));
        assert_eq!(selectable_to_flat(&sections, 5), None);
    }

    #[test]
    fn flat_to_selectable_returns_none_for_headers() {
        let (a, b) = two_section_view();
        let sections = views(&a, &b);
        assert_eq!(flat_to_selectable(&sections, 0), None); // hdrA
        assert_eq!(flat_to_selectable(&sections, 1), Some(0));
        assert_eq!(flat_to_selectable(&sections, 2), Some(1));
        assert_eq!(flat_to_selectable(&sections, 3), None); // hdrB
        assert_eq!(flat_to_selectable(&sections, 4), Some(2));
        assert_eq!(flat_to_selectable(&sections, 6), Some(4));
        assert_eq!(flat_to_selectable(&sections, 7), None);
    }

    #[test]
    fn flat_selectable_round_trip() {
        let (a, b) = two_section_view();
        let sections = views(&a, &b);
        for sel in 0..selectable_count(&sections) {
            let flat = selectable_to_flat(&sections, sel).unwrap();
            assert_eq!(flat_to_selectable(&sections, flat), Some(sel));
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
    fn filtered_drops_empty_sections() {
        use crate::app::icon_picker::catalog::IconCatalogData;
        let catalog = IconCatalogData::load();
        // A query that matches nothing should leave no section headers
        // hanging — the filter must drop empty sections entirely.
        let sections = catalog.filtered("zzzzzz-no-match-xyz");
        assert!(
            sections.is_empty(),
            "expected empty filter result, got {} sections",
            sections.len()
        );
    }
}
