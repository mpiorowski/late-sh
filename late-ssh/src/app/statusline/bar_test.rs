use late_core::models::statusline::{
    LabelMode, StatusComponent, StatusComponentSetting, StatusVariant,
};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::bar::{
    Placement, StatusClick, build_status_bar, build_top_status_bar, click_action,
    fixed_topbar_components,
};
use super::data::{StatusData, clock_icon};

fn data() -> StatusData<'static> {
    StatusData {
        clock_24: "14:32",
        clock_ampm: "2:32 pm",
        hour: 14,
        chip_balance: 1204,
        mentions_unread: 3,
        dms_unread: 2,
        pot_size: Some(84_200),
        pot_draws_in: Some("3h12m"),
        online_count: 12,
        turns_waiting: 2,
        station_name: Some("chillsynth"),
        station_track: Some("Artist - A Very Long Track Title"),
        quests_open_daily: 1,
        quests_open_weekly: 1,
        invites: 1,
        voice: Some("lounge [talking]"),
    }
}

fn on(component: StatusComponent, label: LabelMode) -> StatusComponentSetting {
    StatusComponentSetting {
        enabled: true,
        label,
        low_priority: false,
        ..StatusComponentSetting::new(component)
    }
}

/// The frame is 120 wide with an 18-cell title, i.e. no width pressure.
fn roomy(components: &[StatusComponentSetting]) -> String {
    render(components, 120)
}

fn render(components: &[StatusComponentSetting], width: u16) -> String {
    build_status_bar(
        components,
        &data(),
        Placement::TopRight,
        Rect::new(0, 0, width, 24),
        18,
    )
    .map(|bar| {
        bar.line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    })
    .unwrap_or_default()
}

/// Click rects and the fit-against-the-tabs math both come from measured
/// width, so an icon a terminal paints wider or narrower than ratatui measures
/// slides every hit target on the bar. Measured here with `Span::width`, the
/// same call ratatui lays cells out with. See `StatusComponent::icon`.
#[test]
fn every_icon_measures_two_cells() {
    for component in StatusComponent::ALL {
        let icon = if matches!(
            component,
            StatusComponent::Shortcuts | StatusComponent::Time
        ) {
            continue;
        } else {
            component.icon()
        };
        assert_eq!(
            Span::raw(icon).width(),
            2,
            "{} icon {icon:?} must measure two cells",
            component.as_str()
        );
    }
}

#[test]
fn fixed_topbar_reproduces_the_upstream_hud_independently_of_user_defaults() {
    let topbar = fixed_topbar_components();
    assert_eq!(
        topbar.map(|setting| setting.component),
        [
            StatusComponent::Voice,
            StatusComponent::Mentions,
            StatusComponent::Pot,
            StatusComponent::Chips,
        ]
    );
    assert!(topbar.iter().all(|setting| setting.enabled));
    assert!(
        topbar
            .iter()
            .all(|setting| { setting.low_priority == (setting.component == StatusComponent::Pot) })
    );
    assert!(!StatusComponentSetting::new(StatusComponent::Voice).enabled);
}

