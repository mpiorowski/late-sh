use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::{primitives::row_with_hint, theme},
    voice::svc::{VoiceParticipant, VoiceSnapshot},
};

/// Fixed height of the inline voice strip drawn at the top of a voice-enabled
/// room: who is connected on the left, the keys that act on it flushed right,
/// on one row. Constant so the chrome below it never shifts as people join and
/// leave.
pub const VOICE_STRIP_HEIGHT: u16 = 1;

/// ON AIR context for a stream room's voice strip: while the room's stream
/// is live, everyone in voice is audible to anonymous watch-page listeners,
/// and the strip must say so loudly. `streamer_mic` is the streamer's
/// `(user_id, username)` while their go-live page reports the browser mic
/// open: `VoiceService`'s roster only knows CLI participants, and a speaker
/// the roster hides is the one betrayal this display must never allow.
pub struct OnAirView {
    pub live: bool,
    pub streamer_mic: Option<(Uuid, String)>,
}

pub struct VoiceRoomView<'a> {
    pub snapshot: &'a VoiceSnapshot,
    pub room_id: Uuid,
    pub current_user_id: Uuid,
    pub paired_cli_supports_voice: bool,
    /// Present only for rooms with a registered stream.
    pub on_air: Option<OnAirView>,
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

/// Draw the inline voice channel strip at the top of a voice-enabled room.
/// Sized to exactly `VOICE_STRIP_HEIGHT`.
pub fn draw_voice_strip(frame: &mut Frame, area: Rect, view: &VoiceRoomView<'_>) {
    frame.render_widget(
        Paragraph::new(voice_strip_line(view, area.width as usize)),
        area,
    );
}

/// The voice row: who is in the channel, then the keys that act on it flushed
/// to the right edge.
pub fn voice_strip_line(view: &VoiceRoomView<'_>, width: usize) -> Line<'static> {
    row_with_hint(voice_roster_spans(view), voice_control_spans(view), width)
}

/// Who is connected, or why nobody can be.
fn voice_roster_spans(view: &VoiceRoomView<'_>) -> Vec<Span<'static>> {
    if !view.snapshot.enabled {
        return vec![Span::styled(
            "Voice is off on this server.",
            Style::default().fg(theme::TEXT_DIM()),
        )];
    }
    let mut spans = Vec::new();
    // The ON AIR marker leads the row: joining voice here is broadcasting.
    if view.on_air.as_ref().is_some_and(|on_air| on_air.live) {
        spans.push(Span::styled(
            "⦿ ON AIR ",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ));
    }
    // A browser-mic streamer is audible but absent from `VoiceService`'s
    // CLI-only roster; the page's own mic report puts them on the row.
    let streamer_mic = view.on_air.as_ref().and_then(|on_air| {
        on_air.streamer_mic.as_ref().filter(|(user_id, _)| {
            view.participants()
                .iter()
                .all(|participant| participant.user_id != *user_id)
        })
    });
    if view.participants().is_empty() && streamer_mic.is_none() {
        spans.push(Span::styled(
            "No one is in voice yet.",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        return spans;
    }
    if let Some((_, username)) = streamer_mic {
        spans.push(Span::styled(
            format!("{username} · on air"),
            Style::default().fg(theme::AMBER()),
        ));
    }
    for participant in view.participants() {
        if !spans.is_empty() {
            spans.push(Span::styled("  ", Style::default().fg(theme::TEXT_DIM())));
        }
        spans.extend(participant_spans(
            participant,
            participant.user_id == view.current_user_id,
        ));
    }
    spans
}

/// Your own state plus the keys for it. Dim, because it is a reminder and not
/// the content of the room.
fn voice_control_spans(view: &VoiceRoomView<'_>) -> Vec<Span<'static>> {
    vec![Span::styled(
        voice_controls_text(view),
        Style::default().fg(theme::TEXT_DIM()),
    )]
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
