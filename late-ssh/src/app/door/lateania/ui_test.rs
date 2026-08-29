use super::{
    compare_span, fit, hug_poi_arrows, inventory_item_tag, land_chip_name, land_map_lines,
    line_rows, meter, rarity_color, scroll_offset, star_rating, wrapped_rows,
};
use crate::app::door::lateania::world::RegionProgress;
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
        school: "physical",
        weak: None,
        resist: None,
        dot_stacks: 0,
        stunned: false,
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
    // In the field layout the side panel swaps to the battle frame while a
    // foe is locked: the target's full nature and wide meter, the ability
    // roster with readiness, and the other foes still clickable for
    // switching the lock.
    use super::super::state::ClickAction;
    use super::super::svc::AbilityView;
    view.abilities = vec![
        AbilityView {
            slot: 1,
            name: "Cleave".to_string(),
            cost: 12,
            ready: true,
            effect: "heavy swing".to_string(),
        },
        AbilityView {
            slot: 2,
            name: "War Cry".to_string(),
            cost: 40,
            ready: false,
            effect: "a long empowering shout that would overflow the panel".to_string(),
        },
    ];
    // Stress the width budget: boss-sized HP numbers, a full traits line,
    // and afflictions all at once.
    view.mobs[1].hp = 12400;
    view.mobs[1].max_hp = 21000;
    view.mobs[1].weak = Some("frost");
    view.mobs[1].resist = Some("physical");
    view.mobs[1].dot_stacks = 2;
    view.mobs[1].stunned = true;
    let (blines, bhits) = super::battle_side_panel(&view, &usernames, 30);
    // The panel draws without terminal wrapping, so every line must be
    // pre-wrapped or sized to fit - an overflowing line just clips at the
    // border in the real UI.
    for l in &blines {
        let text = line_text(l);
        assert!(
            unicode_width::UnicodeWidthStr::width(text.as_str()) <= 30,
            "battle panel line overflows the panel: {text:?}"
        );
    }
    let all: String = blines.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(all.contains("Battle"), "the battle section renders: {all}");
    assert!(
        all.contains("strikes with"),
        "the targeted foe shows its attack school: {all}"
    );
    assert!(all.contains("Ogre"), "the locked foe is named: {all}");
    assert!(
        all.contains("Also here") && all.contains("Goblin"),
        "the other foe stays visible for switching: {all}"
    );
    assert!(
        all.contains("Cleave") && all.contains("War Cry"),
        "the ability roster shows mid-fight: {all}"
    );
    let foe_hits = bhits
        .iter()
        .filter(|(_, a)| matches!(a, ClickAction::AttackMob(_)))
        .count();
    let cast_hits = bhits
        .iter()
        .filter(|(_, a)| matches!(a, ClickAction::Ability(_)))
        .count();
    assert_eq!(foe_hits, 2, "both foes stay clickable mid-fight");
    assert_eq!(cast_hits, 2, "each ability row casts on click");
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

#[test]
fn foe_rows_carry_the_full_name_without_truncation() {
    use super::super::svc::{MobView, empty_player_view};
    use crate::usernames::UsernameLookup;
    use std::collections::HashMap;

    let mut view = empty_player_view();
    view.classed = true;
    view.mobs = vec![MobView {
        id: 7,
        name: "a scrawny wolf-pup of the King's Road".to_string(),
        hp: 12,
        max_hp: 20,
        level: 2,
        rank: "common".to_string(),
        boss: false,
        targeted: false,
        school: "physical",
        weak: None,
        resist: None,
        dot_stacks: 0,
        stunned: false,
    }];
    let names: HashMap<uuid::Uuid, String> = HashMap::new();
    let usernames = UsernameLookup::new(&names, None);
    let (lines, _hits, _player_hits) = super::room_panel(&view, &usernames, 28, None);
    let all: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(
        !all.contains('\u{2026}'),
        "no ellipsis truncation in the foe roster: {all}"
    );
    // The whole name survives, wrapped across lines (whitespace collapses).
    let flat = all.replace('\n', " ");
    let squashed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        squashed.contains("a scrawny wolf-pup of the King's Road"),
        "the full foe name is readable: {squashed}"
    );
    assert!(squashed.contains("12/20"), "the meter carries real numbers");
}