#[test]
fn fixed_topbar_reads_the_runners_rations_and_signal() {
    use crate::app::common::theme;
    use crate::app::deadchannel::fight::state::Sheet;

    let mut sheet = Sheet::fresh(
        uuid::Uuid::from_u128(1),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 25).expect("date"),
    );
    sheet.level = 2;
    sheet.signal = 12;
    sheet.rations_left = 7;
    let with_runner = |sheet: &Sheet, border_width: u16| {
        build_top_status_bar(
            &StatusData {
                chip_balance: 1_500,
                mentions_unread: 2,
                pot_size: Some(84_200),
                pot_draws_in: Some("3h12m"),
                ..StatusData::default()
            },
            Some(sheet),
            Rect::new(0, 0, border_width, 24),
            0,
        )
    };
    let full = " unread 2 ─ rations 7 · signal 12/20 ─ pot 84,200 · 3h12m ─ chips 1500 ─";
    let without_pot = " unread 2 ─ rations 7 · signal 12/20 ─ chips 1500 ─";
    let signal_only = " unread 2 ─ signal 12/20 ─ chips 1500 ─";
    // With the readout gone the pot has room for its size again.
    let without_runner = " unread 2 ─ pot 84,200 ─ chips 1500 ─";
    let width = |text: &str| Span::raw(text).width() as u16 + 2;

    let hud = with_runner(&sheet, 200).expect("hud");
    assert_eq!(hud.line.to_string(), full);
    let signal = hud
        .line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "12/20")
        .expect("signal span");
    assert_eq!(signal.style.fg, Some(theme::TEXT_BRIGHT()));
    assert_eq!(
        with_runner(&sheet, width(without_pot)).map(|hud| hud.line.to_string()),
        Some(without_pot.to_string()),
        "the pot sheds before the runner's readout"
    );
    assert_eq!(
        with_runner(&sheet, width(without_pot) - 1).map(|hud| hud.line.to_string()),
        Some(signal_only.to_string()),
        "one cell short drops the rations, not the signal"
    );
    assert_eq!(
        with_runner(&sheet, width(signal_only) - 1).map(|hud| hud.line.to_string()),
        Some(without_runner.to_string()),
        "too tight for the signal drops the readout"
    );

    sheet.signal = 0;
    let down = with_runner(&sheet, 200).expect("hud");
    let signal = down
        .line
        .spans
        .iter()
        .find(|span| span.content.as_ref() == "0/20")
        .expect("signal span");
    assert_eq!(signal.style.fg, Some(theme::ERROR()));
}

#[test]
fn keyhints_keeps_its_styled_bottom_left_copy_and_both_compactions() {
    let components = [StatusComponentSetting::new(StatusComponent::Shortcuts)];
    let area_for = |text: &str| Rect::new(0, 0, Span::raw(text).width() as u16 + 2, 24);
    let render_in = |area| {
        build_status_bar(&components, &data(), Placement::BottomLeft, area, 0)
            .expect("shortcut bar")
            .line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    };
    let full = "─ Settings Ctrl+O · Lobby Ctrl+G · Zen Ctrl+F · Shop Ctrl+S · Guide ? · Exit qq ";
    let spaced = "─ Settings Ctrl+O  Lobby Ctrl+G  Zen Ctrl+F  Shop Ctrl+S  Guide ?  Exit qq ";
    let caret = "─ Settings ^O  Lobby ^G  Zen ^F  Shop ^S  Guide ?  Exit qq ";

    assert_eq!(render_in(area_for(full)), full);
    assert_eq!(render_in(area_for(spaced)), spaced);
    assert_eq!(render_in(area_for(caret)), caret);
}

#[test]
fn brief_keyhints_fits_its_exact_glyphs_and_keeps_adjacent_click_targets_aligned() {
    let components = [
        StatusComponentSetting {
            brief: true,
            ..StatusComponentSetting::new(StatusComponent::Shortcuts)
        },
        on(StatusComponent::Chips, LabelMode::Text),
    ];
    let expected = "─ ⚙ ^o · ⚄ ^g · ◉ ^s ─ chips 1204 ";
    let width = Line::raw(expected).width() as u16;
    let area = Rect::new(7, 2, width + 2, 24);
    let bar = build_status_bar(&components, &data(), Placement::BottomLeft, area, 0)
        .expect("brief hints and chips fit exactly");
    assert_eq!(bar.line.to_string(), expected);
    assert_eq!(
        bar.hits,
        vec![(
            StatusComponent::Chips,
            Rect::new(
                8 + Line::raw("─ ⚙ ^o · ⚄ ^g · ◉ ^s ─").width() as u16,
                25,
                12,
                1
            ),
        )]
    );
    let brief_only = &components[..1];
    let brief_width = Line::raw("─ ⚙ ^o · ⚄ ^g · ◉ ^s ").width() as u16;
    assert!(
        build_status_bar(
            brief_only,
            &data(),
            Placement::BottomLeft,
            Rect::new(0, 0, brief_width + 1, 24),
            0,
        )
        .is_none(),
        "drop the whole hint when it cannot fit"
    );
}

