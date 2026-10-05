use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use late_core::models::user::{RightSidebarMode, RoomListMode};

use crate::app::common::{
    markdown::render_body_to_lines, primitives::thousands, sidebar::SidebarOwnership, theme,
};
use crate::app::referral::state::{
    INVITEE_BONUS_CHIPS, INVITER_REWARD_CHIPS, invitee_status_label, ssh_invite_command,
};
use late_core::models::referral::ReferralStatus;

use super::mouse::{Field, Pane, Target};

use super::{
    data::country_label,
    gem::{GemPosition, GemState, MoveDirection},
    state::{
        AccountRow, BIO_MAX_LEN, IrcTokenFocus, LinkAccountEnterCodeFocus, LinkAccountStep,
        PickerKind, Row, SettingsModalState, StatuslineDial, StatuslinePane, Tab, ThemeTreeRow,
        TweakRow,
    },
};

pub(crate) const MODAL_WIDTH: u16 = 96;
pub(crate) const MODAL_HEIGHT: u16 = 34;

/// `ownership` marks the shop-gated sidebar panels the account cannot show
/// yet in the Sidebar panels dialog.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &SettingsModalState,
    ownership: SidebarOwnership,
) {
    let size = frame.area();
    state.mouse.begin((size.width, size.height));
    let mut surface = Surface {
        buffer: frame.buffer_mut(),
    };
    draw_surface(&mut surface, area, state, ownership);
    state.mouse.finish();
}

struct Surface<'a> {
    buffer: &'a mut Buffer,
}
impl Surface<'_> {
    fn render_widget(&mut self, widget: impl Widget, area: Rect) {
        widget.render(area, self.buffer);
    }
}

/// Render the complete body once, then copy its viewport and translate only
/// visible hit targets. Short terminals never compress one-row controls.
fn draw_scroll(
    frame: &mut Surface<'_>,
    area: Rect,
    state: &SettingsModalState,
    pane: Pane,
    rows: usize,
    focus: usize,
    draw: impl FnOnce(&mut Surface<'_>, Rect),
) {
    if area.is_empty() {
        return;
    }
    let rows = rows.max(area.height as usize).min(u16::MAX as usize);
    let offset = state.mouse.pane(area, pane, rows, focus);
    let local = Rect::new(0, 0, area.width, rows as u16);
    let mut buffer = Buffer::empty(local);
    let mark = state.mouse.mark();
    draw(
        &mut Surface {
            buffer: &mut buffer,
        },
        local,
    );
    state.mouse.translate(mark, area, offset);
    for y in 0..area.height {
        for x in 0..area.width {
            frame.buffer[(area.x + x, area.y + y)] = buffer[(x, y + offset as u16)].clone();
        }
    }
}

fn button(
    frame: &mut Surface<'_>,
    area: Rect,
    state: &SettingsModalState,
    label: &str,
    target: Target,
) {
    let width = Span::raw(label).width() as u16;
    let rect = Rect::new(area.x, area.y, area.width.min(width), area.height.min(1));
    frame.render_widget(
        Paragraph::new(label).style(Style::default().fg(theme::AMBER_GLOW())),
        rect,
    );
    state.mouse.hit(rect, target);
}

fn close_button(frame: &mut Surface<'_>, popup: Rect, state: &SettingsModalState) {
    button(
        frame,
        Rect::new(
            popup.right().saturating_sub(4).max(popup.x),
            popup.y,
            popup.width.min(3),
            popup.height.min(1),
        ),
        state,
        "[x]",
        Target::Close,
    );
}

fn draw_surface(
    frame: &mut Surface<'_>,
    area: Rect,
    state: &SettingsModalState,
    ownership: SidebarOwnership,
) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Settings ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, state);

    let tabs_height = tab_rows(inner.width, state);
    let layout = Layout::vertical([
        Constraint::Length(0),           // breathing room
        Constraint::Length(tabs_height), // tabs
        Constraint::Length(1),           // breathing room
        Constraint::Min(1),              // body
        Constraint::Length(1),           // footer
    ])
    .split(inner);

    draw_tabs(frame, layout[1], state);

    match state.selected_tab() {
        Tab::Settings => draw_scroll(
            frame,
            layout[3],
            state,
            Pane::Settings,
            settings_body_rows(),
            line_of(&settings_lines(), state.selected_row()),
            |f, a| draw_settings_tab(f, a, state),
        ),
        Tab::Tweaks => draw_scroll(
            frame,
            layout[3],
            state,
            Pane::Tweaks,
            tweak_body_rows(),
            line_of(&tweak_lines(), state.selected_tweak_row()),
            |f, a| draw_tweaks_tab(f, a, state),
        ),
        Tab::Statusline => draw_statusline_tab(frame, layout[3], state),
        Tab::Themes => draw_themes_tab(frame, layout[3], state),
        Tab::Bio => draw_bio_tab(frame, layout[3], state),
        Tab::Account => draw_scroll(
            frame,
            layout[3],
            state,
            Pane::Account,
            account_row_y(AccountRow::ALL.len()),
            account_row_y(
                AccountRow::ALL
                    .iter()
                    .position(|r| *r == state.selected_account_row())
                    .unwrap_or(0),
            ),
            |f, a| draw_account_tab(f, a, state),
        ),
        Tab::Feeds => draw_feeds_tab(frame, layout[3], state),
    }

    let mut help_area = layout[4];
    if state.editing_text() {
        help_area.width = help_area.width.saturating_sub(21);
    }
    draw_footer(frame, help_area, state);
    if state.editing_text() {
        let buttons = Rect::new(
            layout[4].right().saturating_sub(20).max(layout[4].x),
            layout[4].y,
            layout[4].width.min(20),
            layout[4].height,
        );
        button(
            frame,
            buttons,
            state,
            if state.editing_bio() {
                "[Done]"
            } else {
                "[Save]"
            },
            Target::Submit,
        );
        if !state.editing_bio() {
            button(
                frame,
                Rect::new(
                    buttons.x + 8,
                    buttons.y,
                    buttons.width.saturating_sub(8),
                    buttons.height,
                ),
                state,
                "[Cancel]",
                Target::Cancel,
            );
        }
    }

    if state.picker_open() {
        state.mouse.clear_surface();
        draw_picker(frame, popup, state);
    }
    if state.right_sidebar_components_open() {
        state.mouse.clear_surface();
        draw_right_sidebar_components_dialog(frame, popup, state, ownership);
    }
    if state.chat_badges_open() {
        state.mouse.clear_surface();
        draw_chat_badges_dialog(frame, popup, state);
    }
    if state.link_account_dialog().open() {
        state.mouse.clear_surface();
        draw_link_account_dialog(frame, popup, state);
    }
    if state.delete_account_dialog().open() {
        state.mouse.clear_surface();
        draw_delete_account_dialog(frame, popup, state);
    }
    if state.irc_token_dialog().open() {
        state.mouse.clear_surface();
        draw_irc_token_dialog(frame, popup, state);
    }
    if state.invites_dialog().open() {
        state.mouse.clear_surface();
        draw_invites_dialog(frame, popup, state);
    }
}

fn tab_rows(width: u16, state: &SettingsModalState) -> u16 {
    let mut x = 0;
    let mut rows = 1;
    for tab in state.visible_tabs() {
        let w = Span::raw(format!(" {} ", tab.label())).width() as u16;
        if x > 0 && x + w > width {
            rows += 1;
            x = 0;
        }
        x += w + 1;
    }
    rows
}

