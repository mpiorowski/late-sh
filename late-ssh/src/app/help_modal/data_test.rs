use super::*;

#[test]
fn all_purpose_guide_keeps_artboard_out_of_topic_tabs() {
    assert!(
        !HelpTopic::ALL
            .iter()
            .any(|topic| topic.title() == "Artboard")
    );
    assert!(!bot_app_context().contains("## Artboard\n"));
}

#[test]
fn all_purpose_guide_splits_game_topics() {
    assert!(HelpTopic::ALL.iter().any(|topic| topic.title() == "Arcade"));
    assert!(HelpTopic::ALL.iter().any(|topic| topic.title() == "Lobby"));
    assert!(
        HelpTopic::ALL
            .iter()
            .any(|topic| topic.title() == "Lateania")
    );
    assert!(!HelpTopic::ALL.iter().any(|topic| topic.title() == "Games"));
    assert!(bot_app_context().contains("## Arcade\n"));
    assert!(bot_app_context().contains("## Lobby\n"));
    assert!(bot_app_context().contains("## Lateania\n"));
    assert!(!bot_app_context().contains("## Games\n"));
}

#[test]
fn all_purpose_guide_folds_music_into_pair_topic() {
    assert!(!HelpTopic::ALL.iter().any(|topic| topic.title() == "Music"));
    assert!(!bot_app_context().contains("## Music\n"));
    let pair = lines_for(HelpTopic::Pair, false, "").join("\n");
    assert!(pair.contains("Music controls"));
    assert!(pair.contains("Music Booth"));
    assert!(pair.contains("active YouTube-source users"));
}

#[test]
fn bot_context_includes_hub_guide_facts() {
    let context = bot_app_context();
    assert!(context.contains("## Economy\n"));
    assert!(context.contains("Monthly Top Chips counts net chip delta."));
    assert!(context.contains("Lateris, 2048, Snake, and Traffic record run scores."));
    assert!(context.contains("Blackjack form: name, pace, stake."));
    assert!(context.contains("Four-seat fixed-stack Texas Hold'em"));
}

#[test]
fn bot_context_includes_terminal_faq_and_image_facts() {
    let context = bot_app_context();
    assert!(context.contains("## Copy\n"));
    assert!(context.contains("## Images\n"));
    assert!(context.contains("## CLI YouTube\n"));
    assert!(context.contains("Why copy sometimes silently fails"));
    assert!(context.contains("CLI YouTube playback"));
    assert!(context.contains("/paste-image"));
    assert!(context.contains("This is CLI-only"));
    assert!(context.contains("The original-quality image is the uploaded/copied URL."));
    assert!(context.contains("Kitty protocol: kitty, Ghostty, rio, warp, Konsole."));
    assert!(context.contains("iTerm2 inline images: iTerm2, WezTerm, mintty, hterm."));
}

#[test]
fn bot_context_includes_account_linking_flow() {
    let context = bot_app_context();
    assert!(context.contains("## Settings\n"));
    assert!(context.contains("Use Settings > Account > Link Accounts"));
    assert!(context.contains("one side generates a 10-minute link code"));
    assert!(context.contains("Choose the main account to keep: Current or Other."));
    assert!(context.contains("Both SSH keys will open the main account after linking."));
    assert!(
        context.contains(
            "chips, messages, scores, streaks, settings, and other data are not merged"
        )
    );
}

#[test]
fn bot_context_includes_irc_access_flow() {
    let context = bot_app_context();
    assert!(HelpTopic::ALL.iter().any(|topic| topic.title() == "IRC"));
    assert!(context.contains("## IRC\n"));
    assert!(context.contains("Settings > Account > IRC access token"));
    assert!(context.contains("server password / PASS field"));
    assert!(context.contains("localhost:6667 with TLS off when running make start"));
    assert!(context.contains("irc.late.sh port 6697 with TLS/SSL enabled"));
    assert!(context.contains("/server add late irc.late.sh/6697"));
    assert!(context.contains("IRC is raw TCP, so irc.late.sh must be DNS-only"));
    assert!(context.contains("Game-room chat is not exposed as IRC channels."));
    assert!(context.contains("Resetting a token shows the new value once"));
}

#[test]
fn chat_guide_lists_user_facing_slash_commands() {
    let lines = chat_help_lines(false).join("\n");
    for expected in [
        "/brb [message]",
        "/challenge [@user]",
        "/coffee",
        "/friend [@user]",
        "/friends",
        "/icons",
        "/petname [name]",
        "/poll",
        "/profile [@user]",
        "/tea",
        "/upload <url>",
    ] {
        assert!(lines.contains(expected), "missing {expected}");
    }
    assert!(!lines.contains("/music"));
}

#[test]
fn music_guide_defers_pairing_setup_to_pair_tab() {
    assert!(MUSIC_PAIR_TEXT.contains("three music sources"));
    assert!(MUSIC_PAIR_TEXT.contains("active YouTube-source users"));
    assert!(!MUSIC_PAIR_TEXT.contains("two audio surfaces"));
    assert!(!MUSIC_PAIR_TEXT.contains("paired users agree"));
    assert!(!MUSIC_PAIR_TEXT.contains(SHELL_INSTALL_COMMAND));
    assert!(!MUSIC_PAIR_TEXT.contains(WINDOWS_INSTALL_COMMAND));
    assert!(!MUSIC_PAIR_TEXT.contains(NIX_COMMAND));
    assert!(!MUSIC_PAIR_TEXT.contains(SOURCE_URL));
}

#[test]
fn chat_guide_collapses_compose_section_when_keep_composer_focused() {
    let off = chat_help_lines(false).join("\n");
    assert!(off.contains("Enter              send and exit"));
    assert!(off.contains("Alt+S              send and keep open"));
    assert!(!off.contains("<<COMPOSE_SEND_LINES>>"));

    let on = chat_help_lines(true).join("\n");
    assert!(on.contains("Enter              send and keep open"));
    assert!(!on.contains("Alt+S"));
    assert!(!on.contains("send and exit"));
    assert!(!on.contains("<<COMPOSE_SEND_LINES>>"));
}

#[test]
fn bot_context_does_not_leak_restricted_commands() {
    let context = bot_app_context();
    for forbidden in [
        "/audio",
        "/create-room",
        "/delete-room",
        "/fill-room",
        "/mod",
        "staff",
        "admin",
        "moderation",
        "unskippable",
    ] {
        assert!(
            !context.to_lowercase().contains(forbidden),
            "bot context leaked {forbidden}"
        );
    }
}

#[test]
fn global_guide_points_to_hub_for_game_details() {
    let arcade = arcade_help_lines().join("\n");
    let lobby = lobby_help_lines().join("\n");
    let lateania = lateania_help_lines().join("\n");
    assert!(arcade.contains("Economy"));
    assert!(lobby.contains("Economy tab"));
    assert!(lateania.contains("Lateania"));
    // The badge glossary names games to explain each badge code; game
    // details still live in the hub, not here.
    assert!(!lobby.contains("Sudoku"));
    assert!(!lateania.contains("Clock presets"));
}
