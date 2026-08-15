use super::{
    compare_span, fit, hug_poi_arrows, inventory_item_tag, line_rows, meter, rarity_color,
    scroll_offset, star_rating, wrapped_rows,
};
use crate::app::door::lateania::worldmap::{MapArrow, Tile};
use ratatui::style::Color;

#[test]
fn poi_arrows_hug_the_explored_cluster_with_boss_priority() {
    // A 10x10 canvas whose explored cluster occupies rows/cols 4..=5.
    let mut canvas = vec![vec![Tile::Empty; 10]; 10];
    canvas[4][4] = Tile::Room(1);
    canvas[5][5] = Tile::Room(2);

    let arrows = vec![
        // A tame arrow far away on the widget border...
        MapArrow {
            row: 0,
            col: 9,
            glyph: '\u{2197}',
            boss: false,
        },
        // ...and a boss arrow that clamps onto the same cluster-edge cell.
        MapArrow {
            row: 2,
            col: 9,
            glyph: '\u{2192}',
            boss: true,
        },
    ];
    let hugged = hug_poi_arrows(arrows, &canvas);

    // Both collapse onto the cluster's north-east corner cell (3, 6); the boss
    // outranks the tame arrow there.
    assert_eq!(hugged.len(), 1);
    assert_eq!((hugged[0].row, hugged[0].col), (3, 6));
    assert!(hugged[0].boss, "a boss arrow outranks a tame on one cell");

    // An empty canvas (nothing explored) leaves arrows untouched.
    let empty = vec![vec![Tile::Empty; 10]; 10];
    let kept = hug_poi_arrows(
        vec![MapArrow {
            row: 0,
            col: 9,
            glyph: '\u{2197}',
            boss: false,
        }],
        &empty,
    );
    assert_eq!((kept[0].row, kept[0].col), (0, 9));
}

#[test]
fn rarity_color_uses_the_standard_rpg_palette() {
    assert_eq!(rarity_color("common"), Color::Rgb(0xff, 0xff, 0xff));
    assert_eq!(rarity_color("uncommon"), Color::Rgb(0x1e, 0xff, 0x00));
    assert_eq!(rarity_color("rare"), Color::Rgb(0x00, 0x70, 0xdd));
    assert_eq!(rarity_color("epic"), Color::Rgb(0xa3, 0x35, 0xee));
    assert_eq!(rarity_color("legendary"), Color::Rgb(0xff, 0x80, 0x00));
    // Anything unlabelled falls back to common white.
    assert_eq!(rarity_color("mystery"), Color::Rgb(0xff, 0xff, 0xff));
}
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

#[test]
fn scroll_offset_keeps_the_selection_visible_in_a_long_list() {
    // A 40-row list in a 10-tall window: the highlighted row must always land
    // inside the visible window [off, off+height) so nothing you're on scrolls
    // off-screen (the bug: titles/inventory ran off the bottom). Short lines,
    // so one logical line is one row.
    let lines: Vec<Line> = (0..40).map(|i| Line::from(format!("row {i}"))).collect();
    let (width, height) = (40usize, 10usize);
    let mut off = 0;
    for sel in 0..lines.len() {
        off = scroll_offset(off, &lines, Some(sel), width, height);
        assert!(
            sel >= off && sel < off + height,
            "row {sel} fell outside window [{off}, {})",
            off + height
        );
        assert!(off <= lines.len() - height, "offset never overscrolls");
    }
}

#[test]
fn wrapped_rows_matches_word_wrap() {
    assert_eq!(wrapped_rows("", 10), 1);
    assert_eq!(wrapped_rows("short", 10), 1);
    assert_eq!(wrapped_rows("exactly-10", 10), 1);
    // Two words that don't both fit wrap to a second row.
    assert_eq!(wrapped_rows("hello world", 8), 2);
    // A single word longer than the width breaks across rows (ceil 12/5).
    assert_eq!(wrapped_rows("abcdefghijkl", 5), 3);
    // A real crafting ingredient row wraps in the narrow side panel.
    let ing = "    cooking · 3 river trout, 2 wild sage, 1 salt block";
    assert!(wrapped_rows(ing, 28) >= 2, "long ingredient row must wrap");
}

#[test]
fn scroll_offset_reaches_the_end_when_rows_wrap() {
    // Each recipe is a short name line + a long ingredient line that wraps to
    // two rows in a narrow panel. The crafting bug: counting logical lines
    // (not wrapped rows) left the last recipes stranded below the screen.
    let (width, height) = (28usize, 12usize);
    let mut lines: Vec<Line> = Vec::new();
    let mut name_line = Vec::new();
    for i in 0..20 {
        name_line.push(lines.len());
        lines.push(Line::from(format!("> Recipe {i}")));
        lines.push(Line::from(format!(
            "    cooking · 3 river trout, 2 wild sage, 1 salt block ({i})"
        )));
    }
    let sel = *name_line.last().unwrap();
    let off = scroll_offset(0, &lines, Some(sel), width, height);
    // The selected line must sit inside the visible *rows*, not just lines.
    let rows: Vec<usize> = lines.iter().map(|l| line_rows(l, width)).collect();
    let win_top: usize = rows[..off].iter().sum();
    let sel_top: usize = rows[..sel].iter().sum();
    assert!(
        sel_top >= win_top && sel_top < win_top + height,
        "last recipe row {sel_top} outside visible rows [{win_top}, {})",
        win_top + height
    );
}