fn draw_tabs(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let mut x = area.x;
    let mut y = area.y;
    for tab in state.visible_tabs() {
        let label = format!(" {} ", tab.label());
        let width = Span::raw(&label).width() as u16;
        if x > area.x && x + width > area.right() {
            x = area.x;
            y += 1;
        }
        if y >= area.bottom() {
            break;
        }
        let rect = Rect::new(x, y, width.min(area.right().saturating_sub(x)), 1);
        let style = if tab == state.selected_tab() {
            theme::selection_style()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        frame.render_widget(Paragraph::new(label).style(style), rect);
        state.mouse.hit(rect, Target::Tab(tab));

        x += width + 1;
    }
}

/// One body line of a grouped tab: a heading opens each group and a blank line
/// separates groups. Drawing and scroll focus both walk the same list, so a
/// row's position is never written down twice.
#[derive(Clone, Copy, PartialEq)]
enum BodyLine<R> {
    Heading(&'static str),
    Row(R),
    Gap,
}

fn grouped_lines<R: Copy>(rows: &[R], group: impl Fn(R) -> &'static str) -> Vec<BodyLine<R>> {
    let mut lines = Vec::new();
    let mut current = None;
    for &row in rows {
        let title = group(row);
        if current != Some(title) {
            if current.is_some() {
                lines.push(BodyLine::Gap);
            }
            lines.push(BodyLine::Heading(title));
            current = Some(title);
        }
        lines.push(BodyLine::Row(row));
    }
    lines
}

fn line_of<R: Copy + PartialEq>(lines: &[BodyLine<R>], row: R) -> usize {
    lines
        .iter()
        .position(|line| *line == BodyLine::Row(row))
        .expect("every row is laid out")
}

fn settings_lines() -> Vec<BodyLine<Row>> {
    grouped_lines(&Row::ALL, |row| match row {
        Row::Username | Row::Country | Row::Timezone | Row::Theme => "Identity",
        Row::Ide | Row::Terminal | Row::Os | Row::Langs => "late.fetch",
        Row::TranslateTo | Row::AutoTranslate | Row::TranslateMine => "Translation",
        Row::DirectMessages
        | Row::Mentions
        | Row::GameEvents
        | Row::Streams
        | Row::Bell
        | Row::Cooldown
        | Row::NotifyFormat => "Notifications",
    })
}

/// The grouped rows, a breathing line, and the shortcuts hint.
fn settings_body_rows() -> usize {
    settings_lines().len() + 2
}

fn tweak_lines() -> Vec<BodyLine<TweakRow>> {
    grouped_lines(&TweakRow::ALL, |row| match row {
        TweakRow::BackgroundColor
        | TweakRow::TextBrightness
        | TweakRow::RightSidebar
        | TweakRow::RoomListSidebar => "Appearance",
        TweakRow::ComposerKeepFocused | TweakRow::InteractionMode => "Input",
        TweakRow::FlagFallback | TweakRow::TerminalImages | TweakRow::ChatBadges => "Display",
        TweakRow::LandingPage | TweakRow::PaperAtLogin | TweakRow::ArtSplash => "Startup",
    })
}

const TWEAK_GEM_ROWS: u16 = 7;

/// The grouped rows with the gem below them.
fn tweak_body_rows() -> usize {
    tweak_lines().len() + TWEAK_GEM_ROWS as usize
}

/// Account rows sit under a heading and a breathing line; each takes its
/// row, its description, and a breathing line.
fn account_row_y(index: usize) -> usize {
    2 + index * 3
}

fn draw_footer(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    if state.editing_text() && area.width < 60 {
        let hint = if state.editing_bio() {
            "  Esc save · ^J newline"
        } else {
            "  Enter save · Esc cancel"
        };
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(theme::TEXT_DIM())),
            area,
        );
        return;
    }
    let mut spans = vec![Span::raw("  ")];
    match (state.selected_tab(), state.editing_bio()) {
        (Tab::Bio, true) => {
            spans.extend([
                Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" save & preview  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Alt+Enter/Ctrl+J", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" newline  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(
                    " save & switch tabs",
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]);
        }
        (Tab::Bio, false) => {
            spans.extend([
                Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" edit  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Settings, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" navigate  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" cycle  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" edit/apply  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Themes, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" preview  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close/open  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Tweaks, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" navigate  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("←→ ↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" toggle  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Statusline, _) => {
            let close_label = if state.statusline_pane() == StatuslinePane::Detail {
                " back to components"
            } else {
                " close"
            };
            spans.extend([
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(close_label, Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Account, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" choose  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" open  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
        (Tab::Feeds, _) => {
            spans.extend([
                Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" navigate  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("↵/a", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" add  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("d", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" remove  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("r", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" refresh  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
            ]);
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The row above the tree: the live search when it is open, otherwise the hint
/// that says the search exists at all. It occupies the same row either way, so
/// opening the search never reflows the list underneath it.
fn theme_search_line(state: &SettingsModalState) -> Line<'static> {
    if !state.theme_searching() {
        return Line::from(Span::styled(
            "  / search themes · f star the selected one",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }

    // With a query the rows are all matches; without one they are still the
    // full tree (headers, favorites copies), so a count there would lie.
    let tail = match state.theme_query().trim().is_empty() {
        true => "   type to search · Esc back".to_string(),
        false => match state.theme_tree_rows().len() {
            0 => "   no matches · Esc back".to_string(),
            1 => "   1 match · ↑↓ preview · Esc back".to_string(),
            n => format!("   {n} matches · ↑↓ preview · Esc back"),
        },
    };
    Line::from(vec![
        Span::styled("  search ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(
            state.theme_query().to_string(),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled("_", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(tail, Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn draw_themes_tab(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // summary
        Constraint::Length(1), // search / hint
        Constraint::Min(4),    // tree
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(section_heading("Theme browser")),
        sections[0],
    );

    let active_id = state
        .draft()
        .theme_id
        .as_deref()
        .unwrap_or(theme::DEFAULT_ID);
    let active_preview = theme::preview_for_id(active_id);
    let summary = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            theme::label_for_id(active_id).to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   ", Style::default().fg(theme::TEXT_DIM())),
        swatch(active_preview.bg_canvas),
        swatch(active_preview.bg_selection),
        swatch(active_preview.border_active),
        swatch(active_preview.amber),
        swatch(active_preview.chat_author),
        swatch(active_preview.mention),
        swatch(active_preview.success),
        swatch(active_preview.error),
        Span::styled(
            format!("   {}", theme::color_to_hex(active_preview.border_active)),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(summary), sections[1]);
    frame.render_widget(Paragraph::new(theme_search_line(state)), sections[2]);

    let tree_area = sections[3];
    let width = tree_area.width as usize;
    let visible_height = tree_area.height as usize;
    state.set_theme_visible_height(visible_height.max(1));
    state.mouse.hit(sections[2], Target::Search);
    let offset = state.mouse.pane(
        tree_area,
        Pane::Themes,
        state.theme_tree_rows().len(),
        state.theme_selected_row(),
    );

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (row_idx, row) in state.theme_tree_rows().into_iter().enumerate().skip(offset) {
        if lines.len() >= visible_height {
            break;
        }

        let row_rect = Rect::new(
            tree_area.x,
            tree_area.y + lines.len() as u16,
            tree_area.width,
            1,
        );
        state.mouse.hit(row_rect, Target::Theme(row_idx));
        let selected = row_idx == state.theme_selected_row();
        match row {
            ThemeTreeRow::Group { group, collapsed } => {
                lines.push(theme_group_line(group, collapsed, selected, width));
            }
            ThemeTreeRow::FavoritesHeader { collapsed } => {
                lines.push(theme_pseudo_group_line(
                    "★ Favorites",
                    collapsed,
                    selected,
                    width,
                ));
            }
            ThemeTreeRow::Theme {
                option_index,
                last_in_group,
            } => {
                state.mouse.hit(
                    text_value_rect(row_rect, 6).intersection(Rect::new(
                        row_rect.x + 6,
                        row_rect.y,
                        2,
                        1,
                    )),
                    Target::Star(row_idx),
                );
                lines.push(theme_option_line(
                    theme::OPTIONS[option_index],
                    selected,
                    last_in_group,
                    state.theme_is_favorite(option_index),
                    width,
                ));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), tree_area);
}

fn theme_group_line(
    group: theme::ThemeGroup,
    collapsed: bool,
    selected: bool,
    width: usize,
) -> Line<'static> {
    theme_pseudo_group_line(group.label(), collapsed, selected, width)
}

/// A collapsible header row. Real groups and the Favorites block share it so
/// the two read identically in the tree.
fn theme_pseudo_group_line(
    label: &str,
    collapsed: bool,
    selected: bool,
    width: usize,
) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let symbol = if collapsed { "▸" } else { "▾" };
    let text = format!(" {marker} {symbol} {label}");
    let padding = width.saturating_sub(text.chars().count());
    let style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    };
    let trailing_style = if selected {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(text, style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

fn theme_option_line(
    option: theme::ThemeOption,
    selected: bool,
    last_in_group: bool,
    favorite: bool,
    width: usize,
) -> Line<'static> {
    let preview = theme::preview_for_option(option);
    let marker = if selected { "›" } else { " " };
    let branch = if last_in_group { "└─" } else { "├─" };
    let prefix = format!(" {marker} {branch} ");
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    let id_style = if selected {
        Style::default()
            .fg(theme::TEXT_DIM())
            .patch(theme::selection_style())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let trailing_style = if selected {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };
    let swatches = [
        preview.bg_canvas,
        preview.bg_selection,
        preview.border_active,
        preview.text,
        preview.text_bright,
        preview.amber,
        preview.chat_author,
        preview.mention,
    ];
    // The star reads the same inside the Favorites block and out in the theme's
    // own group, so it is always obvious which themes are starred.
    let star = if favorite { "★ " } else { "☆ " };
    let id_text = format!("  {}", option.id);
    let used = prefix.chars().count()
        + star.chars().count()
        + option.label.chars().count()
        + id_text.chars().count()
        + 2
        + (swatches.len() * 2);
    let padding = width.saturating_sub(used);
    let mut spans = vec![
        Span::styled(prefix, prefix_style),
        Span::styled(
            star.to_string(),
            match selected {
                true => Style::default()
                    .fg(theme::AMBER_GLOW())
                    .patch(theme::selection_style()),
                false => Style::default().fg(theme::AMBER()),
            },
        ),
        Span::styled(option.label.to_string(), label_style),
        Span::styled(id_text, id_style),
        Span::styled(" ".repeat(padding + 2), trailing_style),
    ];
    for color in swatches {
        spans.push(swatch(color));
    }
    Line::from(spans)
}

fn swatch(color: ratatui::style::Color) -> Span<'static> {
    Span::styled("  ", Style::default().bg(color))
}

fn draw_settings_tab(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let lines = settings_lines();
    let line_rect = |y: usize| Rect::new(area.x, area.y + y as u16, area.width, 1);
    for (y, line) in lines.iter().enumerate() {
        match *line {
            BodyLine::Gap => {}
            BodyLine::Heading(title) => {
                frame.render_widget(Paragraph::new(section_heading(title)), line_rect(y))
            }
            BodyLine::Row(row) => draw_settings_row(frame, line_rect(y), state, row),
        }
    }
    frame.render_widget(
        Paragraph::new(shortcuts_hint_line(area.width as usize)),
        line_rect(lines.len() + 1),
    );
}

fn draw_settings_row(frame: &mut Surface<'_>, rect: Rect, state: &SettingsModalState, row: Row) {
    let draft = state.draft();
    let (label, value) = match row {
        Row::Username => (
            "Username",
            if state.editing_username() {
                value_span("█", theme::AMBER())
            } else if draft.username.is_empty() {
                value_span("not set", theme::TEXT_FAINT())
            } else {
                value_span(draft.username.clone(), theme::TEXT_BRIGHT())
            },
        ),
        Row::Country => (
            "Country",
            value_with_picker_hint(country_label(draft.country.as_deref())),
        ),
        Row::Timezone => (
            "Timezone",
            value_with_picker_hint(
                draft
                    .timezone
                    .clone()
                    .unwrap_or_else(|| "not set".to_string()),
            ),
        ),
        Row::Theme => (
            "Theme",
            value_span(
                theme::label_for_id(draft.theme_id.as_deref().unwrap_or(theme::DEFAULT_ID))
                    .to_string(),
                theme::TEXT_BRIGHT(),
            ),
        ),
        Row::Ide => ("IDE", system_field_value(state, row, draft.ide.clone())),
        Row::Terminal => (
            "Terminal",
            system_field_value(state, row, draft.terminal.clone()),
        ),
        Row::Os => ("OS", system_field_value(state, row, draft.os.clone())),
        Row::Langs => (
            "Langs",
            if draft.langs.is_empty() {
                value_span("pick from the list…", theme::TEXT_FAINT())
            } else {
                value_with_picker_hint(format_lang_tags(&draft.langs))
            },
        ),
        Row::TranslateTo => ("Target language", translate_to_span(draft.translate_to)),
        Row::AutoTranslate => (
            "Auto-translate new messages",
            toggle_span(draft.auto_translate),
        ),
        Row::TranslateMine => (
            "Translate my messages to English",
            toggle_span(draft.translate_mine_to_en),
        ),
        Row::DirectMessages => ("DMs", toggle_span(has_kind(state, "dms"))),
        Row::Mentions => ("@mentions", toggle_span(has_kind(state, "mentions"))),
        Row::GameEvents => ("Game events", toggle_span(has_kind(state, "game_events"))),
        Row::Streams => (
            "Streams (friends live, your viewers)",
            toggle_span(has_kind(state, "streams")),
        ),
        Row::Bell => ("Bell", toggle_span(draft.notify_bell)),
        Row::Cooldown => ("Cooldown", cooldown_span(draft.notify_cooldown_mins)),
        Row::NotifyFormat => ("Format", notify_format_span(draft.notify_format.as_deref())),
    };
    let line = row_line(state, row, rect.width as usize, label, value);
    if matches!(row, Row::Cooldown | Row::NotifyFormat) {
        choice_hits(
            state,
            rect,
            &line,
            Target::Row(row),
            Target::RowCycle(row, false),
            Target::RowCycle(row, true),
        );
    } else {
        state.mouse.hit(rect, Target::Row(row));
    }
    frame.render_widget(Paragraph::new(line), rect);

    // An open editor paints its scrolled text over the value column.
    if row == Row::Username && state.editing_username() {
        draw_text_field(
            frame,
            text_value_rect(rect, 19),
            state,
            Field::Username,
            state.username_input(),
        );
    } else if state.editing_system_field().is_some() && state.editing_system_row(row) {
        draw_text_field(
            frame,
            text_value_rect(rect, 19),
            state,
            Field::System,
            state.system_input(),
        );
    }
}

fn shortcuts_hint_line(width: usize) -> Line<'static> {
    let bg = theme::BG_HIGHLIGHT();
    let key_style = Style::default()
        .fg(theme::AMBER_GLOW())
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(theme::TEXT_BRIGHT()).bg(bg);
    let bg_style = Style::default().bg(bg);

    let leading = "   ";
    let key1 = "?";
    let text1 = "  Guide";
    let separator = "      ";
    let key2 = "Ctrl+O";
    let text2 = "  reopen settings anywhere";

    let used = leading.chars().count()
        + key1.chars().count()
        + text1.chars().count()
        + separator.chars().count()
        + key2.chars().count()
        + text2.chars().count();
    let trailing = " ".repeat(width.saturating_sub(used));

    Line::from(vec![
        Span::styled(leading, bg_style),
        Span::styled(key1, key_style),
        Span::styled(text1, text_style),
        Span::styled(separator, bg_style),
        Span::styled(key2, key_style),
        Span::styled(text2, text_style),
        Span::styled(trailing, bg_style),
    ])
}

fn draw_tweaks_tab(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let lines = tweak_lines();
    for (y, line) in lines.iter().enumerate() {
        let rect = Rect::new(area.x, area.y + y as u16, area.width, 1);
        match *line {
            BodyLine::Gap => {}
            BodyLine::Heading(title) => {
                frame.render_widget(Paragraph::new(section_heading(title)), rect)
            }
            BodyLine::Row(row) => draw_tweak_row(frame, rect, state, row),
        }
    }
    let gem_area = Rect::new(
        area.x + 2.min(area.width / 2),
        area.bottom().saturating_sub(TWEAK_GEM_ROWS),
        area.width.saturating_sub(4),
        TWEAK_GEM_ROWS - 1,
    );
    if gem_area.width > 0 {
        draw_gem(frame, gem_area, state.gem());
        if let Some(hit) = state.gem().hit_area.get() {
            state.mouse.hit(hit, Target::Gem);
        }
    } else {
        state.gem().hit_area.set(None);
    }
}

fn draw_tweak_row(frame: &mut Surface<'_>, rect: Rect, state: &SettingsModalState, row: TweakRow) {
    let draft = state.draft();
    // `cycling` rows carry the ◂ ▸ arrows; the rest act on a click anywhere.
    let (label, value, cycling) = match row {
        TweakRow::BackgroundColor => (
            "Sync terminal background",
            toggle_span(draft.enable_background_color),
            false,
        ),
        TweakRow::TextBrightness => (
            "Text Brightness",
            text_brightness_span(draft.text_brightness_adjustment),
            true,
        ),
        TweakRow::RightSidebar => (
            "Right sidebar",
            right_sidebar_mode_span(state.device_rails().1),
            true,
        ),
        TweakRow::RoomListSidebar => (
            "Room list",
            room_list_mode_span(state.device_rails().0),
            true,
        ),
        TweakRow::ComposerKeepFocused => (
            "Send and keep open on Enter",
            toggle_span(draft.keep_composer_focused),
            false,
        ),
        TweakRow::InteractionMode => (
            "Interaction mode",
            interaction_mode_span(state.interaction_mode()),
            false,
        ),
        TweakRow::FlagFallback => ("Plain glyphs", toggle_span(draft.show_flag_fallback), false),
        TweakRow::TerminalImages => (
            "Terminal images",
            terminal_images_span(draft.terminal_images),
            true,
        ),
        TweakRow::ChatBadges => ("Chat badges", chat_badges_span(state), false),
        TweakRow::LandingPage => ("Land on", landing_page_span(draft.landing_page), true),
        TweakRow::PaperAtLogin => (
            "Daily paper at login",
            toggle_span(draft.paper_at_login),
            false,
        ),
        TweakRow::ArtSplash => (
            "Show Gallery Art on Splash",
            cycle_value_span(draft.art_splash_mode.label(), &["SFW", "Always", "Never"]),
            true,
        ),
    };
    let line = tweak_row_line(state, row, rect.width as usize, label, value);
    if cycling {
        // The value reserves its longest option, so changing it never moves
        // the arrows or changes the row's clickable extent.
        choice_hits(
            state,
            rect,
            &line,
            if row == TweakRow::RightSidebar {
                Target::SidebarMode
            } else {
                Target::Tweak(row)
            },
            Target::TweakCycle(row, false),
            Target::TweakCycle(row, true),
        );
        if row == TweakRow::RightSidebar {
            let label_end = line.spans.iter().take(2).map(Span::width).sum::<usize>() as u16;
            state.mouse.hit(
                Rect::new(rect.x, rect.y, label_end, 1).intersection(rect),
                Target::Tweak(row),
            );
            let value_end = line.spans.iter().take(3).map(Span::width).sum::<usize>() as u16;
            state.mouse.hit(
                Rect::new(
                    rect.x + value_end + 2,
                    rect.y,
                    Span::raw("[Panels]").width() as u16,
                    1,
                )
                .intersection(rect),
                Target::SidebarPanels,
            );
        }
    } else {
        state.mouse.hit(rect, Target::Tweak(row));
    }
    frame.render_widget(Paragraph::new(line), rect);
}

fn tweak_row_line(
    state: &SettingsModalState,
    row: TweakRow,
    width: usize,
    label: &str,
    value: ValueSpan,
) -> Line<'static> {
    let selected = state.selected_tweak_row() == row;

    let marker = if selected { "›" } else { " " };
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = if selected {
        value.style.patch(theme::selection_style())
    } else {
        value.style
    };

    let prefix = format!(" {marker} ");
    let panels = if row == TweakRow::RightSidebar {
        "  [Panels]"
    } else {
        ""
    };
    let label_width = width
        .saturating_sub(
            Span::raw(&prefix).width()
                + Span::raw(&value.text).width().max(5)
                + Span::raw(panels).width(),
        )
        .min(32);
    let label_text = fit_label(label, label_width);
    let mut used = Span::raw(&prefix).width()
        + Span::raw(&label_text).width()
        + Span::raw(&value.text).width()
        + Span::raw(panels).width();
    if used > width {
        used = width;
    }
    let padding = width.saturating_sub(used);
    let trailing = " ".repeat(padding);
    let trailing_style = if selected {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label_text, label_style),
        Span::styled(value.text, value_style),
        Span::styled(panels, value_style),
        Span::styled(trailing, trailing_style),
    ])
}

fn draw_account_tab(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let rows: [(AccountRow, &str, &str, bool); AccountRow::ALL.len()] = [
        (
            AccountRow::Invites,
            "Invites",
            "Invite friends with your own SSH command. Regulars earn you chips.",
            false,
        ),
        (
            AccountRow::LinkAccounts,
            "Link Accounts",
            "Move this SSH key onto another late.sh account. No data is merged.",
            false,
        ),
        (
            AccountRow::IrcToken,
            "IRC access token",
            "Create, reset, or revoke the token used by IRC clients.",
            false,
        ),
        (
            AccountRow::DeleteAccount,
            "Delete Account",
            "Delete your own account (cannot be undone!)",
            true,
        ),
    ];
    let sections =
        Layout::vertical(vec![Constraint::Length(1); account_row_y(rows.len())]).split(area);

    frame.render_widget(Paragraph::new(section_heading("Account")), sections[0]);

    for (idx, row) in AccountRow::ALL.into_iter().enumerate() {
        state
            .mouse
            .hit(sections[account_row_y(idx)], Target::Account(row));
    }
    let width = area.width as usize;
    for (index, (row, label, description, destructive)) in rows.into_iter().enumerate() {
        let top = account_row_y(index);
        frame.render_widget(
            Paragraph::new(account_row_line(state, row, width, label, destructive)),
            sections[top],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("   "),
                Span::styled(description, Style::default().fg(theme::TEXT_DIM())),
            ])),
            sections[top + 1],
        );
    }
}

fn account_row_line(
    state: &SettingsModalState,
    row: AccountRow,
    width: usize,
    label: &str,
    destructive: bool,
) -> Line<'static> {
    let selected = state.selected_account_row() == row;
    let marker = if selected { "›" } else { " " };
    let accent = if destructive {
        theme::ERROR()
    } else {
        theme::AMBER_GLOW()
    };
    let prefix_style = if selected {
        Style::default()
            .fg(accent)
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if selected {
        Style::default()
            .fg(if destructive {
                theme::ERROR()
            } else {
                theme::TEXT_BRIGHT()
            })
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else if destructive {
        Style::default().fg(theme::ERROR())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let trailing_style = if selected {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };

    let prefix = format!(" {marker} ");
    let used = prefix.chars().count() + label.chars().count();
    let trailing = " ".repeat(width.saturating_sub(used));
    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label.to_string(), label_style),
        Span::styled(trailing, trailing_style),
    ])
}

fn draw_feeds_tab(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let [head, controls, list] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(section_heading("RSS, private until shared")),
        head,
    );
    button(frame, controls, state, "[Add]", Target::AddFeed);
    button(
        frame,
        text_value_rect(controls, 7),
        state,
        "[Remove]",
        Target::RemoveFeed,
    );
    button(
        frame,
        text_value_rect(controls, 17),
        state,
        "[Refresh]",
        Target::RefreshFeeds,
    );
    draw_scroll(
        frame,
        list,
        state,
        Pane::Feeds,
        state.feeds().len() + 1,
        if state.editing_feed_url() {
            state.feeds().len()
        } else {
            state.feed_index()
        },
        |frame, area| {
            for (idx, feed) in state.feeds().iter().enumerate() {
                let row = Rect::new(area.x, area.y + idx as u16, area.width, 1);
                frame.render_widget(
                    Paragraph::new(feed_row_line(
                        idx == state.feed_index() && !state.editing_feed_url(),
                        area.width as usize,
                        feed_display_title(feed),
                        &feed.url,
                        feed.last_error.as_deref(),
                    )),
                    row,
                );
                state.mouse.hit(row, Target::Feed(feed.id));
            }
            let row = Rect::new(area.x, area.y + state.feeds().len() as u16, area.width, 1);
            frame.render_widget(
                Paragraph::new(feed_add_line(
                    state.feed_index_is_add_row(),
                    state.editing_feed_url(),
                    area.width as usize,
                    state,
                )),
                row,
            );
            state.mouse.hit(row, Target::AddFeed);
            if state.editing_feed_url() {
                draw_text_field(
                    frame,
                    text_value_rect(row, 3),
                    state,
                    Field::Feed,
                    state.feed_url_input(),
                );
            }
        },
    );
}

fn feed_display_title(feed: &late_core::models::rss_feed::RssFeed) -> String {
    let title = feed.title.trim();
    if title.is_empty() {
        "untitled RSS".to_string()
    } else {
        title.to_string()
    }
}

fn feed_row_line(
    selected: bool,
    width: usize,
    title: String,
    url: &str,
    error: Option<&str>,
) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let title_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    let url_style = if selected {
        Style::default()
            .fg(theme::TEXT_DIM())
            .patch(theme::selection_style())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let error_style = if selected {
        Style::default()
            .fg(theme::ERROR())
            .patch(theme::selection_style())
    } else {
        Style::default().fg(theme::ERROR())
    };
    let trailing_style = if selected {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };

    let prefix = format!(" {marker} ");
    let title_text = format!("{title:<28}  ");
    let status_text = error
        .map(|err| format!("  error: {err}"))
        .unwrap_or_default();
    let used = prefix.chars().count()
        + title_text.chars().count()
        + url.chars().count()
        + status_text.chars().count();
    let padding = width.saturating_sub(used.min(width));

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(title_text, title_style),
        Span::styled(url.to_string(), url_style),
        Span::styled(status_text, error_style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

fn feed_add_line(
    selected: bool,
    editing: bool,
    width: usize,
    state: &SettingsModalState,
) -> Line<'static> {
    let active = selected || editing;
    let marker = if active { "›" } else { " " };
    let prefix_style = if active {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let trailing_style = if active {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };

    let prefix = format!(" {marker} ");
    let (text, text_style) = if editing {
        let typed = state.feed_url_input().lines().join("");
        let display = if typed.is_empty() {
            "█".to_string()
        } else {
            text_with_caret(&typed, state.feed_url_input().cursor().1)
        };
        (
            display,
            Style::default()
                .fg(theme::AMBER())
                .patch(theme::selection_style()),
        )
    } else if active {
        (
            "+ Add RSS…".to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            "+ Add RSS…".to_string(),
            Style::default().fg(theme::AMBER_DIM()),
        )
    };

    let used = prefix.chars().count() + text.chars().count();
    let padding = width.saturating_sub(used.min(width));

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(text, text_style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

/// Layout note: `area` is the 6-line strip reserved at the bottom of the
/// Special tab. The small gem hugs a corner; the grand gem is centered.
/// The gem's screen-coordinate rect is stashed back on `gem.hit_area` so the
/// input handler can do mouse hit testing.
fn draw_gem(frame: &mut Surface<'_>, area: Rect, gem: &GemState) {
    if gem.evolved() {
        draw_grand_gem(frame, area, gem);
    } else {
        draw_small_gem(frame, area, gem);
    }
}

fn draw_small_gem(frame: &mut Surface<'_>, area: Rect, gem: &GemState) {
    const SMALL_W: u16 = 3;
    const SMALL_H: u16 = 3;
    if area.width < SMALL_W || area.height < SMALL_H {
        gem.hit_area.set(None);
        return;
    }
    let style = Style::default().fg(gem.color());
    let mid = match gem.brand() {
        0 => "\\ /".to_string(),
        n => format!("\\{}/", n),
    };
    let rows = ["___", mid.as_str(), " ' "];

    let x = match gem.position() {
        GemPosition::Left => area.x,
        GemPosition::Right => area.x + area.width.saturating_sub(SMALL_W),
    };
    let y_start = area.y + area.height.saturating_sub(SMALL_H);

    for (i, row) in rows.iter().enumerate() {
        let row_rect = Rect::new(x, y_start + i as u16, SMALL_W, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(row.to_string(), style))),
            row_rect,
        );
    }

    if let Some(direction) = gem.last_move() {
        draw_speed_trail(frame, area, x, y_start, direction, style);
    }

    gem.hit_area
        .set(Some(Rect::new(x, y_start, SMALL_W, SMALL_H)));
}

/// Speed-trail wisps. Rendered on the gem's middle and bottom rows,
/// extending away from the gem in the direction it just came from.
fn draw_speed_trail(
    frame: &mut Surface<'_>,
    area: Rect,
    gem_x: u16,
    gem_y_start: u16,
    direction: MoveDirection,
    style: Style,
) {
    // The two rows of trail ASCII, side-aligned with the gem so position
    // math stays in one place. Each pair is `(mid_row, bottom_row)`.
    let (mid, bottom) = match direction {
        MoveDirection::Leftward => ("  .:`  .:    .", "   ':.. ':..  ':..  ':..  :..  ..  .   ."),
        MoveDirection::Rightward => (
            ".    :.  `:.  ",
            "   .   .  ..  ..:  ..:'  ..:'  ..:' ..:'    ",
        ),
    };

    let mid_y = gem_y_start + 1;
    let bottom_y = gem_y_start + 2;

    let area_left = area.x;
    let area_right = area.x + area.width;

    for (text, y) in [(mid, mid_y), (bottom, bottom_y)] {
        let len = text.chars().count() as u16;
        let (x, render_text): (u16, String) = match direction {
            MoveDirection::Leftward => {
                // Trail starts immediately to the right of the gem; clip
                // anything that would spill past the area's right edge.
                let start = gem_x + 3;
                let available = area_right.saturating_sub(start);
                let clipped: String = text.chars().take(available as usize).collect();
                (start, clipped)
            }
            MoveDirection::Rightward => {
                // Trail ends immediately before the gem; clip from the
                // front if the area can't fit the full length.
                let want_start = gem_x.saturating_sub(len);
                let start = want_start.max(area_left);
                let drop = (start - want_start) as usize;
                let clipped: String = text.chars().skip(drop).collect();
                (start, clipped)
            }
        };
        if render_text.is_empty() {
            continue;
        }
        let width = render_text.chars().count() as u16;
        let rect = Rect::new(x, y, width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(render_text, style))),
            rect,
        );
    }
}

fn draw_grand_gem(frame: &mut Surface<'_>, area: Rect, gem: &GemState) {
    // Each row is a list of (text, kind). `Kind::Gem` styles with the gem
    // color; `Kind::Shine` styles with the shine color. Splitting by kind
    // lets the two colors live on the same cell row.
    #[derive(Clone, Copy)]
    enum Kind {
        Gem,
        Shine,
    }

    let body: [&[(&str, Kind)]; 5] = [
        &[("    _________", Kind::Gem)],
        &[("   /_|_____|_\\", Kind::Gem)],
        &[("   '. \\   / .'", Kind::Gem)],
        &[("     '.\\ /.'", Kind::Gem)],
        &[("       '.'", Kind::Gem)],
    ];
    // Sparkle decorations layered on top when shining. Indices align with
    // the body rows; the extra row ABOVE the body is `shine_top`. Each
    // shining row carries 3 leading spaces so the shine's footprint is
    // symmetric around the body (the natural layout adds 3 chars on the
    // right but only replaces a leading space on the left); that way plain
    // `max_width` centering keeps the body columns stable across shining
    // and non-shining renders.
    let shine_top: &[(&str, Kind)] = &[("     .  `  '  `  .", Kind::Shine)];
    let shine_overlay: [&[(&str, Kind)]; 5] = [
        &[
            ("    `  ", Kind::Shine),
            ("_________", Kind::Gem),
            ("  `", Kind::Shine),
        ],
        &[
            ("   _  ", Kind::Shine),
            ("/_|_____|_\\", Kind::Gem),
            ("  _", Kind::Shine),
        ],
        // Body row 2 — unchanged content, just shifted right with the rest.
        &[("      '. \\   / .'", Kind::Gem)],
        &[
            ("     `  ", Kind::Shine),
            ("'.\\ /.'", Kind::Gem),
            ("  `", Kind::Shine),
        ],
        // Body row 4 — unchanged content, shifted with the rest.
        &[("          '.'", Kind::Gem)],
    ];

    let shining = gem.shining();
    let (rows, total_height): (Vec<&[(&str, Kind)]>, u16) = if shining {
        let mut v: Vec<&[(&str, Kind)]> = Vec::with_capacity(6);
        v.push(shine_top);
        for row in &shine_overlay {
            v.push(*row);
        }
        (v, 6)
    } else {
        (body.to_vec(), 5)
    };

    if area.height < total_height {
        gem.hit_area.set(None);
        return;
    }

    let row_widths: Vec<u16> = rows
        .iter()
        .map(|row| row.iter().map(|(s, _)| s.chars().count() as u16).sum())
        .collect();
    let max_width = row_widths.iter().copied().max().unwrap_or(0);
    if area.width < max_width {
        gem.hit_area.set(None);
        return;
    }

    let x_origin = area.x + (area.width.saturating_sub(max_width)) / 2;
    let y_origin = area.y + area.height.saturating_sub(total_height);
    let gem_style = Style::default().fg(gem.color());
    let shine_style = Style::default().fg(gem.shine_color());

    for (i, row) in rows.iter().enumerate() {
        let row_width: u16 = row.iter().map(|(s, _)| s.chars().count() as u16).sum();
        let row_rect = Rect::new(x_origin, y_origin + i as u16, row_width, 1);
        let spans: Vec<Span<'_>> = row
            .iter()
            .map(|(text, kind)| {
                let style = match kind {
                    Kind::Gem => gem_style,
                    Kind::Shine => shine_style,
                };
                Span::styled((*text).to_string(), style)
            })
            .collect();
        frame.render_widget(Paragraph::new(Line::from(spans)), row_rect);
    }

    gem.hit_area
        .set(Some(Rect::new(x_origin, y_origin, max_width, total_height)));
}

fn notify_format_label(format: Option<&str>) -> &'static str {
    match format.unwrap_or("both") {
        "osc777" => "OSC 777",
        "osc9" => "OSC 9",
        _ => "Both (777 + 9)",
    }
}

fn text_value_rect(row: Rect, prefix: u16) -> Rect {
    let prefix = prefix.min(row.width);
    Rect::new(row.x + prefix, row.y, row.width - prefix, row.height)
}

fn reorder_buttons(
    frame: &mut Surface<'_>,
    row: Rect,
    state: &SettingsModalState,
    up: Target,
    down: Target,
) {
    if row.width < 4 {
        return;
    }
    let x = row.right() - 4;
    button(frame, Rect::new(x, row.y, 2, row.height), state, "[↑", up);
    button(
        frame,
        Rect::new(x + 2, row.y, 2, row.height),
        state,
        "↓]",
        down,
    );
}

fn draw_text_field(
    frame: &mut Surface<'_>,
    area: Rect,
    state: &SettingsModalState,
    field: Field,
    input: &ratatui_textarea::TextArea<'_>,
) {
    if area.is_empty() {
        return;
    }
    let text = input.lines().first().map(String::as_str).unwrap_or("");
    let cursor = input.cursor().1;
    let caret_x = Span::raw(text.chars().take(cursor).collect::<String>()).width();
    let scroll = caret_x.saturating_sub(area.width.saturating_sub(1) as usize);
    let mut spans = Vec::new();
    let mut col = 0;
    let mut logical_x = 0;
    let mut x = 0;
    for grapheme in Line::raw(text).styled_graphemes(Style::default()) {
        let width = Span::raw(grapheme.symbol).width();
        if logical_x >= scroll && x + width <= area.width as usize {
            let style = Style::default().fg(theme::AMBER());
            spans.push(Span::styled(
                grapheme.symbol.to_string(),
                if col == cursor {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                },
            ));
            state.mouse.hit(
                Rect::new(area.x + x as u16, area.y, width as u16, 1),
                Target::Caret(field, col),
            );
            x += width;
        }
        logical_x += width;
        col += grapheme.symbol.chars().count();
    }
    if x < area.width as usize {
        let end = Rect::new(area.x + x as u16, area.y, area.width - x as u16, 1);
        state.mouse.hit(end, Target::Caret(field, col));
        if cursor == col {
            spans.push(Span::styled("█", Style::default().fg(theme::AMBER())));
        }
    }
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_bio_tab(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let [header, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let text = state.bio_input().lines().join("\n");
    frame.render_widget(
        Paragraph::new(format!(
            "  {}/{BIO_MAX_LEN} chars, click to edit",
            text.chars().count()
        ))
        .style(Style::default().fg(theme::TEXT_DIM())),
        header,
    );
    let body = body.inner(Margin::new(2, 0));
    if body.is_empty() {
        return;
    }
    state.mouse.hit(body, Target::Bio);
    if !state.editing_bio() {
        let lines = if state.draft().bio.trim().is_empty() {
            vec![Line::raw(
                "Press ↵ or click to write your bio. Markdown is supported.",
            )]
        } else {
            render_body_to_lines(
                &state.draft().bio,
                body.width as usize,
                Span::raw(""),
                Style::default().fg(theme::TEXT()),
            )
        };
        let rows = Paragraph::new(lines.clone())
            .wrap(Wrap { trim: false })
            .line_count(body.width);
        let offset = state.mouse.pane(body, Pane::Bio, rows, 0);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((offset as u16, 0)),
            body,
        );
        return;
    }
    // TextArea owns wrapping, its viewport and the caret while editing.
    frame.render_widget(state.bio_input(), body);
}

fn draw_picker(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let popup = centered_rect(54, 20, area);
    frame.render_widget(Clear, popup);

    let title = match state.picker().kind {
        Some(PickerKind::Country) => " Pick Country ",
        Some(PickerKind::Timezone) => " Pick Timezone ",
        Some(PickerKind::Language) => " Target language ",
        Some(PickerKind::InteractionMode) => " Interaction mode ",
        None => " Picker ",
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, state);

    let explanation = if state.picker().kind == Some(PickerKind::InteractionMode) {
        "Keyboard disables clicks and wheel input."
    } else {
        ""
    };
    let explanation_rows = if explanation.is_empty() {
        1
    } else {
        Paragraph::new(explanation)
            .wrap(Wrap { trim: true })
            .line_count(inner.width) as u16
    };
    let layout = Layout::vertical([
        Constraint::Length(explanation_rows),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(explanation)
            .style(Style::default().fg(theme::TEXT_DIM()))
            .wrap(Wrap { trim: true }),
        layout[0],
    );
    let search = Line::from(vec![
        Span::raw(" "),
        Span::styled("search ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(
            if state.picker().query.is_empty() {
                "type to filter".to_string()
            } else {
                state.picker().query.clone()
            },
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
    ]);
    frame.render_widget(Paragraph::new(search), layout[1]);

    let entries: Vec<String> = match state.picker().kind {
        Some(PickerKind::Country) => state
            .filtered_countries()
            .into_iter()
            .map(|country| format!("[{}] {}", country.code, country.name))
            .collect(),
        Some(PickerKind::Timezone) => state
            .filtered_timezones()
            .into_iter()
            .map(ToString::to_string)
            .collect(),
        Some(PickerKind::Language) => state
            .filtered_languages()
            .into_iter()
            .map(|lang| format!("{} [{}]", lang.label(), lang.as_str()))
            .collect(),
        Some(PickerKind::InteractionMode) => state
            .filtered_interaction_modes()
            .into_iter()
            .map(|mode| super::state::interaction_mode_label(mode).to_string())
            .collect(),
        None => Vec::new(),
    };

    let list_width = layout[2].width as usize;
    let visible_height = layout[2].height as usize;
    state.picker().visible_height.set(visible_height.max(1));
    let scroll = state.mouse.pane(
        layout[2],
        Pane::Picker,
        entries.len(),
        state.picker().selected_index,
    );
    let end = (scroll + visible_height).min(entries.len());
    let mut lines = Vec::new();
    for (idx, entry) in entries[scroll..end].iter().enumerate() {
        state.mouse.hit(
            Rect::new(layout[2].x, layout[2].y + idx as u16, layout[2].width, 1),
            Target::Pick(scroll + idx),
        );
        let selected = scroll + idx == state.picker().selected_index;
        let (marker, fg, bg, modifier) = if selected {
            (
                "›",
                theme::AMBER_GLOW(),
                Some(theme::BG_HIGHLIGHT()),
                Modifier::BOLD,
            )
        } else {
            ("·", theme::TEXT(), None, Modifier::empty())
        };
        let mut style = Style::default().fg(fg).add_modifier(modifier);
        if let Some(bg) = bg {
            style = style.bg(bg);
        }
        let content = format!(" {marker} {entry}");
        let padded = pad_to_width(&content, list_width, bg.is_some());
        lines.push(Line::from(Span::styled(padded, style)));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no results",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    frame.render_widget(Paragraph::new(lines), layout[2]);

    let footer = Line::from(vec![
        Span::raw("  "),
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" pick  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[3]);
}

fn draw_right_sidebar_components_dialog(
    frame: &mut Surface<'_>,
    area: Rect,
    state: &SettingsModalState,
    ownership: SidebarOwnership,
) {
    let components = state.right_sidebar_components();
    let count = components.len() as u16;
    // rows + heading + blank + 2 footer lines + borders.
    let popup = centered_rect(46, count + 7, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Sidebar panels ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, state);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new("Clock is always shown on top.")
            .style(Style::default().fg(theme::TEXT_DIM())),
        layout[0],
    );
    draw_scroll(
        frame,
        layout[2],
        state,
        Pane::Sidebar,
        components.len(),
        state.right_sidebar_components_index(),
        |frame, area| {
            for (idx, setting) in components.iter().enumerate() {
                let row = Rect::new(area.x, area.y + idx as u16, area.width, 1);
                let selected = state.right_sidebar_components_index() == idx;
                let text = format!(
                    " {} {} {}{}",
                    if selected { ">" } else { " " },
                    if setting.enabled { "[x]" } else { "[ ]" },
                    setting.component.label(),
                    if ownership.owns(setting.component) {
                        ""
                    } else {
                        " (/shop)"
                    }
                );
                frame.render_widget(
                    Paragraph::new(text).style(statusline_row_style(
                        selected,
                        true,
                        setting.enabled,
                    )),
                    row,
                );
                state.mouse.hit(row, Target::Sidebar(idx));
                reorder_buttons(
                    frame,
                    row,
                    state,
                    Target::SidebarMove(idx, -1),
                    Target::SidebarMove(idx, 1),
                );
            }
        },
    );

    let footer_top = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" select  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("[ ]", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" reorder  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" toggle", Style::default().fg(theme::TEXT_DIM())),
    ]);
    let footer_bottom = Line::from(vec![
        Span::raw(" "),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer_top), layout[layout.len() - 2]);
    frame.render_widget(Paragraph::new(footer_bottom), layout[layout.len() - 1]);
}

/// Bottom status bar customizer: the ordered segment list on the left, the
/// selected segment's description and dials on the right.
///
/// The list is ordered top-to-bottom the way the bar reads left-to-right, so
/// "move up" and "move left" are the same gesture and the user never has to
/// hold the mapping in their head.
fn draw_statusline_tab(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let list_width = statusline_list_width();

    let inner = area.inner(Margin::new(2, 0));
    let layout = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // blank
        Constraint::Min(0),    // list + dials
        Constraint::Length(1), // component controls
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new("Segments paint left to right along the bottom border.")
            .style(Style::default().fg(theme::TEXT_DIM())),
        layout[0],
    );

    let body = Layout::horizontal([
        Constraint::Length(list_width.min(layout[2].width)),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .split(layout[2]);
    draw_scroll(
        frame,
        body[0],
        state,
        Pane::StatusList,
        state.statusline_components().len(),
        state.statusline_index(),
        |f, a| draw_statusline_list(f, a, state),
    );
    let description_rows = state
        .statusline_components()
        .get(state.statusline_index())
        .map(|setting| {
            Paragraph::new(setting.component.description())
                .wrap(Wrap { trim: true })
                .line_count(body[2].width)
        })
        .unwrap_or(0);
    draw_scroll(
        frame,
        body[2],
        state,
        Pane::StatusDetail,
        description_rows + 3 + state.statusline_dials().len(),
        description_rows + 3 + state.statusline_dial_index(),
        |f, a| draw_statusline_dials(f, a, state),
    );

    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default().fg(theme::AMBER_DIM());
    let mut controls = vec![
        Span::styled("↑↓ j/k", key),
        Span::styled(" select  ", dim),
        Span::styled("⇧↑↓ / []", key),
        Span::styled(" reorder  ", dim),
    ];
    match state.statusline_pane() {
        StatuslinePane::List => controls.extend([
            Span::styled("Space", key),
            Span::styled(" toggle  ", dim),
            Span::styled("Enter", key),
            Span::styled(" options", dim),
        ]),
        StatuslinePane::Detail => controls.extend([
            Span::styled("←→/Space", key),
            Span::styled(" change option", dim),
        ]),
    }
    frame.render_widget(Paragraph::new(Line::from(controls)), layout[3]);
}

fn draw_statusline_list(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let components = state.statusline_components();
    let focused = state.statusline_pane() == StatuslinePane::List;

    for (idx, setting) in components.iter().enumerate() {
        if idx as u16 >= area.height {
            break;
        }
        let selected = state.statusline_index() == idx;
        let marker = if selected { ">" } else { " " };
        let checkbox = if setting.enabled { "[x]" } else { "[ ]" };
        let text = format!("{marker}{checkbox} {}", setting.component.label());
        let row = Rect::new(area.x, area.y + idx as u16, area.width, 1);
        let style = statusline_row_style(selected, focused, setting.enabled);
        frame.buffer.set_style(row, style);
        let mut label_area = row;
        label_area.width = row.width.saturating_sub(5);
        state.mouse.hit(row, Target::Status(idx));
        state.mouse.hit(
            Rect::new(row.x + 1, row.y, 3.min(row.width.saturating_sub(1)), 1),
            Target::StatusToggle(idx),
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                pad_to_width(&text, label_area.width as usize, selected),
                style,
            ))),
            label_area,
        );
        reorder_buttons(
            frame,
            row,
            state,
            Target::StatusMove(idx, -1),
            Target::StatusMove(idx, 1),
        );
    }
}

/// The description and dials for the selected segment. Renders nothing when
/// the list is empty, which only happens if the roster itself is empty.
fn draw_statusline_dials(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let Some(setting) = state
        .statusline_components()
        .get(state.statusline_index())
        .copied()
    else {
        return;
    };
    let focused = state.statusline_pane() == StatuslinePane::Detail;
    let width = area.width as usize;
    let description = Paragraph::new(setting.component.description())
        .style(Style::default().fg(theme::TEXT_DIM()))
        .wrap(Wrap { trim: true });
    let description_height = description.line_count(area.width) as u16;
    let layout = Layout::vertical([
        Constraint::Length(1), // component name
        Constraint::Length(1), // blank
        Constraint::Length(description_height),
        Constraint::Length(1), // blank
        Constraint::Min(0),    // dials
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            setting.component.label(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))),
        layout[0],
    );
    frame.render_widget(description, layout[2]);

    let area = layout[4];
    let dials = state.statusline_dials();
    if dials.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Toggle this component from the list.",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            area,
        );
        return;
    }

    for (idx, dial) in dials.into_iter().enumerate() {
        let y = idx as u16;
        if y >= area.height {
            break;
        }
        state.mouse.hit(
            Rect::new(area.x, area.y + y, area.width, 1),
            Target::Dial(idx),
        );
        let selected = focused && state.statusline_dial_index() == idx;
        let marker = if selected { "›" } else { " " };
        let title = dial.title(&setting);
        let value = statusline_dial_value(dial, &setting);
        let title_width = width
            .saturating_sub(Span::raw(&value.text).width().max(5) + 4)
            .min(Span::raw(title).width());
        let style = statusline_row_style(selected, focused, true);
        let line = Line::from(vec![
            Span::styled(format!("{marker} "), style),
            Span::styled(
                format!("{}  ", fit_label(title, title_width + 1).trim_end()),
                style,
            ),
            Span::styled(value.text, style.patch(value.style)),
        ]);
        let row = Rect::new(area.x, area.y + y, area.width, 1);
        if matches!(dial, StatuslineDial::Label | StatuslineDial::Variant) {
            choice_hits(
                state,
                row,
                &line,
                Target::Dial(idx),
                Target::DialCycle(idx, false),
                Target::DialCycle(idx, true),
            );
        }
        frame.render_widget(Paragraph::new(line), row);
    }
}

fn statusline_dial_value(
    dial: StatuslineDial,
    setting: &late_core::models::statusline::StatusComponentSetting,
) -> ValueSpan {
    use late_core::models::statusline::LabelMode;
    match dial {
        StatuslineDial::Brief => toggle_span(setting.brief),
        StatuslineDial::AutoHide => toggle_span(setting.auto_hide),
        StatuslineDial::Label => cycle_value_span(
            setting.label.label(),
            &[
                LabelMode::Icon.label(),
                LabelMode::Text.label(),
                LabelMode::None.label(),
            ],
        ),
        StatuslineDial::Variant => {
            let choices: Vec<_> = setting
                .component
                .variants()
                .iter()
                .map(|variant| variant.label())
                .collect();
            let label = setting
                .variant
                .or_else(|| setting.component.variants().first().copied())
                .map(|variant| variant.label())
                .unwrap_or("");
            cycle_value_span(label, &choices)
        }
    }
}

fn statusline_list_width() -> u16 {
    late_core::models::statusline::StatusComponent::ALL
        .into_iter()
        .map(|component| Span::raw(format!(">[ ] {} [↑↓]", component.label())).width() as u16)
        .max()
        .unwrap_or(0)
}

/// Row styling shared by both panes. Only the focused pane paints a selection
/// background; the other keeps its cursor visible as brightened text, so it's
/// always clear which segment the dials belong to.
fn statusline_row_style(selected: bool, focused: bool, enabled: bool) -> Style {
    if selected && focused {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else if enabled {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    }
}

/// Every badge a chat label can carry, one row each (a game's ladder is one
/// row showing only its top rung), with a show/hide switch. The list scrolls
/// to keep the cursor visible on short terminals.
fn draw_chat_badges_dialog(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let rows = late_core::models::profile_award::chat_badge_rows();
    // rows + heading + blank + 2 footer lines + borders.
    let popup = centered_rect(58, rows.len() as u16 + 7, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Chat badges ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, state);

    let layout = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // blank
        Constraint::Min(1),    // rows
        Constraint::Length(1), // footer line 1
        Constraint::Length(1), // footer line 2
    ])
    .split(inner);

    let width = inner.width as usize;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Earn it, hide it. Games show their top badge.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[0],
    );

    let visible = layout[2].height as usize;
    let selected_index = state.chat_badges_index();
    let offset = state
        .mouse
        .pane(layout[2], Pane::Badges, rows.len(), selected_index);
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .map(|(idx, row)| {
            state.mouse.hit(
                Rect::new(
                    layout[2].x,
                    layout[2].y + (idx - offset) as u16,
                    layout[2].width,
                    1,
                ),
                Target::Badge(idx),
            );
            let selected = selected_index == idx;
            let shown = state.chat_badge_row_shown(row);
            let marker = if selected { ">" } else { " " };
            let checkbox = if shown { "[x]" } else { "[ ]" };
            let text = format!(" {marker} {checkbox} {:<22} {}", row.label, row.codes);
            let style = if selected {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .patch(theme::selection_style())
                    .add_modifier(Modifier::BOLD)
            } else if shown {
                Style::default().fg(theme::TEXT())
            } else {
                Style::default().fg(theme::TEXT_FAINT())
            };
            Line::from(Span::styled(pad_to_width(&text, width, selected), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), layout[2]);

    let footer_top = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" select  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" show / hide", Style::default().fg(theme::TEXT_DIM())),
    ]);
    let footer_bottom = Line::from(vec![
        Span::raw(" "),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer_top), layout[3]);
    frame.render_widget(Paragraph::new(footer_bottom), layout[4]);
}

fn draw_link_account_dialog(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let popup = centered_rect(76, 22, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Link Accounts ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, state);

    draw_scroll(
        frame,
        inner,
        state,
        Pane::Link,
        20,
        if state.link_account_dialog().step() == LinkAccountStep::EnterCode {
            7
        } else {
            11
        },
        |f, a| draw_link_account_body(f, a, state),
    );
}

fn draw_link_account_body(frame: &mut Surface<'_>, inner: Rect, state: &SettingsModalState) {
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    match state.link_account_dialog().step() {
        LinkAccountStep::EnterCode => draw_link_account_enter_code(frame, &layout, state),
        LinkAccountStep::Confirm | LinkAccountStep::Pending => {
            draw_link_account_confirm(frame, &layout, state)
        }
    }
}

fn draw_link_account_enter_code(
    frame: &mut Surface<'_>,
    layout: &[Rect],
    state: &SettingsModalState,
) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Open Settings > Account on the other account and exchange codes.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[0],
    );

    let own_code = state
        .link_account_dialog()
        .own_code()
        .map(str::to_string)
        .unwrap_or_else(|| "not generated".to_string());
    let expires = state
        .link_account_dialog()
        .expires_at()
        .map(|expires| format!("  expires {}", expires.format("%H:%M UTC")))
        .unwrap_or_default();
    let enter_focus = state.link_account_dialog().enter_code_focus();
    frame.render_widget(
        Paragraph::new(link_account_generate_line(
            enter_focus == LinkAccountEnterCodeFocus::GenerateCode,
            state.link_account_dialog().pending(),
            layout[2].width as usize,
        )),
        layout[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "This account code: ",
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(
                own_code,
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(expires, Style::default().fg(theme::TEXT_FAINT())),
        ])),
        layout[4],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Other account code:",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[6],
    );
    frame.render_widget(
        Paragraph::new(link_account_input_line(
            state.link_account_dialog().code_input(),
            "code",
            state.link_account_dialog().pending(),
            enter_focus == LinkAccountEnterCodeFocus::PeerCode,
        )),
        layout[7],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "No data is merged. You will choose the main account on the next screen.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[9],
    );
    state.mouse.hit(layout[2], Target::GenerateCode);
    draw_text_field(
        frame,
        text_value_rect(layout[7], 3),
        state,
        Field::LinkCode,
        state.link_account_dialog().code_input(),
    );
    button(frame, layout[12], state, "[Continue]", Target::LookupCode);
    draw_link_account_status(frame, layout[10], state);
    draw_link_account_footer(frame, layout[16], state);
}

