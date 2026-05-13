use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::svc::{Genre, VoteCount};
use crate::app::common::theme;

pub struct VoteCardView<'a> {
    pub vote_counts: &'a VoteCount,
    pub current_genre: Genre,
    pub my_vote: Option<Genre>,
    pub ends_in: Duration,
}

pub fn draw_vote_card(frame: &mut Frame, area: Rect, view: &VoteCardView<'_>) {
    let block = Block::default()
        .title(" Vote Next (v1/v2/v3) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    draw_vote_options(frame, inner, view.vote_counts, view.my_vote);
}

pub fn draw_vote_options(
    frame: &mut Frame,
    area: Rect,
    vote_counts: &VoteCount,
    my_vote: Option<Genre>,
) {
    let options = [
        (
            "v1",
            "Lofi",
            &vote_counts.lofi,
            my_vote == Some(Genre::Lofi),
        ),
        (
            "v2",
            "Ambient",
            &vote_counts.ambient,
            my_vote == Some(Genre::Ambient),
        ),
        (
            "v3",
            "Classic",
            &vote_counts.classic,
            my_vote == Some(Genre::Classic),
        ),
        // ("Z", "Jazz", &vote_counts.jazz, my_vote == Some(Genre::Jazz)),
    ];
    let total_votes = vote_counts.total();
    let max_bar_width = area.width.saturating_sub(20) as usize;

    let layout = Layout::vertical(vec![Constraint::Length(1); options.len()]).split(area);

    for (i, (key, name, votes, is_voted)) in options.iter().enumerate() {
        let bar_filled = if total_votes > 0 {
            (**votes as usize * max_bar_width) / total_votes as usize
        } else {
            0
        };
        let bar_empty = max_bar_width.saturating_sub(bar_filled);

        let mut spans = vec![
            Span::styled(
                format!(" {} ", key),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{:<8}", name), Style::default().fg(theme::TEXT())),
            Span::styled(
                "█".repeat(bar_filled),
                Style::default().fg(if *is_voted {
                    theme::SUCCESS()
                } else {
                    theme::AMBER_DIM()
                }),
            ),
            Span::styled("░".repeat(bar_empty), Style::default().fg(theme::BORDER())),
            Span::styled(
                format!(" {:>3}", votes),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ];

        if *is_voted {
            spans.push(Span::styled(" ✓", Style::default().fg(theme::SUCCESS())));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), layout[i]);
    }
}

/// Borderless, label-less vote rows for the merged-shell stream block.
/// Renders 3 short lines: `lofi    ████   12  v1`. Hint key sits at the row's
/// right edge to match the b1/b2/b3 layout in active tables. Active vote is
/// sage, everything else dim. No section header — caller owns that.
pub fn draw_vote_inline(frame: &mut Frame, area: Rect, view: &VoteCardView<'_>) {
    let options = [
        (
            "v1",
            "lofi",
            &view.vote_counts.lofi,
            view.my_vote == Some(Genre::Lofi),
        ),
        (
            "v2",
            "ambient",
            &view.vote_counts.ambient,
            view.my_vote == Some(Genre::Ambient),
        ),
        (
            "v3",
            "classic",
            &view.vote_counts.classic,
            view.my_vote == Some(Genre::Classic),
        ),
    ];
    let total = view.vote_counts.total().max(1) as usize;
    let max_bar = (area.width as usize).saturating_sub(14).max(1);

    let layout = Layout::vertical(vec![Constraint::Length(1); options.len()]).split(area);
    for (i, (key, name, votes, mine)) in options.iter().enumerate() {
        let filled = (**votes as usize * max_bar) / total;
        let empty = max_bar.saturating_sub(filled);

        let name_color = if *mine {
            theme::SUCCESS()
        } else {
            theme::TEXT()
        };
        let bar_color = if *mine {
            theme::SUCCESS()
        } else {
            theme::AMBER_DIM()
        };

        let spans = vec![
            Span::styled(format!("{:<8}", name), Style::default().fg(name_color)),
            Span::styled("█".repeat(filled), Style::default().fg(bar_color)),
            Span::styled("·".repeat(empty), Style::default().fg(theme::BORDER_DIM())),
            Span::styled(
                format!(" {:>2}", votes),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
            Span::raw(" "),
            Span::styled(
                key.to_string(),
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        frame.render_widget(Paragraph::new(Line::from(spans)), layout[i]);
    }
}

/// Sidebar-flavored vote card sized for the 24-col right rail.
/// Keeps the same `VoteCardView` data; renders 3 active genres + a header rule.
pub fn draw_vote_sidebar(frame: &mut Frame, area: Rect, view: &VoteCardView<'_>) {
    let block = Block::default()
        .title(" Vote ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    draw_vote_sidebar_options(frame, inner, view.vote_counts, view.my_vote);
}

fn draw_vote_sidebar_options(
    frame: &mut Frame,
    area: Rect,
    vote_counts: &VoteCount,
    my_vote: Option<Genre>,
) {
    let options = [
        (
            "v1",
            "Lofi",
            &vote_counts.lofi,
            my_vote == Some(Genre::Lofi),
        ),
        (
            "v2",
            "Ambient",
            &vote_counts.ambient,
            my_vote == Some(Genre::Ambient),
        ),
        (
            "v3",
            "Classic",
            &vote_counts.classic,
            my_vote == Some(Genre::Classic),
        ),
    ];
    let total_votes = vote_counts.total();
    // 14 = key(3) + name(7) + count(4)
    let max_bar_width = (area.width as usize).saturating_sub(14);

    let layout = Layout::vertical(vec![Constraint::Length(1); options.len()]).split(area);

    for (i, (key, name, votes, is_voted)) in options.iter().enumerate() {
        let bar_filled = if total_votes > 0 && max_bar_width > 0 {
            (**votes as usize * max_bar_width) / total_votes as usize
        } else {
            0
        };
        let bar_empty = max_bar_width.saturating_sub(bar_filled);

        let spans = vec![
            Span::styled(
                format!(" {} ", key),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{:<7}", name), Style::default().fg(theme::TEXT())),
            Span::styled(
                "█".repeat(bar_filled),
                Style::default().fg(if *is_voted {
                    theme::SUCCESS()
                } else {
                    theme::AMBER_DIM()
                }),
            ),
            Span::styled("░".repeat(bar_empty), Style::default().fg(theme::BORDER())),
            Span::styled(
                format!(" {:>3}", votes),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ];

        frame.render_widget(Paragraph::new(Line::from(spans)), layout[i]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn draw_vote_options_includes_all_genres_in_totals() {
        let counts = VoteCount {
            lofi: 1,
            classic: 1,
            ambient: 1,
            jazz: 2,
        };

        let backend = TestBackend::new(40, 5);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 40, 5);
                draw_vote_options(frame, area, &counts, None);
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let line = |y: u16| -> String {
            let mut out = String::new();
            for x in 0..40 {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.trim_end().to_string()
        };

        let lofi_line = line(0);
        let ambient_line = line(1);
        let classic_line = line(2);

        let count_blocks = |s: &str| s.chars().filter(|c| *c == '█').count();
        assert!(lofi_line.contains("Lofi"));
        assert!(classic_line.contains("Classic"));
        assert!(ambient_line.contains("Ambient"));
        // Jazz voting disabled
        // total_votes=5 (jazz still counted in total), each visible has 1 vote → 1*20/5 = 4
        assert_eq!(count_blocks(&lofi_line), 4);
        assert_eq!(count_blocks(&classic_line), 4);
        assert_eq!(count_blocks(&ambient_line), 4);
    }
}
