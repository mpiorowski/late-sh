use ratatui::{Terminal, backend::TestBackend};

use super::ui::{ADDRESS, VERSION, draw_landing};

/// Render the hub landing card the way the Games page does and return its
/// text, so the assertions read what a player reads.
fn landing_text() -> String {
    let backend = TestBackend::new(110, 70);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            draw_landing(frame, frame.area(), 0);
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn repo_file(path: &str) -> String {
    let path = format!("{}/../{path}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

/// The value of the first `key = value` line for `key`, unquoted.
fn tf_value(tf: &str, key: &str) -> String {
    let line = tf
        .lines()
        .map(str::trim)
        .find(|line| {
            line.strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
        .unwrap_or_else(|| panic!("{key} not found"));
    let value = line.split_once('=').expect("key = value").1.trim();
    value.trim_matches('"').to_string()
}

/// A player needs four things from this card: where to connect, which client
/// version, which mods, and who to ask for the whitelist.
#[test]
fn landing_tells_a_player_how_to_join() {
    let text = landing_text();
    assert!(text.contains("address   vs.late.sh"), "{text}");
    assert!(text.contains("client    Vintage Story 1.22.7"), "{text}");
    assert!(text.contains("mods      Carry On, ProspectTogether"), "{text}");
    assert!(
        text.contains("/dm       a moderator with your exact Vintage Story username"),
        "{text}"
    );
    assert!(text.contains("Whitelist only."), "{text}");
}

/// Nothing is protected until it is claimed and saved, so the card has to say
/// how to claim and how to let a friend in.
#[test]
fn landing_explains_claims() {
    let text = landing_text();
    assert!(
        text.contains("/land claim save   give it a name; nothing is protected until you save"),
        "{text}"
    );
    assert!(
        text.contains("/land claim grant name all       full access: build and break"),
        "{text}"
    );
}

/// Every setting the card quotes comes from the Terraform that runs the
/// server or the image it runs. A change there has to update `ui.rs` too, or
/// players read a world that no longer exists.
#[test]
fn quoted_settings_match_the_terraform_and_the_image() {
    let server = repo_file("infra/vintagestory.tf");
    let defaults = repo_file("infra/defaults.tf");
    let dockerfile = repo_file("docker/vintagestory/Dockerfile");

    let recipe_version = dockerfile
        .lines()
        .find_map(|line| line.strip_prefix("ARG VS_VERSION="))
        .expect("ARG VS_VERSION in docker/vintagestory/Dockerfile");
    assert_eq!(recipe_version, VERSION);

    let image = tf_value(&defaults, "vintagestory_image");
    let tag = image.rsplit_once(':').expect("image tag").1;
    let tag_version = tag.rsplit_once('-').expect("<version>-<revision> tag").0;
    assert_eq!(
        tag_version, VERSION,
        "vintagestory_image must carry the game version: {image}"
    );

    assert_eq!(
        tf_value(&defaults, "vintagestory_port"),
        "42420",
        "the card gives a bare address, which only works on the default port"
    );

    assert!(
        server.contains(ADDRESS),
        "infra/vintagestory.tf no longer documents {ADDRESS}"
    );
    assert_eq!(tf_value(&server, "WhitelistMode"), "On");
    assert_eq!(tf_value(&server, "VerifyPlayerAuth"), "true");
    assert_eq!(tf_value(&server, "AllowPvP"), "true");
    assert_eq!(tf_value(&server, "AllowFireSpread"), "true");
    assert_eq!(tf_value(&server, "PassTimeWhenEmpty"), "false");
}