#[test]
fn compare_span_colours_upgrades_and_downgrades() {
    assert!(compare_span(None).is_none());
    assert!(compare_span(Some(18)).is_some(), "an upgrade shows a tag");
    assert!(compare_span(Some(-12)).is_some(), "a downgrade shows a tag");
}

#[test]
fn star_rating_fills_proportionally() {
    let stars = |v, m| {
        let spans = star_rating(v, m, Color::White);
        let filled = spans[0].content.chars().filter(|c| *c == '★').count();
        let empty = spans[1].content.chars().filter(|c| *c == '☆').count();
        (filled, empty)
    };
    assert_eq!(stars(0, 18), (0, 5));
    assert_eq!(stars(18, 18), (5, 0));
    assert_eq!(stars(9, 18), (3, 2)); // (9*5 + 9) / 18 = 3
    // Always exactly five stars, whatever the value.
    for v in 0..=18 {
        let (f, e) = stars(v, 18);
        assert_eq!(f + e, 5, "value {v}");
    }
}

#[test]
fn meter_fills_proportionally_and_clamps() {
    assert_eq!(meter(0, 100, 10), "░░░░░░░░░░");
    assert_eq!(meter(100, 100, 10), "██████████");
    assert_eq!(meter(50, 100, 10), "█████░░░░░");
    // Degenerate inputs never panic or overflow the width.
    assert_eq!(meter(5, 0, 6), "░░░░░░");
    assert_eq!(meter(999, 100, 6), "██████");
}

#[test]
fn fit_pads_short_names_and_ellipsizes_long_ones() {
    assert_eq!(UnicodeWidthStr::width(fit("Goblin", 10).as_str()), 10);
    assert_eq!(fit("Goblin", 10), "Goblin    ");
    let long = fit("Ancient Frost Wyrm", 8);
    assert_eq!(UnicodeWidthStr::width(long.as_str()), 8);
    assert!(long.ends_with('…'));
}

#[test]
fn equipped_inventory_tags_show_the_slot() {
    assert_eq!(inventory_item_tag(true, Some("weapon")), " [worn weapon]");
    assert_eq!(inventory_item_tag(true, Some("chest")), " [worn chest]");
    assert_eq!(inventory_item_tag(false, Some("ring")), " (ring)");
}

use super::super::svc::{LogKind, LogLine, empty_player_view};
use super::recent_log_tail;

fn line_text(line: &Line) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn log_view(entries: &[&str]) -> super::PlayerView {
    let mut view = empty_player_view();
    view.log = entries
        .iter()
        .map(|text| LogLine {
            text: (*text).to_string(),
            kind: LogKind::Normal,
        })
        .collect();
    view
}

#[test]
fn recent_log_reads_oldest_top_newest_bottom() {
    // view.log is chronological (oldest first). The feed must render the same
    // way: oldest at the top, newest resting on the bottom row, like any MUD
    // scrollback. This is the exact regression fix/mud-log-order corrects.
    let view = log_view(&["first", "second", "third"]);
    // Wide enough that nothing wraps, tall enough that all three fit.
    let rendered: Vec<String> = recent_log_tail(&view, 40, 8)
        .iter()
        .map(line_text)
        .collect();

    let index_of = |needle: &str| {
        rendered
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} missing from {rendered:?}"))
    };
    assert!(index_of("first") < index_of("second"));
    assert!(index_of("second") < index_of("third"));
    assert!(
        rendered.last().is_some_and(|line| line.contains("third")),
        "newest event must rest on the bottom row, got {rendered:?}"
    );
}

#[test]
fn recent_log_trims_oldest_when_it_overflows_height() {
    // Five events into a window that only fits two under the "Recent" header:
    // the two newest survive, in order, and the three oldest fall off the top.
    let view = log_view(&["e1", "e2", "e3", "e4", "e5"]);
    let rendered: Vec<String> = recent_log_tail(&view, 40, 3)
        .iter()
        .map(line_text)
        .collect();
    let joined = rendered.join("\n");

    for dropped in ["e1", "e2", "e3"] {
        assert!(
            !joined.contains(dropped),
            "oldest event {dropped:?} should have been trimmed, got {rendered:?}"
        );
    }
    assert!(joined.contains("e4") && joined.contains("e5"));
    assert!(
        rendered.last().is_some_and(|line| line.contains("e5")),
        "newest event must rest on the bottom row, got {rendered:?}"
    );
}