#[test]
fn battle_frame_names_the_foe_and_both_sides_vitals() {
    use super::super::svc::{MobView, empty_player_view};

    let mut view = empty_player_view();
    view.classed = true;
    view.hp = 156;
    view.max_hp = 210;
    view.resource = 40;
    view.max_resource = 100;
    view.resource_name = "Rage".to_string();
    view.shield = 24;
    view.mobs = vec![MobView {
        id: 9,
        name: "Vulcaranth, the Cinder-Wyrm".to_string(),
        hp: 1240,
        max_hp: 2100,
        level: 44,
        rank: "epic".to_string(),
        boss: true,
        targeted: true,
        school: "fire",
        weak: Some("frost"),
        resist: Some("fire"),
        dot_stacks: 2,
        stunned: false,
    }];
    let lines = super::battle_context(&view, 60).expect("a targeted foe raises the frame");
    let all: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(
        all.contains("Vulcaranth, the Cinder-Wyrm"),
        "full name: {all}"
    );
    assert!(all.contains("weak to frost"), "the tactical opening shows");
    assert!(all.contains("strikes with fire"), "the attack school shows");
    assert!(all.contains("1240/2100"), "the foe's real numbers show");
    assert!(all.contains("156/210"), "the player's vitals show");
    assert!(all.contains("40/100"), "the resource meter shows");
    assert!(all.contains("bleeding x2"), "afflictions show");
    assert!(all.contains("shield 24"), "player effects show");

    // No fight, no frame: the room prose keeps the column.
    view.mobs.clear();
    assert!(super::battle_context(&view, 60).is_none());
}

#[test]
fn journal_full_view_rows_wrap_and_carry_the_tracked_flag() {
    use super::super::svc::{QuestKind, QuestView, empty_player_view};

    let mut view = empty_player_view();
    view.classed = true;
    let q = QuestView {
        name: "Grave Relics".to_string(),
        desc: "The chapel will pay for three relics recovered from the depths \
               of the Sunken Catacombs, entered from Tasmania's square."
            .to_string(),
        done: false,
        reward: "150 gold".to_string(),
        kind: QuestKind::Board,
        target: Some(1),
    };
    let rows = super::quest_entry_rows(&q, &view, true, true, 30);
    let all: String = rows.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(all.contains("Grave Relics"), "the name renders: {all}");
    assert!(all.contains("tracked"), "the tracked flag renders: {all}");
    assert!(all.contains("150 gold"), "the reward renders: {all}");
    // The description is pre-wrapped: full-screen columns draw without
    // terminal wrapping, so an over-wide line would clip at the column edge.
    for r in &rows[1..] {
        let text = line_text(r);
        assert!(
            unicode_width::UnicodeWidthStr::width(text.as_str()) <= 30,
            "journal column line overflows: {text:?}"
        );
    }
}

#[test]
fn journal_seals_the_frontier_until_its_titles_are_held() {
    use super::super::svc::{QuestKind, QuestView, RoadStepView, empty_player_view};

    let mut view = empty_player_view();
    view.quests = vec![QuestView {
        name: "First Steps".to_string(),
        desc: "Leave the Hollow.".to_string(),
        done: false,
        reward: "25 gold + 20 xp".to_string(),
        kind: QuestKind::Starter,
        target: Some(1),
    }];
    view.road = vec![RoadStepView {
        boss: "the Elder Treant".to_string(),
        place: "Whisperwood",
        unlocks: "the descent into Duskhollow",
        done: false,
        current: true,
        target: Some(28),
    }];
    view.frontier_open = false;
    let (lines, _sel) = super::quests_panel(&view, 0, None);
    let all: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(
        all.contains("Enter track"),
        "the keys are named at the top of the panel: {all}"
    );
    assert!(all.contains("The Long Road"), "the roadmap section renders");
    assert!(all.contains("the Elder Treant"), "milestones are named");
    assert!(
        all.contains("The Frontier - sealed"),
        "a locked Frontier collapses to one line: {all}"
    );

    view.frontier_open = true;
    let (lines, _sel) = super::quests_panel(&view, 0, None);
    let all: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(
        !all.contains("The Frontier - sealed"),
        "an open Frontier drops the sealed line"
    );

    // Tracking: the tracked target's row carries the flag.
    let (lines, _sel) = super::quests_panel(&view, 0, Some(1));
    let all: String = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(all.contains("tracked"), "the tracked row is flagged: {all}");

    // The cursor continues past the quests onto the Long Road, so w/s can
    // walk (and scroll) the whole journal; a road row highlights and its
    // tracked lair carries the flag too.
    let (lines, sel) = super::quests_panel(&view, 1, Some(28));
    let sel = sel.expect("a road row can hold the cursor");
    assert!(
        line_text(&lines[sel]).contains("the Elder Treant"),
        "cursor row 1 is the first road milestone"
    );
    assert!(
        line_text(&lines[sel]).contains("tracked"),
        "a tracked crown is flagged"
    );
}