#[test]
fn every_clock_face_measures_two_cells() {
    for hour in 0..24 {
        let icon = clock_icon(hour);
        assert_eq!(Span::raw(icon).width(), 2, "hour {hour} face {icon:?}");
    }
}

#[test]
fn clock_face_follows_the_hour_and_wraps_at_noon() {
    assert_eq!(clock_icon(0), "🕛");
    assert_eq!(clock_icon(3), "🕒");
    assert_eq!(clock_icon(12), clock_icon(0));
    assert_eq!(clock_icon(15), clock_icon(3));
}

#[test]
fn label_modes_pick_what_sits_beside_the_value() {
    assert_eq!(
        roomy(&[on(StatusComponent::Chips, LabelMode::Text)]),
        " chips 1204 ─"
    );
    assert_eq!(
        roomy(&[on(StatusComponent::Chips, LabelMode::Icon)]),
        " 🪙 1204 ─"
    );
    assert_eq!(
        roomy(&[on(StatusComponent::Chips, LabelMode::None)]),
        " 1204 ─"
    );
}

#[test]
fn pot_keeps_upstream_wording_and_compacts_before_it_drops() {
    let pot = StatusComponentSetting {
        low_priority: true,
        ..on(StatusComponent::Pot, LabelMode::Text)
    };
    let chips = on(StatusComponent::Chips, LabelMode::Text);
    let components = [pot, chips];
    let full = " pot 84,200 · 3h12m ─ chips 1204 ─";
    let compact = " pot 84,200 ─ chips 1204 ─";
    let without_pot = " chips 1204 ─";
    let width_for = |text: &str| 18 + 2 + text.chars().count() as u16;

    assert_eq!(render(&components, width_for(full)), full);
    assert_eq!(render(&components, width_for(full) - 1), compact);
    assert_eq!(
        render(&components, width_for(without_pot)),
        without_pot,
        "the low-priority pot drops before chips"
    );
}

/// The clock names itself, so a text label would paint a stray space rather
/// than a word.
#[test]
fn a_component_with_no_word_paints_no_text_label() {
    assert_eq!(
        roomy(&[on(StatusComponent::Time, LabelMode::Text)]),
        " 14:32 ─"
    );
    assert_eq!(
        roomy(&[on(StatusComponent::Time, LabelMode::Icon)]),
        " 🕑 14:32 ─"
    );
}

/// Separators are border glyphs, not pipes: the bar is painted over the
/// frame's border row and should read as that line running on through the
/// gaps, ending in a glyph rather than a blank next to the corner.
#[test]
fn separators_are_border_glyphs_supplied_by_the_layout_pass() {
    let bar = roomy(&[
        on(StatusComponent::Mentions, LabelMode::None),
        on(StatusComponent::Chips, LabelMode::None),
    ]);
    assert_eq!(bar, " 3 ─ 1204 ─");
}

/// A lone segment still meets the corner through a border glyph, and picks up
/// no separator it has no neighbour for.
#[test]
fn a_lone_segment_carries_one_edge_glyph_and_no_separator() {
    assert_eq!(
        roomy(&[on(StatusComponent::Chips, LabelMode::None)]),
        " 1204 ─"
    );
}

/// A bottom-left bar leads with its edge glyph instead of trailing it, so it
/// meets the corner it actually starts from.
#[test]
fn a_bottom_left_bar_leads_with_its_edge_glyph() {
    let bar = build_status_bar(
        &[on(StatusComponent::Chips, LabelMode::None)],
        &data(),
        Placement::BottomLeft,
        Rect::new(0, 0, 120, 24),
        18,
    )
    .expect("bar");
    let text = bar
        .line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>();
    assert_eq!(text, "─ 1204 ");
}

