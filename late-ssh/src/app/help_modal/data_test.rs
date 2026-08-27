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
    assert!(context.contains("Four-seat fixed-stack Texas Hold'em"));
}

/// The Lobby replaced the rooms-era Tables screen: no table creation, no
/// setup forms, and chess moved to the daily correspondence board. The guide
/// described the old screen long after it was gone, so pin the new shape.
#[test]
fn hub_guide_describes_the_lobby_not_the_rooms_era_tables() {
    let context = bot_app_context();
    assert!(context.contains("There is one fixed table per game: no creating tables"));
    assert!(context.contains("Poker, Blackjack, Asterion, Tron, and Super Snake."));
    assert!(context.contains("Chess and the other daily games are correspondence matches now"));
    for gone in [
        "Open Tables with 4",
        "Create Table Forms",
        "Blackjack form: name, pace, stake",
        "Room stacks: 100, 500, 1000",
        "Tic-Tac-Toe",
        "Clock presets",
    ] {
        assert!(!context.contains(gone), "hub guide still describes {gone}");
    }
}

/// "How do I earn chips" is the question the bot gets most, so the Chips tab
/// has to name every paying surface, not a sample of them.
#[test]
fn chips_guide_lists_every_earning_surface() {
    let context = bot_app_context();
    assert!(HelpTopic::ALL.iter().any(|topic| topic.title() == "Chips"));
    assert!(context.contains("## Chips\n"));
    let chips = lines_for(HelpTopic::Chips, false, "").join("\n");
    for expected in [
        "Arcade dailies",
        "Solitaire draw-3",
        "Le Word daily",
        "Rubik's Cube daily",
        "Quests",
        "daily streak",
        "Bonsai",
        "Watering pays",
        "Daily correspondence matches",
        "Battleship",
        "Backgammon",
        "Asterion",
        "Super Snake",
        "Tron",
        "Poker, Blackjack",
        "Archdemon",
        "King Who Was Promised Nothing",
        "Amulet of Yendor",
        "Green Dragon",
        "Sharing news",
        "/gift @user",
    ] {
        assert!(chips.contains(expected), "chips guide missing {expected}");
    }
    // News moved from the pay-nothing list to a paying surface; the amount
    // has to read the constant the service actually credits.
    assert!(chips.contains(&format!("News pays {NEWS_SHARE_REWARD_CHIPS} chips")));
    assert!(chips.contains(&format!(
        "At most {NEWS_SHARE_MAX_PAID_PER_DAY} shares a day"
    )));
    let news = lines_for(HelpTopic::News, false, "").join("\n");
    assert!(news.contains(&format!("pays you {NEWS_SHARE_REWARD_CHIPS} chips")));
    assert!(news.contains(&format!(
        "At most {NEWS_SHARE_MAX_PAID_PER_DAY} shares a day"
    )));
    // Losing at a non-betting surface must never read as a chip risk, and the
    // pay-nothing surfaces have to be called out or the bot invents payouts.
    assert!(chips.contains("the losers lose nothing"));
    assert!(chips.contains("pay no chips yet"));
    assert!(chips.contains("no login bonus"));
    // Economy keeps ranking rules; the amounts live here, in one place.
    let economy = lines_for(HelpTopic::Economy, false, "").join("\n");
    assert!(economy.contains("The Chips tab lists every way to earn chips"));
    assert!(economy.contains("Monthly Top Chips counts net chip delta."));
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
        context
            .contains("chips, messages, scores, streaks, settings, and other data are not merged")
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
        "/coffee",
        "/friend [@user]",
        "/friends",
        "/gift @user",
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

/// OBS setup questions land on @bot, so the Streaming tab has to carry the
/// full WHIP recipe: service fields, the Opus requirement, and the encoder
/// reset trap that OBS springs when the service switches to WHIP.
#[test]
fn streaming_guide_covers_golive_and_obs_setup() {
    assert!(
        HelpTopic::ALL
            .iter()
            .any(|topic| topic.title() == "Streaming")
    );
    assert!(bot_app_context().contains("## Streaming\n"));
    let streaming = lines_for(HelpTopic::Streaming, false, "").join("\n");
    for expected in [
        "/golive [title]",
        "/golive obs",
        "/golive stop",
        "/watch @user",
        "Service           WHIP",
        "Bearer Token",
        "Opus, required: WHIP cannot carry AAC",
        "Same as stream",
        "Restart OBS",
        "minted per stream and die with it",
        "born silent",
        "ON AIR",
    ] {
        assert!(
            streaming.contains(expected),
            "streaming guide missing {expected}"
        );
    }
    // The chat commands list stays an index and defers the details here.
    let chat = chat_help_lines(false).join("\n");
    assert!(chat.contains("the Streaming tab"));
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