/// An atlas row for the land map, with only the fields that view reads set.
fn land(name: &'static str, explored: usize, chain: Option<(usize, usize)>) -> RegionProgress {
    RegionProgress {
        name,
        tier: "",
        note: "",
        total: 1000,
        explored,
        here: false,
        bosses: 9,
        levels: Some((90, 100)),
        chain,
    }
}

/// The whole atlas as a fresh-ish character sees it: a few lands walked, the
/// Frontier three zones deep and underfoot, everything else untouched.
fn sample_atlas() -> Vec<RegionProgress> {
    atlas_with(3)
}

/// The same atlas, but every chained land walked end to end, so every depth
/// counter carries its widest possible text.
fn walked_atlas() -> Vec<RegionProgress> {
    atlas_with(usize::MAX)
}

fn atlas_with(depth: usize) -> Vec<RegionProgress> {
    use crate::app::door::lateania::world::region_names;
    let chained = [
        ("The Frontier", 20usize),
        ("The Sundered Reaches", 20),
        ("Kaelmyr, the Ashen Reach", 20),
        ("The Sunderlakes", 14),
        ("Broceliande, the Greenwood", 20),
        ("Aelunor, the Faewood", 12),
        ("The Wildbound Waste", 3),
    ];
    let walked = [
        "The Frontier",
        "The Overworld & Capitals",
        "Embergate & the King's Road",
        "City Districts",
        "Wayfarer's Hollow",
    ];
    region_names()
        .into_iter()
        .map(|n| {
            let deep = depth == usize::MAX || walked.contains(&n);
            let mut r = land(
                n,
                if deep { 40 } else { 0 },
                chained
                    .iter()
                    .find(|(c, _)| *c == n)
                    .map(|&(_, z)| (if deep { depth.min(z) } else { 0 }, z)),
            );
            r.here = n == "The Frontier";
            r
        })
        .collect()
}

fn lines_of(atlas: &[RegionProgress]) -> Vec<String> {
    land_map_lines(atlas, 200)
        .into_iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

fn plain_lines() -> Vec<String> {
    lines_of(&sample_atlas())
}

#[test]
fn the_atlas_draws_every_road_in_the_world_and_invents_none() {
    use crate::app::door::lateania::worldmap::land_links;

    // The picture is hand-drawn, but which lands it joins is not: a road may
    // only be drawn where the room graph has one, and every road the room
    // graph has must be on the map. This is the test that fails when a new
    // country is wired into the world and nobody found it a place.
    let pair = |a: &'static str, b: &'static str| if a <= b { (a, b) } else { (b, a) };
    let mut drawn: Vec<(&str, &str)> = super::ROADS.iter().map(|r| pair(r.a, r.b)).collect();
    drawn.sort_unstable();
    let before = drawn.len();
    drawn.dedup();
    assert_eq!(before, drawn.len(), "a road is drawn twice");

    let mut real: Vec<(&str, &str)> = land_links()
        .iter()
        .flat_map(|(&here, theres)| theres.iter().map(move |&there| pair(here, there)))
        .collect();
    real.sort_unstable();
    real.dedup();
    assert_eq!(drawn, real);

    // And every land is somewhere: a keep, a name on a road, or called out as
    // reachable only by waystone. Exactly once, so none is drawn twice either.
    let mut placed: Vec<&str> = super::KEEPS
        .iter()
        .map(|k| k.region)
        .chain(super::PLACES.iter().map(|p| p.region))
        .chain(crate::app::door::lateania::worldmap::portal_lands())
        .collect();
    let before = placed.len();
    placed.sort_unstable();
    placed.dedup();
    assert_eq!(before, placed.len(), "a land is drawn twice");
    let mut names = crate::app::door::lateania::world::region_names();
    names.sort_unstable();
    assert_eq!(placed, names);
}

#[test]
fn the_atlas_lays_the_realm_out_the_way_it_is_walked() {
    let lines = plain_lines();
    let row = |needle: &str| {
        lines
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no row for {needle} in {lines:#?}"))
    };
    let col = |needle: &str| lines[row(needle)].find(needle).expect("column");

    // Two walled keeps side by side, with the road between them running from
    // one to the other, and the districts that open off both drawn between.
    assert!(col("OVERWORLD") < col("EMBERGATE"));
    assert_eq!(row("OVERWORLD"), row("EMBERGATE"));
    assert!(col("OVERWORLD") < col("City Districts"));
    assert!(col("City Districts") < col("EMBERGATE"));

    // The deep road runs south: the Reaches below the overworld, Kaelmyr below
    // the Reaches. Nothing on the map says why; that is the point.
    assert!(row("OVERWORLD") < row("Sundered Reaches"));
    assert!(row("Sundered Reaches") < row("Kaelmyr"));
    // The gentle countries sit north of the road, the dark ones south of it.
    for north in ["Aelunor", "Silvael", "Wildbound Waste", "Sunderlakes"] {
        assert!(row(north) < row("OVERWORLD"), "{north} belongs north");
    }
    for south in ["Sunken Catacombs", "Thornwood Hollows", "Drowned Caverns"] {
        assert!(row(south) > row("OVERWORLD"), "{south} belongs south");
    }
    // Aelunor is reached through Silvael, so it is drawn the far side of it.
    assert!(col("Aelunor") < col("Silvael"));

    // The lands no road reaches are named as such rather than drawn adrift.
    let ways = &lines[row("Only the Ways reach:")];
    assert!(ways.contains("Portal Villages"), "{ways}");
    assert!(ways.contains("Shattered Archipelago"), "{ways}");
}

#[test]
fn the_atlas_says_how_deep_you_have_walked_in_zones_not_rooms() {
    // Three zones into a twenty-zone country on 4% of its rooms: the map says
    // 3/20, because depth is what tells a player how far in they are. It says
    // nothing about bosses or levels even though the atlas rows carry both.
    let lines = plain_lines();
    assert!(
        lines.iter().any(|l| l.contains("Frontier  3/20")),
        "{lines:#?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("Kaelmyr  0/20")),
        "an unwalked land is named, not hidden: {lines:#?}"
    );
    // A land with no zone chain shows no depth at all.
    let districts = lines
        .iter()
        .find(|l| l.contains("City Districts"))
        .expect("districts");
    assert!(!districts.contains("City Districts  "), "{districts}");
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("20/1000") || l.contains("90")),
        "room counts and level bands belong to the text atlas: {lines:#?}"
    );
}

