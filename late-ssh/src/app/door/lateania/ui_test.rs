use super::{
    compare_span, fit, inventory_item_tag, line_rows, meter, rarity_color, scroll_offset,
    star_rating, wrapped_rows,
};
use ratatui::style::Color;

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
