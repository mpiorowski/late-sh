//! Desktop notifications (OSC 777 / OSC 9) for app events.
//!
//! One domain for everything the app can notify the user about. Producers
//! anywhere push a [`Notification`] through a cloned [`Notifier`]; render
//! drains the session [`Outbox`] once per frame into terminal side-channel
//! bytes, applying the user's notify settings (kinds, cooldown, format, bell).

use std::time::{Duration, Instant};

use late_core::models::profile::Profile;
use tokio::sync::mpsc;

/// Everything the app can notify about. `key()` must match the string
/// identifiers stored in `users.settings.notify_kinds`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Friends,
    Dms,
    Mentions,
    GameEvents,
}

impl Kind {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Friends => "friends",
            Self::Dms => "dms",
            Self::Mentions => "mentions",
            Self::GameEvents => "game_events",
        }
    }

    /// Friend notifications skip the `notify_kinds` opt-in because `/friend`
    /// is already the opt-in.
    fn enabled(self, kinds: &[String]) -> bool {
        matches!(self, Self::Friends) || kinds.iter().any(|kind| kind == self.key())
    }
}

/// One desktop notification. The constructors below are the complete set of
/// notifications the app can emit; keep all copy here.
#[derive(Clone, Debug)]
pub(crate) struct Notification {
    pub kind: Kind,
    pub title: String,
    pub body: String,
}

impl Notification {
    pub(crate) fn friend_online(username: &str) -> Self {
        Self {
            kind: Kind::Friends,
            title: "Friend online".to_string(),
            body: format!("@{username} joined late.sh"),
        }
    }

    pub(crate) fn dm(sender: &str, preview: String) -> Self {
        Self {
            kind: Kind::Dms,
            title: format!("New DM from {sender}"),
            body: preview,
        }
    }

    pub(crate) fn mention(sender: &str, preview: String) -> Self {
        Self {
            kind: Kind::Mentions,
            title: format!("{sender} mentioned you"),
            body: preview,
        }
    }

    pub(crate) fn your_turn(game: &str, room: &str) -> Self {
        Self {
            kind: Kind::GameEvents,
            title: format!("{game}: your turn"),
            body: format!("Waiting on you in {room}"),
        }
    }

    pub(crate) fn daily_your_turn(game: &str, opponent: &str) -> Self {
        Self {
            kind: Kind::GameEvents,
            title: format!("Daily {game}: your turn"),
            body: format!("@{opponent} is waiting on your move"),
        }
    }

    pub(crate) fn poll_started(question: &str) -> Self {
        Self {
            kind: Kind::GameEvents,
            title: "Poll started".to_string(),
            body: question.to_string(),
        }
    }
}

/// Create the session's notification channel. Clone the [`Notifier`] into any
/// producer; `App` keeps the [`Outbox`] and drains it on render.
pub(crate) fn channel() -> (Notifier, Outbox) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        Notifier { tx },
        Outbox {
            rx,
            last_emitted_at: None,
        },
    )
}

#[derive(Clone)]
pub(crate) struct Notifier {
    tx: mpsc::UnboundedSender<Notification>,
}

impl Notifier {
    pub(crate) fn push(&self, notification: Notification) {
        let _ = self.tx.send(notification);
    }
}

pub(crate) struct Outbox {
    rx: mpsc::UnboundedReceiver<Notification>,
    last_emitted_at: Option<Instant>,
}

impl Outbox {
    /// Drain pending notifications into at most one terminal payload per
    /// call. Notifications during cooldown or with disabled kinds are
    /// dropped, not queued.
    pub(crate) fn drain(&mut self, profile: &Profile) -> Option<Vec<u8>> {
        let mut first = None;
        let mut pending = 0usize;
        while let Ok(notification) = self.rx.try_recv() {
            pending += 1;
            if first.is_none() && notification.kind.enabled(&profile.notify_kinds) {
                first = Some(notification);
            }
        }
        if pending == 0 {
            return None;
        }

        let cooldown = Duration::from_secs(profile.notify_cooldown_mins as u64 * 60);
        let cooldown_ok = self
            .last_emitted_at
            .is_none_or(|at| at.elapsed() >= cooldown);
        let Some(notification) = first.filter(|_| cooldown_ok) else {
            tracing::debug!(
                cooldown_ok,
                pending,
                "dropping pending desktop notifications"
            );
            return None;
        };

        tracing::info!(
            kind = notification.kind.key(),
            title = notification.title,
            body = notification.body,
            "emitting desktop notification"
        );
        self.last_emitted_at = Some(Instant::now());
        Some(terminal_bytes(
            &notification,
            Mode::from_format(profile.notify_format.as_deref()),
            profile.notify_bell,
        ))
    }
}

/// Which desktop-notification OSC sequence(s) to emit. Chosen by the user
/// in profile settings; stored as a string key and mapped here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Both,
    Osc777,
    Osc9,
}