#[test]
fn the_land_map_stays_inside_the_narrowest_terminal_it_draws_into() {
    // `lands_fit` refuses to draw the map below 76 columns, so every row has to
    // fit that - with every depth counter at its widest, since the picture is
    // anchored on the roads and a name grows away from them as you explore.
    // This is the test that fails when a land is renamed to something too long.
    for atlas in [sample_atlas(), walked_atlas()] {
        for line in lines_of(&atlas) {
            assert!(
                line.chars().count() <= 76,
                "row overflows the 76-column floor: {line:?}"
            );
        }
    }
}

#[test]
fn no_land_on_the_atlas_is_written_over_by_another() {
    // Names and roads share one character grid, so a layout mistake shows up as
    // a name with a road punched through it. Every land has to survive whole,
    // at both ends of the exploration range.
    for atlas in [sample_atlas(), walked_atlas()] {
        let text = lines_of(&atlas).join("\n");
        for region in crate::app::door::lateania::world::region_names() {
            let name = land_chip_name(region);
            let name = match region == "The Overworld & Capitals"
                || region == "Embergate & the King's Road"
            {
                true => name.to_uppercase(),
                false => name,
            };
            assert!(text.contains(&name), "{name} is not readable on the map");
        }
        for depth in ["12/12", "3/3", "14/14", "20/20"] {
            assert!(
                !atlas.iter().any(|r| r
                    .chain
                    .is_some_and(|(w, z)| { format!("{w}/{z}") == depth }))
                    || text.contains(depth),
                "a depth counter was clipped: {depth}"
            );
        }
    }
}

#[test]
fn map_labels_drop_the_atlas_titles_tail_and_leading_the() {
    // The picture has to fit a terminal, so a label carries the short name.
    assert_eq!(land_chip_name("Kaelmyr, the Ashen Reach"), "Kaelmyr");
    assert_eq!(land_chip_name("Embergate & the King's Road"), "Embergate");
    assert_eq!(land_chip_name("The Overworld & Capitals"), "Overworld");
    assert_eq!(land_chip_name("Wayfarer's Hollow"), "Wayfarer's Hollow");
}

