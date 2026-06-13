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

/// Fixed height of the inline voice strip drawn at the top of a voice-enabled
/// room: one border/title row, one roster row, one controls row. Constant so
/// the chrome below it never shifts as people join and leave.
pub const VOICE_STRIP_HEIGHT: u16 = 3;

pub struct VoiceRoomView<'a> {
    pub snapshot: &'a VoiceSnapshot,
    pub room_id: Uuid,
    pub current_user_id: Uuid,
    pub paired_cli_supports_voice: bool,
    pub browser_listen_url: &'a str,
}

impl VoiceRoomView<'_> {
    fn participants(&self) -> &[VoiceParticipant] {
        self.snapshot.participants(self.room_id)
    }

    pub fn current_user_joined(&self) -> bool {
        self.snapshot
            .participant(self.room_id, self.current_user_id)
            .is_some()
    }

    pub fn paired_cli_supports_voice(&self) -> bool {
        self.paired_cli_supports_voice
    }

    pub fn participant_count(&self) -> usize {
        self.participants().len()
    }
}

/// Draw the inline voice channel strip at the top of a voice-enabled room: a
/// title row with the live count, the roster of who is connected, and the
/// controls line. Sized to exactly `VOICE_STRIP_HEIGHT`.
pub fn draw_voice_strip(frame: &mut Frame, area: Rect, view: &VoiceRoomView<'_>) {
    let connected = view.participant_count();
    let title = match connected {
        0 => " Voice ".to_string(),
        1 => " Voice · 1 live ".to_string(),
        n => format!(" Voice · {n} live "),
    };
    let border = if view.current_user_joined() {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let roster = if !view.snapshot.enabled {
        Line::from(Span::styled(
            "Voice is off on this server.",
            Style::default().fg(theme::TEXT_DIM()),
        ))
    } else if view.participants().is_empty() {
        Line::from(Span::styled(
            "No one is in voice yet.",
            Style::default().fg(theme::TEXT_DIM()),
        ))
    } else {
        let mut spans = Vec::new();
        for participant in view.participants() {
            if !spans.is_empty() {
                spans.push(Span::styled("  ", Style::default().fg(theme::TEXT_DIM())));
            }
            spans.extend(participant_spans(
                participant,
                participant.user_id == view.current_user_id,
            ));
        }
        Line::from(spans)
    };

    let controls = Line::from(Span::styled(
        voice_controls_text(view),
        Style::default().fg(theme::TEXT_DIM()),
    ));

    frame.render_widget(Paragraph::new(vec![roster, controls]), inner);
}

fn voice_controls_text(view: &VoiceRoomView<'_>) -> String {
    if !view.snapshot.enabled {
        return "Voice is not configured.".to_string();
    }
    if !view.paired_cli_supports_voice() {
        return format!(
            "Run the native late CLI to join · listen: {}?room={}",
            view.browser_listen_url, view.room_id
        );
    }
    if let Some(participant) = view
        .snapshot
        .participant(view.room_id, view.current_user_id)
    {
        let presence = Presence::of(participant);
        format!(
            "{} {} · Ctrl+V leave · Ctrl+T mic · /voice /mute",
            presence.icon(),
            presence.label()
        )
    } else {
        "🔇 not joined · Ctrl+V join muted · /voice".to_string()
    }
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

fn participant_spans(participant: &VoiceParticipant, current_user: bool) -> Vec<Span<'static>> {
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
    vec![
        Span::styled(
            format!("{} ", presence.icon()),
            Style::default().fg(presence.color()),
        ),
        Span::styled(format!("@{}", participant.username), name_style),
    ]
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
        assert_eq!(
            Presence::of(&participant(true, true, true)),
            Presence::Deafened
        );
        // Muted outranks speaking.
        assert_eq!(
            Presence::of(&participant(true, false, true)),
            Presence::Muted
        );
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
