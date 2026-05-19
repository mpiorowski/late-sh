use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{CatMood, CatState};
use crate::app::common::theme;

pub fn draw_cat_inline(frame: &mut Frame, area: Rect, state: &CatState) {
    if area.height < 3 || area.width < 8 {
        return;
    }

    let mood = state.mood();
    let eyes = mood.eyes();
    let color = mood_color(mood);

    let mut lines: Vec<Line<'_>> = vec![
        Line::from(Span::styled(" /\\_/\\ ", Style::default().fg(color))),
        Line::from(Span::styled(
            format!("( {} )", eyes),
            Style::default().fg(color),
        )),
        Line::from(Span::styled(" >   < ", Style::default().fg(color))),
    ];

    if area.height >= 4 {
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
                "k care",
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::ITALIC),
            ));
        }
        lines.push(Line::from(footer));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn mood_color(mood: CatMood) -> Color {
    match mood {
        CatMood::Happy => theme::AMBER(),
        CatMood::Content => theme::TEXT_BRIGHT(),
        CatMood::Bored | CatMood::Hungry | CatMood::Thirsty | CatMood::Sad => theme::TEXT_DIM(),
    }
}