fn draw_link_account_confirm(frame: &mut Surface<'_>, layout: &[Rect], state: &SettingsModalState) {
    let dialog = state.link_account_dialog();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Choose the main account to keep.",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        layout[0],
    );

    let width = layout[2].width as usize;
    frame.render_widget(
        Paragraph::new(link_account_choice_line(
            dialog.keep_current(),
            width,
            "Current",
            state.draft().username.as_str(),
            state.draft().created_at.as_ref().cloned(),
        )),
        layout[2],
    );
    frame.render_widget(
        Paragraph::new(link_account_choice_line(
            !dialog.keep_current(),
            width,
            "Other",
            dialog.peer_username().unwrap_or("unknown"),
            dialog.peer_created(),
        )),
        layout[3],
    );

    let warning = [
        "Both SSH keys will open the main account.",
        "The other account's data will be abandoned.",
        "No chips, messages, scores, streaks, or settings are merged.",
    ];
    for (idx, text) in warning.into_iter().enumerate() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(text, Style::default().fg(theme::ERROR())),
            ])),
            layout[5 + idx],
        );
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Type the main username to confirm:",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[9],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                state
                    .link_account_kept_username()
                    .unwrap_or_else(|| "main username".to_string()),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        layout[10],
    );
    frame.render_widget(
        Paragraph::new(link_account_input_line(
            dialog.confirm_input(),
            "main username",
            dialog.pending(),
            true,
        )),
        layout[11],
    );

    state.mouse.hit(layout[2], Target::KeepAccount(true));
    state.mouse.hit(layout[3], Target::KeepAccount(false));
    draw_text_field(
        frame,
        text_value_rect(layout[11], 3),
        state,
        Field::LinkConfirm,
        dialog.confirm_input(),
    );
    button(
        frame,
        layout[14],
        state,
        "[Link accounts]",
        Target::ConfirmLink,
    );
    draw_link_account_status(frame, layout[13], state);
    draw_link_account_footer(frame, layout[16], state);
}

