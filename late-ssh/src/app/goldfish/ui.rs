use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{GoldfishMood, GoldfishState};
use crate::app::common::theme;

pub fn draw_goldfish_inline(frame: &mut Frame, area: Rect, state: &GoldfishState) {
    if area.height < 3 || area.width < 8 {
        return;
    }

    let mood = state.mood();
    let color = mood_color(mood);
    let fish = fish_art(mood);

    let mut lines: Vec<Line<'_>> = Vec::new();

    // Bubbles row — only when happy
    if area.height >= 4 {
        let bubbles = if mood == GoldfishMood::Happy {
            Span::styled(" o  o  ", Style::default().fg(theme::TEXT_FAINT()))
        } else {
            Span::raw("       ")
        };
        lines.push(Line::from(bubbles));
    }

    lines.push(Line::from(Span::styled(fish, Style::default().fg(color))));
    lines.push(Line::from(Span::styled(
        "~~~~~~~",
        Style::default().fg(theme::TEXT_FAINT()),
    )));

    // Footer
    let mut footer: Vec<Span<'_>> = vec![Span::styled(
        mood.label(),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    if let Some(fb) = state.action_feedback {
        footer.push(Span::raw("  "));
        footer.push(Span::styled(
            fb,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::ITALIC),
        ));
    } else {
        footer.push(Span::raw("  "));
        footer.push(Span::styled(
            "g care",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC),
        ));
    }
    lines.push(Line::from(footer));

    frame.render_widget(Paragraph::new(lines), area);
}

fn fish_art(mood: GoldfishMood) -> String {
    format!("><((({}> ", mood.eye())
}

fn mood_color(mood: GoldfishMood) -> Color {
    match mood {
        GoldfishMood::Happy => theme::AMBER(),
        GoldfishMood::Content => theme::TEXT_BRIGHT(),
        GoldfishMood::Bored | GoldfishMood::Hungry | GoldfishMood::Dirty | GoldfishMood::Sad => {
            theme::TEXT_DIM()
        }
    }
}