#[test]
fn the_xp_meter_stays_on_the_character_sheet_under_a_pile_of_titles() {
    // The bug: the full-screen character sheet is three fixed-height columns
    // with no scroll of its own (`[`/`]` only reach the narrow side panel), and
    // the right column listed every earned title *before* Experience. Enough
    // titles and the XP bar walked off the bottom with no way to reach it.
    let mut view = empty_player_view();
    view.level = 30;
    view.xp_into_level = 120;
    view.xp_for_next = 400;
    view.titles = (0..30).map(|i| format!("Champion of Zone {i}")).collect();

    let lines = super::sheet_derived(&view, ratatui::style::Color::White);
    let text: Vec<String> = lines.iter().map(line_text).collect();
    let xp_row = text
        .iter()
        .position(|l| l.contains("to next"))
        .expect("the sheet shows the xp meter");

    // The sheet only draws when the area is at least 18 rows tall, minus the
    // block border: the meter has to land inside that.
    assert!(
        xp_row < 16,
        "xp meter fell to row {xp_row}, off a 16-row column: {text:?}"
    );
    assert!(
        text.iter().any(|l| l.contains("+26 more")),
        "the title list is summarised rather than unbounded: {text:?}"
    );
}

// ---- combat action bar (mouse) --------------------------------------------

fn view_with_abilities(slots: &[u8]) -> super::PlayerView {
    use super::super::svc::AbilityView;
    let mut view = super::super::svc::empty_player_view();
    view.classed = true;
    view.abilities = slots
        .iter()
        .map(|&slot| AbilityView {
            slot,
            name: format!("Ability{slot}"),
            cost: 5,
            ready: true,
            effect: String::new(),
        })
        .collect();
    view
}

#[test]
fn action_bar_always_offers_attack_quaff_and_flee() {
    use super::super::state::ClickAction;
    let view = view_with_abilities(&[1, 2, 3]);
    let chips = super::combat_chips(&view, 80);
    let actions: Vec<ClickAction> = chips.iter().map(|c| c.action).collect();
    assert_eq!(
        actions.first(),
        Some(&ClickAction::Attack),
        "attack leads the bar"
    );
    // Quaff then Flee anchor the end - the two a wounded player reaches for.
    assert_eq!(actions[actions.len() - 2], ClickAction::Quaff);
    assert_eq!(actions[actions.len() - 1], ClickAction::Flee);
    assert!(
        actions.contains(&ClickAction::Ability(2)),
        "an ability slot in the middle is clickable"
    );
}

#[test]
fn action_bar_slot_ten_is_labelled_with_the_zero_key() {
    let view = view_with_abilities(&[10]);
    let chips = super::combat_chips(&view, 80);
    let slot_chip = chips
        .iter()
        .find(|c| c.action == super::super::state::ClickAction::Ability(10))
        .expect("slot 10 chip present");
    assert!(
        slot_chip.label.starts_with("0 "),
        "slot 10 casts with `0`, so its chip shows 0: {:?}",
        slot_chip.label
    );
}