#[test]
fn bottom_left_hit_rects_follow_the_leading_edge_glyph() {
    let area = Rect::new(0, 0, 40, 24);
    let bar = build_status_bar(
        &[on(StatusComponent::Chips, LabelMode::None)],
        &data(),
        Placement::BottomLeft,
        area,
        0,
    )
    .expect("bar");

    assert_eq!(bar.hits.len(), 1);
    assert_eq!(bar.hits[0].0, StatusComponent::Chips);
    assert_eq!(bar.hits[0].1, Rect::new(2, area.bottom() - 1, 6, 1));
}

#[test]
fn disabled_components_paint_nothing() {
    let components = [StatusComponentSetting {
        enabled: false,
        ..StatusComponentSetting::new(StatusComponent::Chips)
    }];
    assert_eq!(roomy(&components), "");
}

#[test]
fn auto_hide_drops_a_component_reading_zero() {
    let mut data = data();
    data.mentions_unread = 0;
    data.dms_unread = 0;
    let components = [StatusComponentSetting {
        enabled: true,
        auto_hide: true,
        ..StatusComponentSetting::new(StatusComponent::Mentions)
    }];
    let bar = build_status_bar(
        &components,
        &data,
        Placement::TopRight,
        Rect::new(0, 0, 120, 24),
        18,
    );
    assert!(bar.is_none(), "an empty bar is no bar at all");
}

#[test]
fn auto_hide_off_shows_the_resting_reading_instead() {
    let mut data = data();
    data.mentions_unread = 0;
    data.dms_unread = 0;
    let components = [StatusComponentSetting {
        enabled: true,
        auto_hide: false,
        label: LabelMode::None,
        ..StatusComponentSetting::new(StatusComponent::Mentions)
    }];
    let bar = build_status_bar(
        &components,
        &data,
        Placement::TopRight,
        Rect::new(0, 0, 120, 24),
        18,
    )
    .expect("bar");
    assert_eq!(
        bar.line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>(),
        " 0 ─"
    );
}

#[test]
fn variants_pick_what_the_component_counts() {
    let mentions_only = StatusComponentSetting {
        variant: Some(StatusVariant::MentionsOnly),
        ..on(StatusComponent::Mentions, LabelMode::None)
    };
    let with_dms = StatusComponentSetting {
        variant: Some(StatusVariant::MentionsAndDms),
        ..on(StatusComponent::Mentions, LabelMode::None)
    };
    assert_eq!(roomy(&[mentions_only]), " 3 ─");
    assert_eq!(roomy(&[with_dms]), " 5 ─");
}

#[test]
fn the_clock_variant_switches_format() {
    let ampm = StatusComponentSetting {
        variant: Some(StatusVariant::ClockAmPm),
        ..on(StatusComponent::Time, LabelMode::None)
    };
    assert_eq!(roomy(&[ampm]), " 2:32 pm ─");
}

/// The station falls back to naming itself when the metadata feed has no
/// track, rather than going blank.
#[test]
fn the_track_variant_falls_back_to_the_station_name() {
    let mut data = data();
    data.station_track = None;
    let components = [StatusComponentSetting {
        variant: Some(StatusVariant::StationTrack),
        ..on(StatusComponent::Station, LabelMode::None)
    }];
    let bar = build_status_bar(
        &components,
        &data,
        Placement::TopRight,
        Rect::new(0, 0, 120, 24),
        18,
    )
    .expect("bar");
    assert!(
        bar.line
            .spans
            .iter()
            .any(|s| s.content.contains("chillsynth"))
    );
}

// --- fitting -------------------------------------------------------------

/// Under pressure a text-labelled segment gives up its word before anything is
/// dropped, so the reading survives even when the wording cannot.
#[test]
fn a_squeezed_bar_sheds_labels_before_segments() {
    let components = [
        on(StatusComponent::Mentions, LabelMode::Text),
        on(StatusComponent::Chips, LabelMode::Text),
    ];
    // " unread 3 ─ chips 1204 " is 23 cells; give it 18.
    let squeezed = render(&components, 18 + 18 + 2);
    assert!(squeezed.contains('3'), "mentions survived: {squeezed:?}");
    assert!(squeezed.contains("1204"), "chips survived: {squeezed:?}");
    assert!(
        !squeezed.contains("unread") || !squeezed.contains("chips"),
        "at least one label was shed: {squeezed:?}"
    );
}

