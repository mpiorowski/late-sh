use late_core::models::statusline::{
    LabelMode, StatusComponent, StatusComponentSetting, StatusVariant,
};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::bar::{
    Placement, StatusClick, build_status_bar, build_top_status_bar, build_zen_status_row,
    click_action, fixed_topbar_components, zen_keys_line,
};
use super::data::{StatusData, clock_icon};

fn data() -> StatusData<'static> {
    StatusData {
        clock_24: "14:32",
        clock_ampm: "2:32 pm",
        hour: 14,
        date_short: "Thu 1 Oct",
        date_full: "Thursday, 1 October",
        date_iso: "2026-10-01",
        chip_balance: 1204,
        mentions_unread: 3,
        dms_unread: 2,
        pot_size: Some(84_200),
        pot_draws_in: Some("3h12m"),
        online_count: 12,
        turns_waiting: 2,
        station_name: "chillsynth",
        station_track: Some("Artist - A Very Long Track Title"),
        quests_open_daily: 1,
        quests_open_weekly: 1,
        care_due: 1,
        voice: Some("lounge [talking]"),
        live: Some("stream mat · late night rust"),
    }
}

fn on(component: StatusComponent, label: LabelMode) -> StatusComponentSetting {
    StatusComponentSetting {
        enabled: true,
        label,
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

/// The top-right bar is fixed policy: the two ambient readings, the pot
/// ahead of the balance.
#[test]
fn fixed_topbar_is_the_pot_and_the_chips() {
    let topbar = fixed_topbar_components();
    assert_eq!(
        topbar.map(|setting| setting.component),
        [StatusComponent::Pot, StatusComponent::Chips]
    );
    assert!(topbar.iter().all(|setting| setting.enabled));
}

/// The chips sit in the corner and claim room first; the pot paints whole
/// beside them or not at all.
#[test]
fn fixed_topbar_renders_the_pot_before_chips_and_drops_it_whole() {
    let hud = |border_width: u16| {
        build_top_status_bar(
            &StatusData {
                chip_balance: 1_500,
                // Mentions and voice are bottom-bar readings: never painted here.
                mentions_unread: 2,
                voice: Some("#lounge [muted]"),
                pot_size: Some(84_200),
                pot_draws_in: Some("3h12m"),
                ..StatusData::default()
            },
            Rect::new(0, 0, border_width, 24),
            0,
            &[],
        )
        .map(|hud| hud.line.to_string())
    };
    let full = " pot 84,200 · 3h12m ─ chips 1500 ─";
    let without_pot = " chips 1500 ─";
    let width = |text: &str| Span::raw(text).width() as u16 + 2;

    assert_eq!(hud(200).as_deref(), Some(full));
    assert_eq!(hud(width(full)).as_deref(), Some(full), "fits exactly");
    assert_eq!(
        hud(width(full) - 1).as_deref(),
        Some(without_pot),
        "one column short and the pot goes whole, countdown and all"
    );
    assert_eq!(hud(width(without_pot) - 1), None);
}

/// Placing the pot or the chips on the bottom bar moves the reading there:
/// the top bar skips what the bottom one painted, and only that. A segment
/// the bottom bar had to drop for width stays on the top.
#[test]
fn a_reading_painted_on_the_bottom_bar_leaves_the_top_one() {
    let data = StatusData {
        chip_balance: 1_500,
        pot_size: Some(84_200),
        pot_draws_in: Some("3h12m"),
        ..StatusData::default()
    };
    let components = [
        StatusComponentSetting::new(StatusComponent::Shortcuts),
        on(StatusComponent::Chips, LabelMode::Text),
    ];
    let top = |area: Rect, painted_on_bottom: &[StatusComponent]| {
        build_top_status_bar(&data, area, 0, painted_on_bottom).map(|bar| bar.line.to_string())
    };

    let roomy = Rect::new(0, 0, 200, 24);
    let bottom =
        build_status_bar(&components, &data, Placement::BottomLeft, roomy, 0).expect("bottom bar");
    assert_eq!(
        bottom.painted,
        vec![StatusComponent::Shortcuts, StatusComponent::Chips]
    );
    assert_eq!(
        top(roomy, &bottom.painted).as_deref(),
        Some(" pot 84,200 · 3h12m ─"),
        "chips moved down, the pot stayed up"
    );

    // Room for the hints only: the chips never made it onto the bottom bar.
    let hints_only = "─ Settings ^O  Lobby ^G  Zen ^F  Shop ^S  Guide ?  Exit qq ";
    let tight = Rect::new(0, 0, Span::raw(hints_only).width() as u16 + 2, 24);
    let bottom =
        build_status_bar(&components, &data, Placement::BottomLeft, tight, 0).expect("bottom bar");
    assert_eq!(bottom.painted, vec![StatusComponent::Shortcuts]);
    assert_eq!(
        top(roomy, &bottom.painted).as_deref(),
        Some(" pot 84,200 · 3h12m ─ chips 1500 ─")
    );

    assert_eq!(
        top(roomy, &[StatusComponent::Pot, StatusComponent::Chips]),
        None,
        "nothing left for the top bar to say"
    );
}

/// The bar adds the `mic` label and the padding itself, so the badge it is
/// handed must be the bare body. Fed from the real badge function so the two
/// cannot drift apart again.
#[test]
fn the_voice_badge_is_labelled_once() {
    use crate::app::voice::svc::{VoiceParticipant, VoiceSnapshot};

    let room_id = uuid::Uuid::from_u128(42);
    let user_id = uuid::Uuid::from_u128(7);
    let snapshot = VoiceSnapshot {
        enabled: true,
        livekit_url: None,
        rooms: [(
            room_id,
            vec![VoiceParticipant {
                user_id,
                username: "tester".to_string(),
                muted: true,
                deafened: false,
                speaking: false,
                updated_at: chrono::Utc::now(),
            }],
        )]
        .into_iter()
        .collect(),
    };
    let badge = crate::app::voice::ui::global_voice_badge(&snapshot, user_id, |_| {
        Some("#lounge".to_string())
    });

    let bar = build_status_bar(
        &[
            StatusComponentSetting::new(StatusComponent::Shortcuts),
            StatusComponentSetting::new(StatusComponent::Voice),
        ],
        &StatusData {
            voice: badge.as_deref(),
            ..StatusData::default()
        },
        Placement::BottomLeft,
        Rect::new(0, 0, 200, 24),
        0,
    )
    .expect("bar");
    assert_eq!(
        bar.line.to_string(),
        "─ Settings ^O  Lobby ^G  Zen ^F  Shop ^S  Guide ?  Exit qq ─ mic #lounge [muted] "
    );
}

/// Keyhints have one rendering, caret notation, and are never shortened.
#[test]
fn keyhints_paint_caret_notation_or_nothing() {
    let components = [StatusComponentSetting::new(StatusComponent::Shortcuts)];
    let caret = "─ Settings ^O  Lobby ^G  Zen ^F  Shop ^S  Guide ?  Exit qq ";
    let width = Span::raw(caret).width() as u16 + 2;
    let render_in = |width: u16| {
        build_status_bar(
            &components,
            &data(),
            Placement::BottomLeft,
            Rect::new(0, 0, width, 24),
            0,
        )
        .map(|bar| bar.line.to_string())
    };

    assert_eq!(render_in(200).as_deref(), Some(caret));
    assert_eq!(render_in(width).as_deref(), Some(caret));
    assert_eq!(render_in(width - 1), None);
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
            StatusClick::Shop,
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
    // The fixture's 3 mentions and 2 unread DMs, which the default counts too.
    assert_eq!(bar, " 5 ─ 1204 ─");
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
    assert_eq!(bar.hits[0].0, StatusClick::Shop);
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

/// Care counts the companions still waiting on today's care. With auto-hide
/// on, as it ships, it leaves the bar once everything is tended; off, it
/// rests at zero.
#[test]
fn care_shows_what_is_still_due() {
    let always = StatusComponentSetting {
        enabled: true,
        auto_hide: false,
        ..StatusComponentSetting::new(StatusComponent::Care)
    };
    let auto_hiding = StatusComponentSetting {
        auto_hide: true,
        ..always
    };
    let render = |setting: StatusComponentSetting, care_due: usize| {
        build_status_bar(
            &[setting],
            &StatusData {
                care_due,
                ..StatusData::default()
            },
            Placement::BottomLeft,
            Rect::new(0, 0, 120, 24),
            0,
        )
        .map(|bar| bar.line.to_string())
    };

    assert_eq!(render(always, 2).as_deref(), Some("─ care 2 "));
    assert_eq!(render(always, 0).as_deref(), Some("─ care 0 "));
    assert_eq!(render(auto_hiding, 0), None);
}

/// Auto-hide is offered exactly where it can do something: on an idle frame a
/// component either reads inactive or it always has a reading, never both.
#[test]
fn auto_hide_is_offered_exactly_where_a_component_can_read_inactive() {
    let idle = StatusData::default();
    for component in StatusComponent::ALL {
        // Keyhints is styled help copy, not a value reading.
        if component == StatusComponent::Shortcuts {
            continue;
        }
        let variant = component.variants().first().copied();
        assert_eq!(
            idle.value(component, variant).is_none(),
            component.can_auto_hide(),
            "{}",
            component.as_str()
        );
    }
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

/// Every title on the bar (the station's track, the live line) is cut to the
/// one cap, so a long track or headline shortens instead of dropping the
/// segment it rides.
#[test]
fn titles_are_cut_to_the_one_cap() {
    let track = StatusComponentSetting {
        variant: Some(StatusVariant::StationTrack),
        ..on(StatusComponent::Station, LabelMode::None)
    };
    let live = on(StatusComponent::Live, LabelMode::None);
    let data = StatusData {
        station_track: Some("No High Scores Here - Secret of Mana OST"),
        live: Some("youtube @mat · No High Scores Here - Secret of Mana"),
        ..data()
    };

    assert_eq!(
        data.value(StatusComponent::Station, track.variant)
            .as_deref(),
        Some("No High Scores Here…")
    );
    assert_eq!(
        data.value(StatusComponent::Live, live.variant).as_deref(),
        Some("youtube @mat · No H…")
    );
    assert_eq!(
        data.value(StatusComponent::Station, Some(StatusVariant::StationName))
            .as_deref(),
        Some("chillsynth")
    );
}

// --- fitting -------------------------------------------------------------

/// A segment with no room is dropped whole: its wording is never shortened
/// to squeeze it in.
#[test]
fn a_segment_that_does_not_fit_is_dropped_not_shortened() {
    let components = [
        on(StatusComponent::Users, LabelMode::Text),
        on(StatusComponent::Chips, LabelMode::Text),
    ];
    let both = " online 12 ─ chips 1204 ─";
    let width_for = |text: &str| 18 + 2 + Span::raw(text).width() as u16;

    assert_eq!(render(&components, width_for(both)), both);
    // Exact, not `contains`: a dropped segment takes its separator with it.
    assert_eq!(render(&components, width_for(both) - 1), " chips 1204 ─");
}

/// The list order is the only priority. Room goes to the corner end first, so
/// a top-right bar keeps its rightmost segment and a bottom-left bar its
/// leftmost.
#[test]
fn the_corner_end_of_the_list_claims_room_first() {
    let components = [
        on(StatusComponent::Users, LabelMode::None),
        on(StatusComponent::Chips, LabelMode::None),
    ];
    // Room for one segment and its edge glyph only.
    assert_eq!(render(&components, 18 + 9), " 1204 ─");

    let bottom = build_status_bar(
        &components,
        &data(),
        Placement::BottomLeft,
        Rect::new(0, 0, 18 + 8, 24),
        18,
    )
    .expect("bar");
    assert_eq!(bottom.line.to_string(), "─ 12 ");
}

/// A segment too wide for the room left does not block a narrower one after
/// it: each is kept or dropped on its own.
#[test]
fn a_narrower_segment_still_fits_after_a_wide_one_is_dropped() {
    let components = [
        on(StatusComponent::Chips, LabelMode::None),
        on(StatusComponent::Station, LabelMode::Text),
        on(StatusComponent::Users, LabelMode::None),
    ];
    let without_station = "─ 1204 ─ 12 ";
    let bar = build_status_bar(
        &components,
        &data(),
        Placement::BottomLeft,
        Rect::new(0, 0, Span::raw(without_station).width() as u16 + 2 + 5, 24),
        0,
    )
    .expect("bar");
    assert_eq!(bar.line.to_string(), without_station);
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
    assert_eq!(bar.hits[0].0, StatusClick::Mentions);
    assert_eq!(bar.hits[0].1, Rect::new(start, 0, 3, 1));
    assert_eq!(bar.hits[1].0, StatusClick::Shop);
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

    assert_eq!(a.hits[0].0, StatusClick::Mentions);
    assert_eq!(b.hits[0].0, StatusClick::Shop);
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
    assert_eq!(bar.hits[0].0, StatusClick::Shop);
}

#[test]
fn a_dropped_segment_leaves_no_hit_rect_behind() {
    let components = [
        on(StatusComponent::Users, LabelMode::None),
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
        bar.hits.iter().all(|(c, _)| *c != StatusClick::Profiles),
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
    assert_eq!(click_action(StatusComponent::Care), Some(StatusClick::Zen));
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

/// Zen's row: the keys hold the right end, and the bar fits into the rest,
/// dots between segments, a segment with no room dropped whole.
#[test]
fn zen_row_fits_the_bar_beside_the_keys() {
    let components = [
        on(StatusComponent::Mentions, LabelMode::Text),
        on(StatusComponent::Chips, LabelMode::Text),
        on(StatusComponent::Users, LabelMode::Text),
    ];
    let keys = zen_keys_line();
    assert_eq!(keys.to_string(), "? help  S split  F flip  X close  z zoom");
    // 30 cells for the bar, one of gap, then the keys.
    let row = Rect::new(0, 39, 30 + 1 + keys.width() as u16, 1);
    let zen_row = build_zen_status_row(&components, &data(), row);

    assert_eq!(zen_row.keys.expect("keys").to_string(), keys.to_string());
    let bar = zen_row.bar.expect("bar");
    assert_eq!(bar.to_string(), " unread 5 · chips 1204 ");
    assert_eq!(
        zen_row.hits,
        vec![
            (StatusClick::Mentions, Rect::new(0, 39, 10, 1)),
            (StatusClick::Shop, Rect::new(11, 39, 12, 1)),
        ]
    );
}

/// A row narrower than the keys gives the bar the whole row.
#[test]
fn zen_row_too_narrow_for_the_keys_is_all_bar() {
    let components = [on(StatusComponent::Mentions, LabelMode::Text)];
    let zen_row = build_zen_status_row(&components, &data(), Rect::new(0, 39, 30, 1));
    assert!(zen_row.keys.is_none());
    assert_eq!(zen_row.bar.expect("bar").to_string(), " unread 5 ");
}

/// With the bar silent (auto-hidden, or every component off) the keys are
/// still there and nothing is clickable.
#[test]
fn zen_row_keeps_its_keys_while_the_bar_is_silent() {
    let quiet = StatusData {
        mentions_unread: 0,
        dms_unread: 0,
        ..data()
    };
    let auto_hidden = [StatusComponentSetting {
        auto_hide: true,
        ..on(StatusComponent::Mentions, LabelMode::Text)
    }];
    let all_off = StatusComponent::ALL.map(|component| StatusComponentSetting {
        enabled: false,
        ..StatusComponentSetting::new(component)
    });
    for components in [&auto_hidden[..], &all_off[..]] {
        let zen_row = build_zen_status_row(components, &quiet, Rect::new(0, 39, 80, 1));
        assert!(zen_row.bar.is_none());
        assert!(zen_row.keys.is_some());
        assert!(zen_row.hits.is_empty());
    }
}

/// The date paints in the format its dial picks, bare like the clock, and is
/// a readout: no click target.
#[test]
fn the_date_paints_the_format_its_dial_picks() {
    let render = |variant: StatusVariant| {
        let components = [StatusComponentSetting {
            variant: Some(variant),
            ..StatusComponentSetting::new(StatusComponent::Date)
        }];
        build_status_bar(
            &components,
            &data(),
            Placement::BottomLeft,
            Rect::new(0, 0, 120, 24),
            0,
        )
        .map(|bar| (bar.line.to_string(), bar.hits.len()))
    };

    assert_eq!(
        render(StatusVariant::DateShort),
        Some(("─ Thu 1 Oct ".to_string(), 0))
    );
    assert_eq!(
        render(StatusVariant::DateFull),
        Some(("─ Thursday, 1 October ".to_string(), 0))
    );
    assert_eq!(
        render(StatusVariant::DateIso),
        Some(("─ 2026-10-01 ".to_string(), 0))
    );
}

/// The Live segment reads what the live strip shows and a click opens it;
/// with the strip down it rests at `-`, or hides when told to.
#[test]
fn live_reads_the_strip_and_rests_or_hides_when_it_is_down() {
    let always = StatusComponentSetting::new(StatusComponent::Live);
    let auto_hiding = StatusComponentSetting {
        auto_hide: true,
        ..always
    };
    let quiet = StatusData {
        live: None,
        ..data()
    };
    let render = |setting: StatusComponentSetting, data: &StatusData<'_>| {
        build_status_bar(
            &[setting],
            data,
            Placement::BottomLeft,
            Rect::new(0, 0, 120, 24),
            0,
        )
        .map(|bar| bar.line.to_string())
    };

    assert_eq!(
        render(always, &data()).as_deref(),
        Some("─ now stream mat · late n… ")
    );
    assert_eq!(render(always, &quiet).as_deref(), Some("─ now - "));
    assert_eq!(render(auto_hiding, &quiet), None);
    assert_eq!(click_action(StatusComponent::Live), Some(StatusClick::Live));
    assert_eq!(click_action(StatusComponent::Date), None);
}
