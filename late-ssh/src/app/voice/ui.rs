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

    pub fn participant_count(&self) -> usize {
        self.snapshot.participants.len()
    }
}

pub fn draw_voice_room(frame: &mut Frame, area: Rect, view: &VoiceRoomView<'_>) {
    let connected = view.participant_count();
    let title = if connected == 1 {
        format!(" Voice #{} · 1 connected ", view.snapshot.room_name)
    } else {
        format!(
            " Voice #{} · {} connected ",
            view.snapshot.room_name, connected
        )
    };
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
        "Voice is not configured.".to_string()
    } else if !view.paired_cli_supports_voice() {
        "Run the native late CLI to join voice.".to_string()
    } else if let Some(participant) = view.snapshot.participant(view.current_user_id) {
        let presence = Presence::of(participant);
        format!(
            "{} {} · ⏎ leave · 🎤 u mic · 🎧 d deafen",
            presence.icon(),
            presence.label()
        )
    } else {
        "🔇 not joined · ⏎ join muted".to_string()
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

/// A participant's live presence, in priority order: a deafened user can't hear
/// (so it outranks muted), a muted user isn't transmitting (outranks speaking),
/// otherwise they're either actively speaking or just listening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Presence {
    Deafened,
    Muted,
    Speaking,
    Listening,
}

impl Presence {
    fn of(participant: &VoiceParticipant) -> Self {
        if participant.deafened {
            Self::Deafened
        } else if participant.muted {
            Self::Muted
        } else if participant.speaking {
            Self::Speaking
        } else {
            Self::Listening
        }
    }

    /// Status icon shown before the name. Green/white dots mirror the familiar
    /// "live light" convention; the slashed speaker/bell read as mic/ears off.
    fn icon(self) -> &'static str {
        match self {
            Self::Speaking => "🟢",
            Self::Listening => "⚪",
            Self::Muted => "🔇",
            Self::Deafened => "🔕",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Speaking => "speaking",
            Self::Listening => "listening",
            Self::Muted => "muted",
            Self::Deafened => "deafened",
        }
    }

    fn color(self) -> ratatui::style::Color {
        match self {
            Self::Speaking => theme::SUCCESS(),
            Self::Listening => theme::TEXT_DIM(),
            Self::Muted => theme::AMBER(),
            Self::Deafened => theme::ERROR(),
        }
    }
}

fn participant_line(participant: &VoiceParticipant, current_user: bool) -> Line<'static> {
    let presence = Presence::of(participant);
    // The name pops green+bold while a user is actively speaking (the live
    // indicator); the current user is always amber so you can find yourself.
    let name_style = if current_user {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else if presence == Presence::Speaking {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    Line::from(vec![
        Span::styled(
            format!("{} ", presence.icon()),
            Style::default().fg(presence.color()),
        ),
        Span::styled(format!("@{}", participant.username), name_style),
        Span::styled("  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            presence.label().to_string(),
            Style::default().fg(presence.color()),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn participant(muted: bool, deafened: bool, speaking: bool) -> VoiceParticipant {
        VoiceParticipant {
            user_id: Uuid::nil(),
            username: "tester".to_string(),
            muted,
            deafened,
            speaking,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn presence_priority_is_deafened_then_muted_then_speaking() {
        // Deafened outranks everything, even an erroneously-set speaking flag.
        assert_eq!(Presence::of(&participant(true, true, true)), Presence::Deafened);
        // Muted outranks speaking.
        assert_eq!(Presence::of(&participant(true, false, true)), Presence::Muted);
        // Speaking shows over plain listening.
        assert_eq!(
            Presence::of(&participant(false, false, true)),
            Presence::Speaking
        );
        // Joined, mic on, silent => listening.
        assert_eq!(
            Presence::of(&participant(false, false, false)),
            Presence::Listening
        );
    }

    #[test]
    fn every_presence_has_a_distinct_icon_and_label() {
        let all = [
            Presence::Speaking,
            Presence::Listening,
            Presence::Muted,
            Presence::Deafened,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a.icon(), b.icon(), "icons must be distinct");
                assert_ne!(a.label(), b.label(), "labels must be distinct");
            }
        }
    }
}