/// The whole point of the tier: a low-priority segment is given up before a
/// normal one yields, wherever the two sit relative to each other.
#[test]
fn low_priority_segments_drop_before_normal_ones() {
    let components = [
        StatusComponentSetting {
            low_priority: true,
            ..on(StatusComponent::Users, LabelMode::None)
        },
        on(StatusComponent::Chips, LabelMode::None),
    ];
    // Room for one segment and its edge glyph only.
    let squeezed = render(&components, 18 + 9);
    assert_eq!(
        squeezed, " 1204 ─",
        "normal segment held, alone and unseparated"
    );
}

/// Tier beats position: the low-priority segment goes first even when a
/// normal-priority segment sits further left, nearer the colliding tabs.
#[test]
fn tier_outranks_position_when_dropping() {
    let components = [
        on(StatusComponent::Chips, LabelMode::None),
        StatusComponentSetting {
            low_priority: true,
            ..on(StatusComponent::Users, LabelMode::None)
        },
    ];
    let squeezed = render(&components, 18 + 9);
    assert_eq!(squeezed, " 1204 ─", "normal segment held");
}

/// Within a tier, the segment nearest the page tabs is the one that collides,
/// so it is the one that yields.
#[test]
fn within_a_tier_the_leftmost_segment_yields_first() {
    let components = [
        on(StatusComponent::Users, LabelMode::None),
        on(StatusComponent::Chips, LabelMode::None),
    ];
    // Exact, not `contains`: a dropped segment must take its separator with
    // it rather than leaving a stray glyph behind.
    let squeezed = render(&components, 18 + 9);
    assert_eq!(squeezed, " 1204 ─", "rightmost held and leftmost yielded");
}

/// A bottom-left bar collides with the sponsor line on its right, so it yields
/// from the other end.
#[test]
fn a_bottom_left_bar_yields_from_its_right_end() {
    let components = [
        on(StatusComponent::Users, LabelMode::None),
        on(StatusComponent::Chips, LabelMode::None),
    ];
    let bar = build_status_bar(
        &components,
        &data(),
        Placement::BottomLeft,
        Rect::new(0, 0, 18 + 8, 24),
        18,
    )
    .expect("bar");
    let text = bar
        .line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>();
    assert!(text.contains("12"), "leftmost held: {text:?}");
    assert!(!text.contains("1204"), "rightmost yielded: {text:?}");
}

#[test]
fn a_bar_with_no_room_at_all_disappears() {
    let components = [on(StatusComponent::Chips, LabelMode::Text)];
    assert_eq!(render(&components, 19), "");
}

// --- hit rects -----------------------------------------------------------

/// The rects must land exactly where the right-aligned line paints, or clicks
/// hit the wrong segment. Reproduces ratatui's placement: the line's last cell
/// sits just inside the top-right corner.
#[test]
fn hit_rects_track_the_right_aligned_line() {
    let components = [
        on(StatusComponent::Mentions, LabelMode::None),
        on(StatusComponent::Chips, LabelMode::None),
    ];
    let area = Rect::new(0, 0, 40, 24);
    let bar = build_status_bar(&components, &data(), Placement::TopRight, area, 10).expect("bar");

    // " 3 " + "─" + " 1204 " + "─" = 3 + 1 + 6 + 1 = 11 cells.
    let total = bar.line.width() as u16;
    assert_eq!(total, 11);
    let start = area.right() - total - 1;

    assert_eq!(bar.hits.len(), 2);
    assert_eq!(bar.hits[0].0, StatusComponent::Mentions);
    assert_eq!(bar.hits[0].1, Rect::new(start, 0, 3, 1));
    assert_eq!(bar.hits[1].0, StatusComponent::Chips);
    assert_eq!(bar.hits[1].1, Rect::new(start + 4, 0, 6, 1));
}

