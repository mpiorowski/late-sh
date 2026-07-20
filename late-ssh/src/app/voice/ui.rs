use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    voice::svc::{VoiceParticipant, VoiceSnapshot},
};

/// Fixed height of the inline voice strip drawn at the top of a voice-enabled
/// room: one roster row and one controls row. Constant so
/// the chrome below it never shifts as people join and leave.
pub const VOICE_STRIP_HEIGHT: u16 = 2;

pub struct VoiceRoomView<'a> {
    pub snapshot: &'a VoiceSnapshot,
    pub room_id: Uuid,
    pub current_user_id: Uuid,
    pub paired_cli_supports_voice: bool,
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

/// Draw the inline voice channel strip at the top of a voice-enabled room: the
/// roster of who is connected and the controls line. Sized to exactly
/// `VOICE_STRIP_HEIGHT`.
pub fn draw_voice_strip(frame: &mut Frame, area: Rect, view: &VoiceRoomView<'_>) {
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

    frame.render_widget(Paragraph::new(vec![roster, controls]), area);
}

pub fn global_voice_badge<F>(
    snapshot: &VoiceSnapshot,
    current_user_id: Uuid,
    mut channel_label: F,
) -> Option<String>
where
    F: FnMut(Uuid) -> Option<String>,
{
    if !snapshot.enabled {
        return None;
    }
    let room_id = snapshot.current_room(current_user_id)?;
    let participant = snapshot.participant(room_id, current_user_id)?;
    let label = channel_label(room_id).unwrap_or_else(|| short_voice_room_id(room_id));
    let status = Presence::of(participant).label();
    Some(format!(" mic {label} [{status}] "))
}

fn voice_controls_text(view: &VoiceRoomView<'_>) -> String {
    if !view.snapshot.enabled {
        return "Voice is not configured.".to_string();
    }
    if !view.paired_cli_supports_voice() {
        return "Run the native late CLI to join voice.".to_string();
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

fn short_voice_room_id(room_id: Uuid) -> String {
    let id = room_id.simple().to_string();
    format!("voice-{}", &id[..8])
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
#[path = "ui_test.rs"]
mod ui_test;