fn link_account_choice_line(
    selected: bool,
    width: usize,
    label: &str,
    username: &str,
    created: Option<chrono::DateTime<chrono::Utc>>,
) -> Line<'static> {
    let marker = if selected { "●" } else { "○" };
    let choice = if selected { "  main" } else { "" };
    let content = format!(
        " {marker} {label:<8} {username:<18} {}{choice}",
        created_label(created)
    );
    let style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Line::from(Span::styled(pad_to_width(&content, width, selected), style))
}

fn created_label(created: Option<chrono::DateTime<chrono::Utc>>) -> String {
    created
        .map(|created| format!("created {}", created.format("%Y-%m-%d")))
        .unwrap_or_else(|| "created unknown".to_string())
}

fn link_account_generate_line(selected: bool, pending: bool, width: usize) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let label = if pending {
        "Generating Link Code..."
    } else {
        "Generate Link Code"
    };
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::AMBER_DIM())
    };
    let trailing_style = if selected {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };
    let prefix = format!(" {marker} ");
    let used = prefix.chars().count() + label.chars().count();
    let trailing = " ".repeat(width.saturating_sub(used));
    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label.to_string(), label_style),
        Span::styled(trailing, trailing_style),
    ])
}

fn link_account_input_line(
    input: &ratatui_textarea::TextArea<'static>,
    placeholder: &str,
    pending: bool,
    focused: bool,
) -> Line<'static> {
    let typed = input.lines().join("");
    let text = if typed.is_empty() {
        placeholder.to_string()
    } else if pending {
        typed.clone()
    } else if focused {
        text_with_caret(&typed, input.cursor().1)
    } else {
        typed.clone()
    };
    let marker = if focused { "›" } else { " " };
    let prefix_style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let style = if typed.is_empty() && focused {
        Style::default()
            .fg(theme::TEXT_FAINT())
            .patch(theme::selection_style())
    } else if typed.is_empty() {
        Style::default().fg(theme::TEXT_FAINT())
    } else if focused {
        Style::default()
            .fg(theme::AMBER())
            .patch(theme::selection_style())
    } else {
        Style::default().fg(theme::AMBER())
    };
    Line::from(vec![
        Span::styled(format!(" {marker} "), prefix_style),
        Span::styled(text, style),
    ])
}

