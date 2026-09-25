use super::{color_lines, realm_draft_lines};
use crate::app::lobby::realm::state::{CreateStep, PLAYER_PALETTES, RealmCreateDraft};

/// Every step of the create overlay, at the width the overlay actually
/// draws at. A tagline or a note that outgrows the box used to be invisible
/// until someone screenshotted it.
#[test]
fn the_create_overlay_never_writes_past_its_box() {
    use unicode_width::UnicodeWidthStr;

    // The overlay is 58 wide; the borders and a space each side leave this.
    const TEXT_WIDTH: usize = 54;
    for step in [
        CreateStep::Ruleset,
        CreateStep::Map,
        CreateStep::MapShape,
        CreateStep::Pace,
        CreateStep::ResetHour,
        CreateStep::Name,
        CreateStep::Color,
        CreateStep::Options,
    ] {
        for cursor in 0..4usize {
            let draft = RealmCreateDraft {
                map: cursor.min(crate::app::lobby::realm::map::MAPS.len() - 1),
                shape: crate::app::lobby::realm::mapgen::GeneratedMapSpec {
                    // The widest the shape rows ever get: three digits on
                    // every value.
                    continents: 10,
                    islands: 20,
                    territories: 400,
                    ..Default::default()
                },
                shape_field: cursor.min(2),
                color: cursor,
                step,
                ruleset: cursor,
                pace: cursor,
                hour: 18,
                name: "a name as long as anyone is allowed to type it!!".to_string(),
                option: cursor,
                options: crate::app::lobby::realm::rulesets::default_options(),
            };
            // Both clocks: a viewer with no zone gets the longer note telling
            // them where the second clock went, and a viewer far enough east
            // gets the longest hour row there is — local time plus a day.
            for viewer_tz in [None, Some(chrono_tz::Pacific::Auckland)] {
                let lines =
                    realm_draft_lines(&draft, "a-fairly-long-username", viewer_tz, TEXT_WIDTH);
                for line in &lines {
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    assert!(
                        text.width() <= TEXT_WIDTH,
                        "{step:?} row {:?} is {} wide, past {TEXT_WIDTH}",
                        text,
                        text.width()
                    );
                }
            }
        }
    }
}

/// The join overlay is the one place a player is told which colours are
/// gone and who has them, so the longest possible username on the longest
/// colour name still has to fit the box.
#[test]
fn the_join_colour_list_fits_its_box() {
    use unicode_width::UnicodeWidthStr;

    const TEXT_WIDTH: usize = 54;
    let taken: Vec<(usize, String)> = (0..4)
        .map(|i| (i, "a-really-long-username-here".to_string()))
        .collect();
    let lines = color_lines(5, &taken, TEXT_WIDTH);
    assert_eq!(lines.len(), PLAYER_PALETTES.len(), "every colour is listed");
    for line in &lines {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.width() <= TEXT_WIDTH,
            "{text:?} is {} wide, past {TEXT_WIDTH}",
            text.width()
        );
    }
}