impl Mode {
    /// Map the `notify_format` profile field to a concrete mode. Unknown
    /// or missing values fall back to `Both`, matching the on-read
    /// default in `late_core::models::user::extract_notify_format`.
    fn from_format(format: Option<&str>) -> Self {
        match format.unwrap_or("both") {
            "osc777" => Self::Osc777,
            "osc9" => Self::Osc9,
            _ => Self::Both,
        }
    }
}

fn sanitize_field(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '\x1b' | '\x07' | '\n' | '\r' => ' ',
            ';' => '|',
            _ => ch,
        })
        .collect()
}

fn terminal_bytes(notification: &Notification, mode: Mode, bell: bool) -> Vec<u8> {
    // OSC 777 carries (title, body) separately — kitty, Ghostty, rxvt-unicode,
    // foot, wezterm, konsole. OSC 9 is iTerm2's single-string variant.
    // `Both` is the profile/default setting for users who want broad
    // compatibility. Terminal image protocol detection is separate and does
    // not narrow notification formats.
    let title = sanitize_field(&notification.title);
    let body = sanitize_field(&notification.body);
    let osc777 = format!("\x1b]777;notify;{title};{body}\x1b\\");
    let osc9 = format!("\x1b]9;{title}: {body}\x1b\\");
    let bell = if bell { "\x07" } else { "" };
    match mode {
        Mode::Both => format!("{osc777}{osc9}{bell}").into_bytes(),
        Mode::Osc777 => format!("{osc777}{bell}").into_bytes(),
        Mode::Osc9 => format!("{osc9}{bell}").into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dm_bytes(mode: Mode, bell: bool) -> String {
        let notification = Notification::dm("sender", "hello".to_string());
        let notification = Notification {
            title: "DM title".to_string(),
            ..notification
        };
        String::from_utf8(terminal_bytes(&notification, mode, bell)).expect("valid utf8")
    }

    #[test]
    fn terminal_bytes_both_mode_with_bell_emits_osc_777_and_osc_9() {
        assert_eq!(
            dm_bytes(Mode::Both, true),
            "\x1b]777;notify;DM title;hello\x1b\\\x1b]9;DM title: hello\x1b\\\x07"
        );
    }

    #[test]
    fn terminal_bytes_osc777_mode_emits_only_osc_777() {
        assert_eq!(
            dm_bytes(Mode::Osc777, false),
            "\x1b]777;notify;DM title;hello\x1b\\"
        );
    }

    #[test]
    fn terminal_bytes_osc9_mode_emits_only_osc_9() {
        assert_eq!(dm_bytes(Mode::Osc9, false), "\x1b]9;DM title: hello\x1b\\");
    }

    #[test]
    fn terminal_bytes_sanitize_control_bytes_and_separators() {
        let notification = Notification {
            kind: Kind::Dms,
            title: "hey;\x07".to_string(),
            body: "a\nb\x1bc".to_string(),
        };
        let got = String::from_utf8(terminal_bytes(&notification, Mode::Both, false))
            .expect("valid utf8");
        assert_eq!(
            got,
            "\x1b]777;notify;hey| ;a b c\x1b\\\x1b]9;hey| : a b c\x1b\\"
        );
    }

    #[test]
    fn mode_from_format_maps_known_values_and_defaults_to_both() {
        assert_eq!(Mode::from_format(Some("both")), Mode::Both);
        assert_eq!(Mode::from_format(Some("osc777")), Mode::Osc777);
        assert_eq!(Mode::from_format(Some("osc9")), Mode::Osc9);
        assert_eq!(Mode::from_format(None), Mode::Both);
        assert_eq!(Mode::from_format(Some("")), Mode::Both);
        assert_eq!(Mode::from_format(Some("garbage")), Mode::Both);
    }

    #[test]
    fn drain_emits_first_enabled_kind_and_drops_the_rest() {
        let (notifier, mut outbox) = channel();
        let profile = Profile {
            notify_kinds: vec!["mentions".to_string()],
            ..Profile::default()
        };
        notifier.push(Notification::dm("a", "dm body".to_string()));
        notifier.push(Notification::mention("b", "mention body".to_string()));
        notifier.push(Notification::mention("c", "later body".to_string()));

        let bytes = outbox.drain(&profile).expect("one payload");
        let got = String::from_utf8(bytes).expect("valid utf8");
        assert!(got.contains("mention body"));
        assert!(!got.contains("dm body"));
        // The rest were dropped, not queued.
        assert!(outbox.drain(&profile).is_none());
    }

    #[test]
    fn drain_always_allows_friend_notifications() {
        let (notifier, mut outbox) = channel();
        notifier.push(Notification::friend_online("pal"));
        assert!(outbox.drain(&Profile::default()).is_some());
    }

    #[test]
    fn drain_honors_cooldown() {
        let (notifier, mut outbox) = channel();
        let profile = Profile {
            notify_kinds: vec!["dms".to_string()],
            notify_cooldown_mins: 5,
            ..Profile::default()
        };
        notifier.push(Notification::dm("a", "first".to_string()));
        assert!(outbox.drain(&profile).is_some());
        notifier.push(Notification::dm("a", "second".to_string()));
        assert!(outbox.drain(&profile).is_none());
    }
}