fn draw_link_account_status(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let Some(status) = state.link_account_dialog().status() else {
        return;
    };
    let dialog = state.link_account_dialog();
    let color = if dialog.pending() {
        theme::AMBER()
    } else if status == "Link code ready." || dialog.step() == LinkAccountStep::Pending {
        theme::SUCCESS()
    } else {
        theme::ERROR()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(status.to_string(), Style::default().fg(color)),
        ])),
        area,
    );
}

fn draw_link_account_footer(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let footer = match state.link_account_dialog().step() {
        LinkAccountStep::EnterCode => Line::from(vec![
            Span::raw(" "),
            Span::styled("↑↓", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" choose  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" generate/check  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]),
        LinkAccountStep::Confirm => Line::from(vec![
            Span::raw(" "),
            Span::styled("↑← / ↓→", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" choose main  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" link  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]),
        LinkAccountStep::Pending => Line::from(vec![
            Span::raw(" "),
            Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
        ]),
    };
    frame.render_widget(Paragraph::new(footer), area);
}

/// How many invitees the dialog lists before summing up the rest.
const INVITES_LISTED: usize = 8;

fn draw_invites_dialog(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let dialog = state.invites_dialog();
    let popup = centered_rect(76, 24, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Invites ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, state);
    let layout = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    let dim = Style::default().fg(theme::TEXT_DIM());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let Some(overview) = dialog.overview() else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled("Loading...", dim),
            ])),
            layout[0],
        );
        draw_invites_footer(frame, layout[2], dialog.accepts_code());
        return;
    };

    let mut code_line = None;
    let mut lines = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Send a friend this command:", dim),
        ]),
        Line::from(vec![
            Span::raw("   "),
            Span::styled(
                ssh_invite_command(&overview.code),
                bright.add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!(
                    "Once they become an active regular here, you get {} chips",
                    thousands(INVITER_REWARD_CHIPS)
                ),
                dim,
            ),
        ]),
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!(
                    "and they get a {} chip welcome bonus.",
                    thousands(INVITEE_BONUS_CHIPS)
                ),
                dim,
            ),
        ]),
        Line::raw(""),
    ];

    match (&overview.inviter, overview.can_attach) {
        (Some(inviter), _) => {
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled("Invited by ", dim),
                Span::styled(format!("@{inviter}"), bright),
            ]));
            lines.push(Line::raw(""));
        }
        (None, true) => {
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled("Did someone invite you? Enter their code:", dim),
            ]));
            code_line = Some(lines.len() as u16);
            lines.push(link_account_input_line(
                dialog.code_input(),
                "invite code",
                dialog.pending(),
                true,
            ));
            lines.push(Line::raw(""));
            lines.push(Line::raw(""));
        }
        (None, false) => {}
    }

    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("Your invites ({})", overview.invited.len()), dim),
    ]));
    if overview.invited.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled("Nobody yet.", Style::default().fg(theme::TEXT_FAINT())),
        ]));
    }
    for invited in overview.invited.iter().take(INVITES_LISTED) {
        let status_style = match invited.status {
            ReferralStatus::Paid => Style::default().fg(theme::SUCCESS()),
            ReferralStatus::Pending | ReferralStatus::Qualified => {
                Style::default().fg(theme::AMBER())
            }
            ReferralStatus::Expired => Style::default().fg(theme::TEXT_FAINT()),
        };
        lines.push(Line::from(vec![
            Span::raw("   "),
            // Padded to a column, with a space that survives a username
            // longer than the column (they run to 32).
            Span::styled(format!("@{:<20} ", invited.username), bright),
            Span::styled(invitee_status_label(invited.status), status_style),
        ]));
    }
    if overview.invited.len() > INVITES_LISTED {
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(
                format!("and {} more", overview.invited.len() - INVITES_LISTED),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), layout[0]);
    // The code field and its button sit on the input line and the blank line
    // under it; a pending check takes neither clicks nor typing.
    if let Some(y) = code_line.filter(|y| y + 1 < layout[0].height && !dialog.pending()) {
        let row = Rect::new(layout[0].x, layout[0].y + y, layout[0].width, 1);
        draw_text_field(
            frame,
            text_value_rect(row, 3),
            state,
            Field::InviteCode,
            dialog.code_input(),
        );
        button(
            frame,
            text_value_rect(
                Rect {
                    y: row.y + 1,
                    ..row
                },
                3,
            ),
            state,
            "[Add code]",
            Target::AddInviteCode,
        );
    }

    if let Some((message, is_error)) = dialog.message() {
        let color = if is_error {
            theme::ERROR()
        } else {
            theme::SUCCESS()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(message.to_string(), Style::default().fg(color)),
            ])),
            layout[1],
        );
    }
    draw_invites_footer(frame, layout[2], dialog.accepts_code());
}