/// Reordering the list moves the rects with the segments; nothing else has to
/// know the order changed.
#[test]
fn reordering_the_list_moves_the_hit_rects() {
    let area = Rect::new(0, 0, 40, 24);
    let forward = [
        on(StatusComponent::Mentions, LabelMode::None),
        on(StatusComponent::Chips, LabelMode::None),
    ];
    let reversed = [forward[1], forward[0]];

    let a = build_status_bar(&forward, &data(), Placement::TopRight, area, 10).expect("bar");
    let b = build_status_bar(&reversed, &data(), Placement::TopRight, area, 10).expect("bar");

    assert_eq!(a.hits[0].0, StatusComponent::Mentions);
    assert_eq!(b.hits[0].0, StatusComponent::Chips);
    // Same widths, so the leading slot is the same cells either way.
    assert_eq!(a.hits[0].1.x, b.hits[0].1.x);
    assert_ne!(a.hits[0].1.width, b.hits[0].1.width);
}

/// Widening a segment (a longer value) must move everything after it, which is
/// the case a fixed-width layout would get wrong.
#[test]
fn a_wider_value_shifts_the_segments_after_it() {
    let components = [
        on(StatusComponent::Mentions, LabelMode::None),
        on(StatusComponent::Chips, LabelMode::None),
    ];
    let area = Rect::new(0, 0, 40, 24);
    let narrow = build_status_bar(&components, &data(), Placement::TopRight, area, 10).expect("a");

    let mut wide_data = data();
    wide_data.chip_balance = 1_000_000;
    let wide = build_status_bar(&components, &wide_data, Placement::TopRight, area, 10).expect("b");

    assert_eq!(wide.hits[1].1.width, narrow.hits[1].1.width + 3);
    // Right-aligned, so a wider tail pushes the head left instead.
    assert_eq!(wide.hits[0].1.x, narrow.hits[0].1.x - 3);
}

/// Readouts get no rect at all, so a click near them falls through to whatever
/// is behind rather than acting on the wrong segment.
#[test]
fn readout_components_get_no_hit_rect() {
    let components = [
        on(StatusComponent::Time, LabelMode::None),
        on(StatusComponent::Chips, LabelMode::None),
    ];
    let bar = build_status_bar(
        &components,
        &data(),
        Placement::TopRight,
        Rect::new(0, 0, 40, 24),
        10,
    )
    .expect("bar");
    assert_eq!(bar.hits.len(), 1);
    assert_eq!(bar.hits[0].0, StatusComponent::Chips);
}

#[test]
fn a_dropped_segment_leaves_no_hit_rect_behind() {
    let components = [
        StatusComponentSetting {
            low_priority: true,
            ..on(StatusComponent::Users, LabelMode::None)
        },
        on(StatusComponent::Chips, LabelMode::None),
    ];
    let bar = build_status_bar(
        &components,
        &data(),
        Placement::TopRight,
        Rect::new(0, 0, 18 + 9, 24),
        18,
    )
    .expect("bar");
    assert!(
        bar.hits.iter().all(|(c, _)| *c != StatusComponent::Users),
        "the dropped segment kept a rect"
    );
}

#[test]
fn click_actions_cover_exactly_the_actionable_components() {
    assert_eq!(
        click_action(StatusComponent::Mentions),
        Some(StatusClick::Mentions)
    );
    assert_eq!(
        click_action(StatusComponent::Chips),
        Some(StatusClick::Shop)
    );
    assert_eq!(click_action(StatusComponent::Time), None);
    assert_eq!(click_action(StatusComponent::Shortcuts), None);
    assert_eq!(click_action(StatusComponent::Voice), None);
    assert_eq!(click_action(StatusComponent::Pot), None);
}

/// A left title wider than the whole border row must not underflow the spare
/// calculation into a huge budget: segments are dropped, not force-fitted over
/// the page tabs.
#[test]
fn a_title_wider_than_the_border_row_does_not_underflow_the_budget() {
    let components = [on(StatusComponent::Chips, LabelMode::Text)];
    let bar = build_status_bar(
        &components,
        &data(),
        Placement::TopRight,
        Rect::new(0, 0, 10, 24),
        40,
    );
    assert!(bar.is_none(), "nothing fits behind an oversized title");
}
