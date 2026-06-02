use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    voice::svc::{VoiceParticipant, VoiceSnapshot},
};

pub struct VoiceRoomView<'a> {
    pub snapshot: &'a VoiceSnapshot,
    pub current_user_id: Uuid,
    pub paired_cli_supports_voice: bool,
    pub browser_listen_url: &'a str,
}

impl VoiceRoomView<'_> {
    pub fn current_user_joined(&self) -> bool {
        self.snapshot.participant(self.current_user_id).is_some()
    }

    pub fn paired_cli_supports_voice(&self) -> bool {
        self.paired_cli_supports_voice
    }
}

pub fn draw_voice_room(frame: &mut Frame, area: Rect, view: &VoiceRoomView<'_>) {
    let title = format!(" Voice #{} ", view.snapshot.room_name);
    let block = Block::default()
        .title(title)
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    if !view.snapshot.enabled {
        lines.push(Line::from(Span::styled(
            "Voice is off on this server.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::styled(
                "Browser listen-only: ",
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(
                view.browser_listen_url.to_string(),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        if view.snapshot.participants.is_empty() {
            lines.push(Line::from(Span::styled(
                "No one is in voice.",
                Style::default().fg(theme::TEXT_DIM()),
            )));
        } else {
            for participant in &view.snapshot.participants {
                lines.push(participant_line(
                    participant,
                    participant.user_id == view.current_user_id,
                ));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

pub fn draw_voice_controls(frame: &mut Frame, area: Rect, view: &VoiceRoomView<'_>) {
    let border = if view.current_user_joined() {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let block = Block::default()
        .title(" Voice ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    let text = if !view.snapshot.enabled {
        "Voice is not configured."
    } else if !view.paired_cli_supports_voice() {
        "Run the native late CLI to join voice."
    } else if view.current_user_joined() {
        "Enter leave · u mute mic · d deafen"
    } else {
        "Enter join muted · u mute mic · d deafen"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {text}"),
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .block(block),
        area,
    );
}

fn participant_line(participant: &VoiceParticipant, current_user: bool) -> Line<'static> {
    let name_style = if current_user {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let status = if participant.deafened {
        "deafened"
    } else if participant.muted {
        "muted"
    } else if participant.speaking {
        "speaking"
    } else {
        "listening"
    };
    Line::from(vec![
        Span::styled("@", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(participant.username.clone(), name_style),
        Span::styled("  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(status.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}