fn draw_invites_footer(frame: &mut Surface<'_>, area: Rect, accepts_code: bool) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let footer = if accepts_code {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Enter", key),
            Span::styled(" or click add code  ", dim),
            Span::styled("Esc", key),
            Span::styled(" close", dim),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Enter/Esc", key),
            Span::styled(" close", dim),
        ])
    };
    frame.render_widget(Paragraph::new(footer), area);
}

fn draw_irc_token_dialog(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let dialog = state.irc_token_dialog();
    let popup_height = if dialog.revealed_token().is_some() {
        15
    } else {
        13
    };
    let popup = centered_rect(76, popup_height, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" IRC Access Token ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, state);

    draw_scroll(frame, inner, state, Pane::Irc, 13, 3, |f, a| {
        draw_irc_token_body(f, a, state)
    });
}

fn draw_irc_token_body(frame: &mut Surface<'_>, inner: Rect, state: &SettingsModalState) {
    let dialog = state.irc_token_dialog();
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    if let Some(token) = dialog.revealed_token() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "Save this token now. It will not be shown again.",
                    Style::default()
                        .fg(theme::SUCCESS())
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            layout[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    token.to_string(),
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            layout[2],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "Paste it into your IRC client's server password field.",
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ])),
            layout[4],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "Use any nick; late.sh forces your account username.",
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ])),
            layout[5],
        );
        draw_irc_token_message(frame, layout[6], state);
        let mut footer = layout[8];
        footer.x += footer.width.min(8);
        footer.width = footer.width.saturating_sub(8);
        draw_irc_token_footer(frame, footer, true, false);
        button(frame, layout[8], state, "[Done]", Target::DismissToken);
        return;
    }

    match dialog.status() {
        None => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("Loading...", Style::default().fg(theme::TEXT_DIM())),
                ])),
                layout[0],
            );
        }
        Some(None) => {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(
                            "No IRC token exists for this account.",
                            Style::default().fg(theme::TEXT_BRIGHT()),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(
                            "Create one to connect an IRC client to late.sh chat.",
                            Style::default().fg(theme::TEXT_DIM()),
                        ),
                    ]),
                ])
                .wrap(Wrap { trim: false }),
                layout[0],
            );
            frame.render_widget(
                Paragraph::new(irc_token_single_button_line(
                    "Create token",
                    true,
                    dialog.pending(),
                    layout[3].width as usize,
                )),
                layout[3],
            );
        }
        Some(Some(status)) => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("Active since ", Style::default().fg(theme::TEXT_DIM())),
                    Span::styled(
                        token_time_label(&status.created),
                        Style::default()
                            .fg(theme::TEXT_BRIGHT())
                            .add_modifier(Modifier::BOLD),
                    ),
                ])),
                layout[0],
            );
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("Last used ", Style::default().fg(theme::TEXT_DIM())),
                    Span::styled(
                        status
                            .last_used
                            .as_ref()
                            .map(token_time_label)
                            .unwrap_or_else(|| "never".to_string()),
                        Style::default().fg(theme::TEXT_BRIGHT()),
                    ),
                ])),
                layout[1],
            );
            frame.render_widget(
                Paragraph::new(irc_token_action_line(
                    dialog.focus(),
                    dialog.pending(),
                    layout[3].width as usize,
                )),
                layout[3],
            );
        }
    }

    if dialog.status().is_some() {
        state.mouse.hit(
            Rect::new(
                layout[3].x + 1,
                layout[3].y,
                if dialog.has_token() { 9 } else { 16 },
                layout[3].height,
            )
            .intersection(layout[3]),
            Target::Irc(IrcTokenFocus::Primary),
        );
        if dialog.has_token() {
            let rect = Rect::new(layout[3].x + 12, layout[3].y, 10, layout[3].height)
                .intersection(layout[3]);
            state.mouse.hit(rect, Target::Irc(IrcTokenFocus::Revoke));
            if dialog.confirming_revoke() {
                button(
                    frame,
                    layout[5],
                    state,
                    "[Confirm revoke]",
                    Target::Irc(IrcTokenFocus::Revoke),
                );
            }
        }
    }
    draw_irc_token_message(frame, layout[6], state);
    draw_irc_token_footer(frame, layout[8], false, dialog.pending());
}

