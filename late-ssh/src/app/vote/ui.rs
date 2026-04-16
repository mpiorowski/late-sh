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
    pub my_vote: Option<Genre>,
}

pub fn draw_vote_card(frame: &mut Frame, area: Rect, view: &VoteCardView<'_>) {
    let block = Block::default()
        .title(" Vote Next (L/A/C) ")
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
        ("L", "Lofi", &vote_counts.lofi, my_vote == Some(Genre::Lofi)),
        (
            "A",
            "Ambient",
            &vote_counts.ambient,
            my_vote == Some(Genre::Ambient),
        ),
        (
            "C",
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