#[test]
fn the_land_map_only_offers_the_scroll_key_when_it_overflows() {
    // A hint for a key that does nothing is worse than no hint.
    let tall: String = land_map_lines(&sample_atlas(), 200)
        .last()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .unwrap_or_default();
    assert!(!tall.contains("scroll"), "{tall}");
    let short: String = land_map_lines(&sample_atlas(), 8)
        .last()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .unwrap_or_default();
    assert!(short.contains("[ ] scroll"), "{short}");
}

#[test]
fn every_class_glows_the_ability_that_drives_its_attack() {
    // The character sheet marks one attribute row as the class's key ability -
    // the score its damage leans on (`primary_score`). The mapping used to be a hand-kept
    // list of the first five classes, so a Berserker (STR) or a Bard (CHA) read
    // as if no attribute mattered to them at all.
    use crate::app::door::lateania::classes::Class;
    use ratatui::style::Modifier;

    for class in Class::ALL {
        let mut view = crate::app::door::lateania::svc::empty_player_view();
        view.classed = true;
        view.class_name = class.name().to_string();
        view.class_key = class.as_key().to_string();

        let labels: Vec<&str> = view.scores.rows().iter().map(|(l, _, _)| *l).collect();
        let glowing: Vec<String> = super::character_panel(&view)
            .iter()
            .filter_map(|line| {
                let label = line.spans.first()?;
                let name = label.content.trim().to_string();
                if !labels.contains(&name.as_str()) {
                    return None;
                }
                label
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD)
                    .then_some(name)
            })
            .collect();

        assert_eq!(
            glowing,
            vec![class.primary_score().label().to_string()],
            "{} should glow exactly its key ability",
            class.name()
        );
    }
}

#[test]
fn every_class_wears_its_own_emblem_under_the_portrait() {
    // Same stale-list bug as the key ability: the portrait emblem was a
    // hand-kept map of ten class names, so the other seven callings stood under
    // a nameless "Adventurer" bust. Each class also gets its own glyph - two
    // callings sharing a mark makes the portrait say less than nothing.
    use crate::app::door::lateania::classes::Class;

    let mut seen: Vec<String> = Vec::new();
    for class in Class::ALL {
        let portrait =
            super::composed_portrait(class.as_key(), &[0u8; 4], ratatui::style::Color::White);
        let emblem = portrait.last().map(line_text).unwrap_or_default();
        assert!(
            emblem.contains(class.name()),
            "{} stands under \"{}\"",
            class.name(),
            emblem.trim()
        );
        let glyph = emblem.trim().chars().next().expect("an emblem glyph");
        assert!(
            !seen.contains(&glyph.to_string()),
            "{} reuses the {glyph} emblem",
            class.name()
        );
        seen.push(glyph.to_string());
    }
}

#[test]
fn the_creation_screen_states_what_every_score_does_in_numbers() {
    use crate::app::door::lateania::stats::AbilityScores;
    let mut view = crate::app::door::lateania::svc::empty_player_view();
    view.level = 1;
    view.scores = AbilityScores {
        strength: 16,
        dexterity: 8,
        constitution: 14,
        intelligence: 10,
        wisdom: 12,
        charisma: 6,
    };
    let text: Vec<String> = super::attribute_rule_lines(&view)
        .iter()
        .map(line_text)
        .collect();
    assert!(text[0].contains("a point to place every 4 levels, scores cap at 20"));
    let rows: Vec<&str> = text[1..].iter().map(|s| s.trim_end()).collect();
    assert_eq!(rows.len(), 6);
    assert!(
        rows[0].starts_with("  STR swings hit for +6%"),
        "{}",
        rows[0]
    );
    assert!(rows[0].ends_with("each +1 modifier: +2% swing damage"));
    assert!(
        rows[1].starts_with("  DEX 2% of swings glance for half"),
        "{}",
        rows[1]
    );
    assert!(
        rows[2].starts_with("  CON +8 max HP at level 1"),
        "{}",
        rows[2]
    );
    assert!(rows[3].starts_with("  INT spell power +0%"), "{}", rows[3]);
    assert!(
        rows[4].starts_with("  WIS +1 resource every tick"),
        "{}",
        rows[4]
    );
    assert!(
        rows[5].starts_with("  CHA shops 6% dearer, sells 6% cheaper, taming -6%"),
        "{}",
        rows[5]
    );
}