fn draw_irc_token_message(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let Some(message) = state.irc_token_dialog().message() else {
        return;
    };
    let dialog = state.irc_token_dialog();
    let color = if dialog.pending() {
        theme::AMBER()
    } else if dialog.confirming_revoke() {
        theme::ERROR()
    } else {
        theme::SUCCESS()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(message.to_string(), Style::default().fg(color)),
        ])),
        area,
    );
}

fn draw_irc_token_footer(frame: &mut Surface<'_>, area: Rect, reveal: bool, pending: bool) {
    let footer = if reveal {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Enter/Space/Esc", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" hide token", Style::default().fg(theme::TEXT_DIM())),
        ])
    } else if pending {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Working...", Style::default().fg(theme::AMBER_DIM())),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("↑↓←→ j/k", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" choose  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Enter/Space", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" activate  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ])
    };
    frame.render_widget(Paragraph::new(footer), area);
}

fn irc_token_action_line(focus: IrcTokenFocus, pending: bool, width: usize) -> Line<'static> {
    let left = irc_token_button_span("Reset", focus == IrcTokenFocus::Primary, pending, false);
    let spacer = Span::raw("  ");
    let right = irc_token_button_span("Revoke", focus == IrcTokenFocus::Revoke, pending, true);
    let used = 2 + 9 + 2 + 10;
    Line::from(vec![
        Span::raw(" "),
        left,
        spacer,
        right,
        Span::raw(" ".repeat(width.saturating_sub(used))),
    ])
}

fn irc_token_single_button_line(
    label: &'static str,
    selected: bool,
    pending: bool,
    width: usize,
) -> Line<'static> {
    let button = irc_token_button_span(label, selected, pending, false);
    let used = 1 + label.chars().count() + 4;
    Line::from(vec![
        Span::raw(" "),
        button,
        Span::raw(" ".repeat(width.saturating_sub(used))),
    ])
}

