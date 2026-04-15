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
        .title(vote_card_title(area.width))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER));
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
    if area.width == 0 || area.height == 0 {
        return;
    }

    let compact_mode = area.width < 15;
    let padded_compact_mode = (15..=16).contains(&area.width);
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

    for (slot, (key, name, votes, is_voted)) in layout.iter().zip(options.iter()) {
        let bar_filled = if total_votes > 0 {
            (**votes as usize * max_bar_width) / total_votes as usize
        } else {
            0
        };
        let bar_empty = max_bar_width.saturating_sub(bar_filled);

        let mut spans = if compact_mode || padded_compact_mode {
            compact_vote_spans(
                key,
                name,
                **votes,
                *is_voted,
                area.width as usize,
                padded_compact_mode,
            )
        } else {
            vec![
                Span::styled(
                    format!(" {} ", key),
                    Style::default()
                        .fg(theme::AMBER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<8}", name), Style::default().fg(theme::TEXT)),
                Span::styled(
                    "█".repeat(bar_filled),
                    Style::default().fg(if *is_voted {
                        theme::SUCCESS
                    } else {
                        theme::AMBER_DIM
                    }),
                ),
                Span::styled("░".repeat(bar_empty), Style::default().fg(theme::BORDER)),
                Span::styled(
                    format!(" {:>3}", votes),
                    Style::default().fg(theme::TEXT_DIM),
                ),
            ]
        };

        if *is_voted && !(compact_mode || padded_compact_mode) {
            spans.push(Span::styled(" ✓", Style::default().fg(theme::SUCCESS)));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), *slot);
    }
}

fn compact_vote_spans<'a>(
    key: &'a str,
    name: &'a str,
    votes: i64,
    is_voted: bool,
    available_width: usize,
    leading_pad: bool,
) -> Vec<Span<'a>> {
    let rest = &name[key.len()..];
    let votes_text = votes.to_string();
    let leading_width = usize::from(leading_pad);
    let indicator_width = votes_text.len() + 1;
    let name_width = available_width
        .saturating_sub(indicator_width + leading_width)
        .max(key.len());
    let rest_width = name_width.saturating_sub(key.len());

    let mut spans = Vec::with_capacity(5);
    if leading_pad {
        spans.push(Span::raw(" "));
    }
    spans.extend([
        Span::styled(
            key,
            Style::default()
                .fg(theme::AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{rest:<rest_width$}"),
            Style::default().fg(theme::TEXT),
        ),
        Span::styled(votes_text, Style::default().fg(theme::TEXT_DIM)),
    ]);

    if is_voted {
        spans.push(Span::styled("✓", Style::default().fg(theme::SUCCESS)));
    } else {
        spans.push(Span::raw(" "));
    }

    spans
}

fn vote_card_title(width: u16) -> &'static str {
    match width {
        20.. => " Vote Next (L/A/C) ",
        14..=18 => " Vote Next ",
        13 => " Vote Next",
        _ => "Vote Next",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };

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

    #[test]
    fn draw_vote_options_compact_mode_accents_initial_letters() {
        let counts = VoteCount {
            lofi: 5,
            classic: 2,
            ambient: 2,
            jazz: 0,
        };

        let backend = TestBackend::new(12, 4);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 12, 4);
                draw_vote_options(frame, area, &counts, Some(Genre::Lofi));
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let line = |y: u16| -> String {
            let mut out = String::new();
            for x in 0..12 {
                out.push_str(buffer[(x, y)].symbol());
            }
            out
        };

        assert_eq!(line(0), "Lofi      5✓");
        assert_eq!(line(1), "Ambient   2 ");
        assert_eq!(line(2), "Classic   2 ");

        for (x, y, ch) in [(0, 0, "L"), (0, 1, "A"), (0, 2, "C")] {
            assert_eq!(buffer[(x, y)].symbol(), ch);
            assert_eq!(buffer[(x, y)].fg, theme::AMBER);
            assert!(buffer[(x, y)].modifier.contains(Modifier::BOLD));
        }

        assert_eq!(buffer[(1, 0)].symbol(), "o");
        assert_eq!(buffer[(1, 0)].fg, theme::TEXT);
        assert_eq!(buffer[(10, 0)].symbol(), "5");
        assert_eq!(buffer[(10, 0)].fg, theme::TEXT_DIM);
        assert_eq!(buffer[(11, 0)].symbol(), "✓");
        assert_eq!(buffer[(11, 0)].fg, theme::SUCCESS);

        let compact_first_char_style = buffer[(0, 0)].style();
        assert_eq!(compact_first_char_style.fg, Some(Color::Rgb(184, 120, 44)));
        assert!(compact_first_char_style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn draw_vote_options_padded_compact_mode_keeps_left_padding() {
        let counts = VoteCount {
            lofi: 0,
            classic: 0,
            ambient: 0,
            jazz: 0,
        };

        let backend = TestBackend::new(15, 4);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 15, 4);
                draw_vote_options(frame, area, &counts, None);
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].symbol(), " ");
        assert_eq!(buffer[(1, 0)].symbol(), "L");
        assert_eq!(buffer[(1, 1)].symbol(), "A");
        assert_eq!(buffer[(1, 2)].symbol(), "C");
    }

    #[test]
    fn vote_card_title_shortens_as_width_shrinks() {
        assert_eq!(vote_card_title(19), " Vote Next (L/A/C) ");
        assert_eq!(vote_card_title(18), " Vote Next ");
        assert_eq!(vote_card_title(13), " Vote Next");
        assert_eq!(vote_card_title(12), "Vote Next");
    }
}
