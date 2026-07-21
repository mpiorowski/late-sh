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
    let got =
        String::from_utf8(terminal_bytes(&notification, Mode::Both, false)).expect("valid utf8");
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