fn irc_token_button_span(
    label: &'static str,
    selected: bool,
    pending: bool,
    destructive: bool,
) -> Span<'static> {
    let label = if pending && selected {
        format!("[ {label}... ]")
    } else {
        format!("[ {label} ]")
    };
    let fg = if destructive {
        theme::ERROR()
    } else {
        theme::TEXT_BRIGHT()
    };
    let style = if selected {
        Style::default()
            .fg(fg)
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else if destructive {
        Style::default().fg(theme::ERROR())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Span::styled(label, style)
}

fn token_time_label(time: &chrono::DateTime<chrono::Utc>) -> String {
    time.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn draw_delete_account_dialog(frame: &mut Surface<'_>, area: Rect, state: &SettingsModalState) {
    let popup = centered_rect(64, 12, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Delete Account ")
        .title_style(
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ERROR()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, state);

    draw_scroll(frame, inner, state, Pane::Delete, 10, 4, |f, a| {
        draw_delete_account_body(f, a, state)
    });
}

fn draw_delete_account_body(frame: &mut Surface<'_>, inner: Rect, state: &SettingsModalState) {
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "This cannot be undone.",
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Type your username to confirm:",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                state.draft().username.clone(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        layout[3],
    );

    let typed = state.delete_account_dialog().input().lines().join("");
    let input_text = if typed.is_empty() {
        "username".to_string()
    } else if state.delete_account_dialog().pending() {
        typed.clone()
    } else {
        format!("{typed}█")
    };
    let input_style = if typed.is_empty() {
        Style::default().fg(theme::TEXT_FAINT())
    } else {
        Style::default().fg(theme::AMBER())
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" › ", Style::default().fg(theme::AMBER_GLOW())),
            Span::styled(input_text, input_style),
        ])),
        layout[4],
    );

    draw_text_field(
        frame,
        text_value_rect(layout[4], 3),
        state,
        Field::DeleteConfirm,
        state.delete_account_dialog().input(),
    );
    button(
        frame,
        layout[7],
        state,
        "[Delete account]",
        Target::ConfirmDelete,
    );
    if let Some(status) = state.delete_account_dialog().status() {
        let color = if state.delete_account_dialog().pending() {
            theme::AMBER()
        } else {
            theme::ERROR()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(status.to_string(), Style::default().fg(color)),
            ])),
            layout[5],
        );
    }

    let footer = Line::from(vec![
        Span::raw(" "),
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" delete  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[7]);
    button(
        frame,
        layout[7],
        state,
        "[Delete account]",
        Target::ConfirmDelete,
    );
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  ── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
}

struct ValueSpan {
    text: String,
    style: Style,
}

fn value_span(text: impl Into<String>, color: ratatui::style::Color) -> ValueSpan {
    ValueSpan {
        text: text.into(),
        style: Style::default().fg(color),
    }
}

fn text_with_caret(text: &str, cursor_col: usize) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    chars.insert(cursor_col.min(chars.len()), '█');
    chars.into_iter().collect()
}

fn system_field_value(state: &SettingsModalState, row: Row, value: Option<String>) -> ValueSpan {
    if state.editing_system_row(row) {
        let typed = state.system_input().lines().join("");
        if typed.is_empty() {
            value_span("█", theme::AMBER())
        } else {
            value_span(
                text_with_caret(&typed, state.system_input().cursor().1),
                theme::AMBER(),
            )
        }
    } else {
        match value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(value) => value_span(value.to_string(), theme::TEXT_BRIGHT()),
            None => value_span("not set", theme::TEXT_FAINT()),
        }
    }
}

fn format_lang_tags(langs: &[String]) -> String {
    langs
        .iter()
        .map(|lang| format!("#{lang}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn toggle_span(enabled: bool) -> ValueSpan {
    if enabled {
        ValueSpan {
            text: "● on".to_string(),
            style: Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        }
    } else {
        ValueSpan {
            text: "○ off".to_string(),
            style: Style::default().fg(theme::TEXT_FAINT()),
        }
    }
}

fn interaction_mode_span(mode: late_core::models::user::InteractionMode) -> ValueSpan {
    picker_value_span(
        super::state::interaction_mode_label(mode),
        &["Keyboard", "Mouse", "Hybrid"],
    )
}

fn right_sidebar_mode_span(mode: RightSidebarMode) -> ValueSpan {
    cycle_value_span(
        match mode {
            RightSidebarMode::On => "On",
            RightSidebarMode::Off => "Off",
            RightSidebarMode::Auto => "Auto",
        },
        &["On", "Off", "Auto"],
    )
}

fn translate_to_span(lang: late_core::models::message_translation::TranslateLang) -> ValueSpan {
    let choices: Vec<_> = late_core::models::message_translation::TranslateLang::ALL
        .iter()
        .map(|lang| lang.label())
        .collect();
    picker_value_span(lang.label(), &choices)
}

fn picker_value_span(label: &str, choices: &[&str]) -> ValueSpan {
    let width = choices
        .iter()
        .map(|label| Span::raw(*label).width())
        .max()
        .unwrap_or(0);
    ValueSpan {
        text: format!(
            "{label}  …{}",
            " ".repeat(width.saturating_sub(Span::raw(label).width()))
        ),
        style: Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    }
}

fn cooldown_span(mins: i32) -> ValueSpan {
    let label = if mins == 0 {
        "off".to_string()
    } else {
        format!("{mins} min")
    };
    cycle_value_span(&label, &["off", "240 min"])
}

fn notify_format_span(format: Option<&str>) -> ValueSpan {
    cycle_value_span(
        notify_format_label(format),
        &["Both (777 + 9)", "OSC 777", "OSC 9"],
    )
}

/// The "Chat badges" row: how many badge rows are hidden, Enter to edit.
fn chat_badges_span(state: &SettingsModalState) -> ValueSpan {
    let rows = late_core::models::profile_award::chat_badge_rows();
    let hidden = rows
        .iter()
        .filter(|row| !state.chat_badge_row_shown(row))
        .count();
    let text = match hidden {
        0 => "all shown ↵".to_string(),
        hidden => format!("{hidden} hidden ↵"),
    };
    ValueSpan {
        text,
        style: Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    }
}

/// The "Terminal images" row: auto-detect, or force previews off or to sixel.
fn terminal_images_span(mode: late_core::models::user::TerminalImagesMode) -> ValueSpan {
    use late_core::models::user::TerminalImagesMode;
    let label = match mode {
        TerminalImagesMode::Auto => "Auto",
        TerminalImagesMode::Off => "Off",
        TerminalImagesMode::Sixel => "Sixel",
    };
    cycle_value_span(label, &["Auto", "Off", "Sixel"])
}

/// The "Land on" row: the page a session opens on, cycled with the arrows.
fn landing_page_span(page: late_core::models::user::LandingPage) -> ValueSpan {
    use late_core::models::user::LandingPage;
    let label = match page {
        LandingPage::Clubhouse => "Clubhouse",
        LandingPage::Home => "Home",
        LandingPage::Zen => "Zen",
    };
    cycle_value_span(label, &["Clubhouse", "Home", "Zen"])
}

fn cycle_value_span(label: &str, choices: &[&str]) -> ValueSpan {
    let width = choices
        .iter()
        .map(|label| Span::raw(*label).width())
        .max()
        .unwrap_or(0);
    let padding = " ".repeat(width.saturating_sub(Span::raw(label).width()));
    ValueSpan {
        text: format!("◂ {label}{padding} ▸"),
        style: Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    }
}

/// The room-list rail row. Mirrors `right_sidebar_mode_span` without the panel
/// editor affordance: the rail has no panel list of its own.
fn room_list_mode_span(mode: RoomListMode) -> ValueSpan {
    cycle_value_span(
        match mode {
            RoomListMode::On => "On",
            RoomListMode::Off => "Off",
            RoomListMode::Auto => "Auto",
        },
        &["On", "Off", "Auto"],
    )
}

fn text_brightness_span(adjustment: i32) -> ValueSpan {
    let adjustment = adjustment.clamp(-5, 5);
    let text = match adjustment {
        -5 => "-5 darker",
        -4 => "-4 darker",
        -3 => "-3 darker",
        -2 => "-2 darker",
        -1 => "-1 darker",
        0 => "neutral",
        1 => "+1 lighter",
        2 => "+2 lighter",
        3 => "+3 lighter",
        4 => "+4 lighter",
        5 => "+5 lighter",
        _ => unreachable!(),
    };
    cycle_value_span(text, &["-5 darker", "neutral", "+5 lighter"])
}

fn value_with_picker_hint(text: String) -> ValueSpan {
    ValueSpan {
        text: format!("{text}  …"),
        style: Style::default().fg(theme::TEXT_BRIGHT()),
    }
}

fn row_line(
    state: &SettingsModalState,
    row: Row,
    width: usize,
    label: &str,
    value: ValueSpan,
) -> Line<'static> {
    let selected = state.selected_row() == row
        && !state.editing_username()
        && state.editing_system_field().is_none()
        && !state.editing_bio();

    let marker = if selected { "›" } else { " " };
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = if selected {
        value.style.patch(theme::selection_style())
    } else {
        value.style
    };

    let prefix = format!(" {marker} ");
    let desired = (Span::raw(label).width() + 1).max(16);
    let label_width = if (row == Row::Username && state.editing_username())
        || (state.editing_system_field().is_some() && state.editing_system_row(row))
    {
        16 // Editable fields render their horizontally scrolled viewport at column 19.
    } else {
        desired.min(
            width
                .saturating_sub(Span::raw(&prefix).width() + Span::raw(&value.text).width().max(5)),
        )
    };
    let label_text = fit_label(label, label_width);
    let mut used = Span::raw(&prefix).width()
        + Span::raw(&label_text).width()
        + Span::raw(&value.text).width();
    if used > width {
        used = width;
    }
    let padding = width.saturating_sub(used);
    let trailing = " ".repeat(padding);
    let trailing_style = if selected {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label_text, label_style),
        Span::styled(value.text, value_style),
        Span::styled(trailing, trailing_style),
    ])
}

/// Labels yield cells before complete controls do. Ratatui supplies both the
/// grapheme boundaries and the same display measurements used for mouse hits.
fn fit_label(label: &str, width: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for grapheme in Line::raw(label).styled_graphemes(Style::default()) {
        let cells = Span::raw(grapheme.symbol).width();
        if used + cells > width.saturating_sub(1) {
            break;
        }
        result.push_str(grapheme.symbol);
        used += cells;
    }
    result.push_str(&" ".repeat(width.saturating_sub(used)));
    result
}

fn choice_hits(
    state: &SettingsModalState,
    row: Rect,
    line: &Line<'_>,
    ordinary: Target,
    backward: Target,
    forward: Target,
) {
    let x = line.spans.iter().take(2).map(Span::width).sum::<usize>() as u16;
    let width = line.spans[2].width() as u16;
    let ordinary_rect = if ordinary == Target::SidebarMode {
        Rect::new(row.x + x + 2, row.y, width.saturating_sub(4), 1)
    } else {
        Rect::new(row.x, row.y, x + width, 1)
    };
    state.mouse.hit(ordinary_rect.intersection(row), ordinary);
    for (offset, target) in [(x, backward), (x + width.saturating_sub(2), forward)] {
        state.mouse.hit(
            Rect::new(row.x + offset, row.y, 2, 1).intersection(row),
            target,
        );
    }
}

fn pad_to_width(text: &str, width: usize, _has_bg: bool) -> String {
    let len = Span::raw(text).width();
    if len >= width {
        return text.to_string();
    }
    let mut out = String::from(text);
    out.push_str(&" ".repeat(width - len));
    out
}

fn has_kind(state: &SettingsModalState, kind: &str) -> bool {
    state.draft().notify_kinds.iter().any(|value| value == kind)
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