#[test]
fn action_bar_drops_abilities_before_crowding_out_quaff_and_flee() {
    use super::super::state::ClickAction;
    // A narrow bar can't fit every ability, but Quaff and Flee must survive.
    let view = view_with_abilities(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let chips = super::combat_chips(&view, 24);
    let actions: Vec<ClickAction> = chips.iter().map(|c| c.action).collect();
    assert_eq!(actions.first(), Some(&ClickAction::Attack));
    assert!(
        actions.contains(&ClickAction::Quaff),
        "quaff kept on a narrow bar"
    );
    assert!(
        actions.contains(&ClickAction::Flee),
        "flee kept on a narrow bar"
    );
    let abilities = actions
        .iter()
        .filter(|a| matches!(a, ClickAction::Ability(_)))
        .count();
    assert!(
        abilities < 8,
        "some abilities are dropped when they don't fit"
    );
}

#[test]
fn room_panel_makes_each_foe_a_clickable_row() {
    use super::super::svc::{MobView, empty_player_view};
    use crate::usernames::UsernameLookup;
    use std::collections::HashMap;

    let foe = |id: u32, name: &str, targeted: bool| MobView {
        id,
        name: name.to_string(),
        hp: 5,
        max_hp: 10,
        level: 3,
        rank: "common".to_string(),
        boss: false,
        targeted,
    };
    let mut view = empty_player_view();
    view.classed = true;
    view.mobs = vec![foe(11, "Goblin", false), foe(22, "Ogre", true)];

    let names: HashMap<uuid::Uuid, String> = HashMap::new();
    let usernames = UsernameLookup::new(&names, None);
    let (lines, hits, _player_hits) = super::room_panel(&view, &usernames, 30, None);

    assert_eq!(hits.len(), 2, "one clickable row per foe");
    for (idx, id) in &hits {
        let want = if *id == 11 { "Goblin" } else { "Ogre" };
        assert!(
            line_text(&lines[*idx]).contains(want),
            "recorded row {idx} should be the {want} row"
        );
    }
    // The foe you're locked onto is flagged with » so a click's effect shows.
    let ogre_row = hits.iter().find(|(_, id)| *id == 22).unwrap().0;
    assert!(
        line_text(&lines[ogre_row]).contains('\u{00bb}'),
        "the targeted foe is marked with »"
    );
}

#[test]
fn leaderboard_panel_shows_rank_level_class_and_value_per_board() {
    use super::super::svc::{LeaderboardEntry, LeaderboardView, empty_player_view};
    use crate::usernames::UsernameLookup;
    use std::collections::HashMap;
    use std::sync::Arc;

    let bob = uuid::Uuid::from_u128(1);
    let mut view = empty_player_view();
    view.classed = true;
    view.leaderboard = Arc::new(LeaderboardView {
        by_level: vec![LeaderboardEntry {
            user_id: bob,
            level: 42,
            class_key: "warrior".to_string(),
            value: 42,
        }],
        by_pvp_kills: vec![LeaderboardEntry {
            user_id: bob,
            level: 42,
            class_key: "warrior".to_string(),
            value: 7,
        }],
        by_gold: vec![LeaderboardEntry {
            user_id: bob,
            level: 42,
            class_key: "warrior".to_string(),
            value: 999,
        }],
    });

    let mut names: HashMap<uuid::Uuid, String> = HashMap::new();
    names.insert(bob, "Bob".to_string());
    let usernames = UsernameLookup::new(&names, None);
    let lines = super::leaderboard_panel(&view, &usernames);
    let joined = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

    assert!(
        joined.contains("Bob"),
        "the resolved name is shown: {joined}"
    );
    assert!(joined.contains("Lv42"), "the level is shown: {joined}");
    assert!(
        joined.contains("WAR"),
        "the class abbreviation is shown: {joined}"
    );
    assert!(
        joined.contains("7 kills"),
        "the pvp kill count is shown: {joined}"
    );
    assert!(joined.contains("999g"), "the gold total is shown: {joined}");
}

#[test]
fn leaderboard_panel_handles_nobody_online_yet() {
    use super::super::svc::empty_player_view;
    use crate::usernames::UsernameLookup;
    use std::collections::HashMap;

    let mut view = empty_player_view();
    view.classed = true;
    let names: HashMap<uuid::Uuid, String> = HashMap::new();
    let usernames = UsernameLookup::new(&names, None);
    // Must not panic on an empty leaderboard (the default `PlayerView`).
    let lines = super::leaderboard_panel(&view, &usernames);
    let joined = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(joined.contains("no one else is online yet"));
    assert!(joined.contains("no rivals slain yet"));
}

// The exits line says what is available; the heading line says which of them
// to take. That second question is the one the map itself cannot answer, since
// a zone boundary is a jump in the coordinate field rather than a direction.
#[test]
fn the_heading_line_names_the_exit_to_take_next() {
    use super::super::state::Heading;
    use super::super::svc::empty_player_view;
    use super::super::world::Dir;
    use super::super::worldmap::Route;
    use crate::usernames::UsernameLookup;
    use std::collections::HashMap;

    let names: HashMap<uuid::Uuid, String> = HashMap::new();
    let usernames = UsernameLookup::new(&names, None);
    let mut view = empty_player_view();
    view.classed = true;
    view.exits = vec![
        (Dir::North, "north".to_string()),
        (Dir::Down, "down".to_string()),
    ];

    let panel = |heading| {
        let (lines, _, _) = super::room_panel(&view, &usernames, 40, heading);
        lines.iter().map(line_text).collect::<Vec<_>>().join("\n")
    };

    let toward = panel(Some(Heading::Toward(
        "the Vigil House",
        Route {
            next: Dir::Down,
            rooms: 4,
        },
    )));
    assert!(
        toward.contains("the Vigil House") && toward.contains("4 rooms"),
        "the heading names the place and how far it still is: {toward}"
    );
    assert!(
        toward.contains("take down"),
        "and names the very next exit, not just the destination: {toward}"
    );

    assert!(
        panel(Some(Heading::Arrived("the Vigil House"))).contains("you're here"),
        "arriving says so instead of pointing somewhere"
    );
    assert!(
        panel(Some(Heading::Unreachable("the Vigil House"))).contains("no way there"),
        "an unreachable mark admits it rather than showing a confident direction"
    );
    assert!(!panel(None).contains("heading"), "no mark, no line");
}
