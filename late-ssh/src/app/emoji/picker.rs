use super::{
    EmojiPickerState,
    catalog::{IconCatalogData, IconEntry, IconPickerTab, SectionView},
};
use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
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
            return s.entries.get(remaining);
        }
        remaining -= len;
    }
    None
}

pub fn render(f: &mut Frame, area: Rect, state: &EmojiPickerState, catalog: &IconCatalogData) {
    let height = ((area.height as u32 * 70) / 100) as u16;
    let height = height.clamp(12, area.height);
    let popup = centered_rect(56, height, area);
    f.render_widget(Clear, popup);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE))
        .title(Span::styled(
            " Icon Picker ",
            Style::default()
                .fg(theme::AMBER_GLOW)
                .add_modifier(Modifier::BOLD),
        ))
        .title(
            Line::from(vec![
                Span::styled("Esc", Style::default().fg(theme::AMBER_DIM)),
                Span::raw(" "),
                Span::styled("Cancel ", Style::default().fg(theme::TEXT_DIM)),
            ])
            .right_aligned(),
        );

    let inner = outer_block.inner(popup);
    f.render_widget(outer_block, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Icon Set tabs
            Constraint::Length(3), // Search
            Constraint::Min(3),    // Icons list
            Constraint::Length(3), // Keymap
        ])
        .split(inner);

    render_tabs(f, layout[0], state);
    render_search(f, layout[1], state);
    render_icon_list(f, layout[2], state, catalog);

    render_keymap(
        f,
        layout[3],
        &[
            ("Tab", "Switch Set"),
            ("\u{23CE}", "Insert"),
            ("Alt+\u{23CE}", "Insert (keep open)"),
        ],
    );
}

fn render_tabs(f: &mut Frame, area: Rect, state: &EmojiPickerState) {
    let tabs = [
        ("Emoji", IconPickerTab::Emoji),
        ("Unicode", IconPickerTab::Unicode),
        ("Nerd Font", IconPickerTab::NerdFont),
    ];

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    for (i, (label, tab)) in tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default().fg(theme::BORDER_DIM)));
        }
        let selected = state.tab == *tab;
        let indicator = if selected { "\u{2022}" } else { " " };
        let style = if selected {
            Style::default()
                .fg(theme::AMBER_GLOW)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_MUTED)
        };
        spans.push(Span::styled(format!("[{}] {}", indicator, label), style));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_DIM))
        .title(Span::styled(
            " Icon Set ",
            Style::default().fg(theme::TEXT_MUTED),
        ));

    let line = Line::from(spans);
    let para = Paragraph::new(line).block(block);
    f.render_widget(para, area);
}

fn render_search(f: &mut Frame, area: Rect, state: &EmojiPickerState) {
    let text = render_text_with_cursor(&state.search_query, state.search_cursor);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::AMBER))
        .title(Span::styled(
            " Search ",
            Style::default().fg(theme::AMBER_GLOW),
        ));

    let para = Paragraph::new(text).block(block);
    f.render_widget(para, area);
}

/// Render a text field with a block cursor at the given character position.
fn render_text_with_cursor(text: &str, cursor_pos: usize) -> Line<'static> {
    let before: String = text.chars().take(cursor_pos).collect();
    let cursor_char: String = text.chars().nth(cursor_pos).map_or(
        "\u{2588}".to_string(), // block cursor when at end
        |c| c.to_string(),
    );
    let after: String = text.chars().skip(cursor_pos + 1).collect();

    let cursor_style = if cursor_pos < text.chars().count() {
        Style::default()
            .fg(theme::BG_SELECTION)
            .bg(theme::AMBER_GLOW)
    } else {
        Style::default().fg(theme::AMBER_GLOW)
    };

    Line::from(vec![
        Span::raw(" "),
        Span::styled(before, Style::default().fg(theme::TEXT_BRIGHT)),
        Span::styled(cursor_char, cursor_style),
        Span::styled(after, Style::default().fg(theme::TEXT_BRIGHT)),
    ])
}

fn render_icon_list(
    f: &mut Frame,
    area: Rect,
    state: &EmojiPickerState,
    catalog: &IconCatalogData,
) {
    let tab = *state.tab.current();
    let sections = catalog.sections(tab, &state.search_query);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_DIM))
        .title(Span::styled(
            " Icons ",
            Style::default().fg(theme::TEXT_MUTED),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

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

    // Walk sections and only emit Line widgets for rows intersecting [scroll, view_end).
    // This avoids materializing the full 100k-row flat list for the Unicode tab.
    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut row = 0usize;
    'outer: for section in &sections {
        if row >= view_end {
            break;
        }
        // Header row
        if row >= scroll && row < view_end {
            lines.push(header_line(section.title));
            if lines.len() == visible_height {
                break 'outer;
            }
        }
        row += 1;
        let entries_len = section.entries.len();
        let entries_end = row + entries_len;

        // Visible window inside this section's entries.
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

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);

    if total_flat > 0 {
        let total_pages = total_flat.div_ceil(visible_height);
        let current_page = scroll / visible_height + 1;
        let counter = format!(" Page {}/{} ", current_page, total_pages);
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
                Style::default().fg(theme::TEXT_FAINT),
            )),
            counter_area,
        );
    }
}

fn header_line(title: &'static str) -> Line<'static> {
    let dashes = "\u{2500}".repeat(3);
    Line::from(vec![
        Span::styled(
            format!("{dashes}\u{2500}{dashes} "),
            Style::default().fg(theme::TEXT_FAINT),
        ),
        Span::styled(
            title,
            Style::default()
                .fg(theme::AMBER_DIM)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {dashes}"), Style::default().fg(theme::TEXT_FAINT)),
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
                    .fg(theme::TEXT_BRIGHT)
                    .bg(theme::BG_HIGHLIGHT),
            ),
            Span::styled(
                name.clone(),
                Style::default()
                    .fg(theme::AMBER_GLOW)
                    .bg(theme::BG_HIGHLIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ".repeat(pad), Style::default().bg(theme::BG_HIGHLIGHT)),
        ])
    } else {
        Line::from(vec![
            Span::styled(format!(" {icon} "), Style::default().fg(theme::TEXT_BRIGHT)),
            Span::styled(name.clone(), Style::default().fg(theme::TEXT)),
        ])
    }
}

fn render_keymap(f: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    let line = render_key_hints(hints);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_DIM))
        .title(Span::styled(
            " Keymap ",
            Style::default().fg(theme::TEXT_MUTED),
        ));

    let para = Paragraph::new(line).block(block);
    f.render_widget(para, area);
}

fn render_key_hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let key_style = Style::default().fg(theme::AMBER_DIM);
    let label_style = Style::default().fg(theme::TEXT_DIM);

    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   ".to_string()));
        }
        spans.push(Span::styled(key.to_string(), key_style));
        spans.push(Span::raw(" ".to_string()));
        spans.push(Span::styled(label.to_string(), label_style));
    }
    Line::from(spans)
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center);
    let [vert] = vertical.areas(area);
    let [rect] = horizontal.areas(vert);
    rect
}