#[test]
fn the_point_screen_shows_now_and_after_for_every_score() {
    use crate::app::door::lateania::stats::ScoreOfferView;
    let mut view = crate::app::door::lateania::svc::empty_player_view();
    view.level = 8;
    view.score_points = 2;
    view.score_offer = vec![
        ScoreOfferView {
            label: "STR".to_string(),
            name: "Strength".to_string(),
            value: 13,
            modifier: 1,
            now: "swings hit for +2%".to_string(),
            after: Some("swings hit for +4%".to_string()),
            rule: "each +1 modifier: +2% swing damage".to_string(),
        },
        ScoreOfferView {
            label: "DEX".to_string(),
            name: "Dexterity".to_string(),
            value: 12,
            modifier: 1,
            now: "2% of swings crit for double".to_string(),
            after: Some("2% of swings crit for double".to_string()),
            rule: "rule".to_string(),
        },
        ScoreOfferView {
            label: "CON".to_string(),
            name: "Constitution".to_string(),
            value: 20,
            modifier: 5,
            now: "+40 max HP at level 8".to_string(),
            after: None,
            rule: "rule".to_string(),
        },
    ];
    let text: Vec<String> = super::score_point_lines(&view, 40)
        .iter()
        .map(line_text)
        .collect();
    assert!(
        text[1].contains("Level 8 - 2 attribute point(s) to place"),
        "{}",
        text[1]
    );
    assert_eq!(text[4], "  1 STR 13 (+1) · Strength");
    assert_eq!(text[5], "      now: swings hit for +2%");
    assert_eq!(text[6], "      +1 -> 14: swings hit for +4%");
    assert_eq!(text[7], "      each +1 modifier: +2% swing damage");
    assert_eq!(text[9], "  2 DEX 12 (+1) · Dexterity");
    assert_eq!(
        text[11],
        "      +1 -> 13: 2% of swings crit for double (the modifier moves at 14)"
    );
    assert_eq!(text[14], "  3 CON 20 (+5) · Constitution");
    assert_eq!(text[16], "      at the cap of 20");
}

/// The point screen blocks every key until the point is placed, so all six
/// choices must be on screen. The full layout is five rows a score, 34 in
/// all; on a standard 80x24 the game area has about 21, which used to leave
/// Wisdom and Charisma (keys 5 and 6) below the fold with no way to scroll.
/// When the rows run short every score collapses to one line.
#[test]
fn the_point_screen_fits_the_rows_it_has() {
    use crate::app::door::lateania::stats::{AbilityScores, Score, ScoreOfferView};
    let mut view = crate::app::door::lateania::svc::empty_player_view();
    view.level = 8;
    view.score_points = 1;
    let scores = AbilityScores {
        strength: 13,
        dexterity: 12,
        constitution: 20,
        intelligence: 10,
        wisdom: 9,
        charisma: 7,
    };
    view.score_offer = Score::ALL
        .iter()
        .map(|&which| {
            let value = scores.score(which);
            let mut raised = scores;
            let after = raised.raise(which).then(|| raised.effect(which, 8));
            ScoreOfferView {
                label: which.label().to_string(),
                name: which.name().to_string(),
                value,
                modifier: crate::app::door::lateania::stats::modifier(value),
                now: scores.effect(which, 8),
                after,
                rule: which.rule().to_string(),
            }
        })
        .collect();

    let tall: Vec<String> = super::score_point_lines(&view, 40)
        .iter()
        .map(line_text)
        .collect();
    assert_eq!(tall.len(), 34, "the full layout when there is room");

    let short: Vec<String> = super::score_point_lines(&view, 21)
        .iter()
        .map(line_text)
        .collect();
    assert!(
        short.len() <= 21,
        "{} lines cannot fit 21 rows:\n{}",
        short.len(),
        short.join("\n")
    );
    let rows: Vec<&String> = short
        .iter()
        .filter(|l| l.trim_start().starts_with(char::is_numeric))
        .collect();
    assert_eq!(rows.len(), 6, "one line a score:\n{}", short.join("\n"));
    assert!(rows[0].starts_with("  1 STR 13 (+1)"), "{}", rows[0]);
    assert!(
        rows[0].contains("swings hit for +2% -> swings hit for +4%"),
        "{}",
        rows[0]
    );
    assert!(
        rows[1].contains("2% of swings crit for double -> the same until 14"),
        "{}",
        rows[1]
    );
    assert!(rows[2].contains("at the cap of 20"), "{}", rows[2]);
    assert!(rows[4].starts_with("  5 WIS 9 (-1)"), "{}", rows[4]);
    assert!(rows[5].starts_with("  6 CHA 7 (-2)"), "{}", rows[5]);
    assert!(
        short.iter().any(|l| l.contains("1-6")),
        "the keys are still explained"
    );
}
