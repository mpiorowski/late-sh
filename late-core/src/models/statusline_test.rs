use serde_json::{Value, json};

use super::statusline::{
    LabelMode, STATUS_COMPONENT_COUNT, StatusComponent, StatusComponentSetting, StatusVariant,
    default_statusline_components, normalize_statusline_components, parse_statusline_components,
    statusline_components_json,
};

fn find(
    components: &[StatusComponentSetting],
    component: StatusComponent,
) -> &StatusComponentSetting {
    components
        .iter()
        .find(|s| s.component == component)
        .expect("component present")
}

#[test]
fn default_list_covers_every_component_exactly_once() {
    let defaults = default_statusline_components();
    assert_eq!(defaults.len(), STATUS_COMPONENT_COUNT);
    for component in StatusComponent::ALL {
        assert_eq!(
            defaults.iter().filter(|s| s.component == component).count(),
            1,
            "{} appears once",
            component.as_str()
        );
    }
}

/// A user without a stored list keeps the longstanding bottom-left keyboard
/// hint. Status readings are opt-in because the fixed top bar already carries
/// the upstream HUD.
#[test]
fn default_bottom_bar_is_keyhints() {
    let enabled: Vec<StatusComponent> = default_statusline_components()
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| s.component)
        .collect();
    assert_eq!(enabled, vec![StatusComponent::Shortcuts]);
}

/// The keyboard hint keeps its established place while every opt-in status
/// component starts in the low-priority tier.
#[test]
fn opt_in_status_components_start_low_priority() {
    for setting in default_statusline_components() {
        assert_eq!(
            setting.low_priority,
            setting.component != StatusComponent::Shortcuts,
            "{} priority tier",
            setting.component.as_str()
        );
    }
}

/// `Time` supplies its icon at render time and the keyboard shortcuts are a
/// pre-styled hint rather than an icon/value pair, so every status component
/// other than those must carry an icon. The two-cells-wide invariant is
/// locked in `late-ssh` against ratatui's own measuring function.
#[test]
fn every_value_component_but_time_carries_an_icon() {
    for component in StatusComponent::ALL {
        assert_eq!(
            component.icon().is_empty(),
            matches!(
                component,
                StatusComponent::Shortcuts | StatusComponent::Time
            ),
            "{} icon",
            component.as_str()
        );
    }
}

#[test]
fn normalize_drops_duplicates_and_keeps_stored_order() {
    let stored = vec![
        StatusComponentSetting {
            enabled: true,
            ..StatusComponentSetting::new(StatusComponent::Chips)
        },
        StatusComponentSetting {
            enabled: false,
            ..StatusComponentSetting::new(StatusComponent::Mentions)
        },
        // Duplicate with a different reading: the first entry wins.
        StatusComponentSetting {
            enabled: false,
            ..StatusComponentSetting::new(StatusComponent::Chips)
        },
    ];

    let normalized = normalize_statusline_components(&stored);
    assert_eq!(normalized.len(), STATUS_COMPONENT_COUNT);
    assert_eq!(normalized[0].component, StatusComponent::Shortcuts);
    assert!(normalized[0].enabled);
    assert_eq!(normalized[1].component, StatusComponent::Chips);
    assert!(normalized[1].enabled);
    assert_eq!(normalized[2].component, StatusComponent::Mentions);
    assert!(!normalized[2].enabled);
}

/// Backfill diverges from the sidebar's blanket enable. Only the longstanding
/// keyboard hint is required; optional status components stay off in an
/// existing custom roster.
#[test]
fn normalize_backfills_each_component_at_its_own_policy() {
    let stored = vec![StatusComponentSetting::new(StatusComponent::Chips)];
    let normalized = normalize_statusline_components(&stored);

    assert_eq!(normalized.len(), STATUS_COMPONENT_COUNT);
    assert_eq!(normalized[0].component, StatusComponent::Shortcuts);
    assert!(normalized[0].enabled);
    assert_eq!(normalized[1].component, StatusComponent::Chips);
    for setting in normalized.iter().skip(2) {
        assert_eq!(
            setting.enabled,
            setting.component.backfill_existing(),
            "{} backfilled at its own policy",
            setting.component.as_str()
        );
        assert!(
            !setting.enabled,
            "only the keyboard shortcuts force themselves on"
        );
    }
}

#[test]
fn normalize_repairs_a_variant_that_does_not_belong_to_the_component() {
    let stored = vec![StatusComponentSetting {
        // A clock variant stored against the quests component: stale data, not
        // a reason to drop the component.
        variant: Some(StatusVariant::Clock24),
        ..StatusComponentSetting::new(StatusComponent::Quests)
    }];

    let normalized = normalize_statusline_components(&stored);
    assert_eq!(
        find(&normalized, StatusComponent::Quests).variant,
        Some(StatusVariant::QuestsDaily)
    );
}

#[test]
fn normalize_clears_auto_hide_on_components_that_cannot_hide() {
    let stored = vec![StatusComponentSetting {
        auto_hide: true,
        ..StatusComponentSetting::new(StatusComponent::Users)
    }];

    let normalized = normalize_statusline_components(&stored);
    assert!(!StatusComponent::Users.can_auto_hide());
    assert!(!find(&normalized, StatusComponent::Users).auto_hide);
}

#[test]
fn parse_skips_unknown_keys_and_falls_back_per_field() {
    let values = vec![
        json!({"key": "not_a_component", "enabled": true}),
        // Every dial absent: each one falls back to that component's default
        // rather than to a blanket value.
        json!({"key": "time"}),
        json!({"key": "chips", "enabled": false, "label": "bogus"}),
    ];

    let parsed = parse_statusline_components(&values);
    assert_eq!(parsed.len(), STATUS_COMPONENT_COUNT);
    assert_eq!(parsed[0].component, StatusComponent::Shortcuts);
    assert_eq!(parsed[1].component, StatusComponent::Time);

    let time = find(&parsed, StatusComponent::Time);
    assert_eq!(time.enabled, StatusComponent::Time.default_enabled());
    assert_eq!(time.label, LabelMode::None);
    assert_eq!(time.variant, Some(StatusVariant::Clock24));

    let chips = find(&parsed, StatusComponent::Chips);
    assert!(!chips.enabled);
    assert_eq!(chips.label, LabelMode::Text, "unreadable label mode");
    assert_eq!(chips.variant, None, "chips has no dial");
}

#[test]
fn json_round_trips_through_parse() {
    let mut components = default_statusline_components();
    components.swap(0, 3);
    components[0].enabled = true;
    components[0].label = LabelMode::Icon;
    components[0].low_priority = true;
    let time = components
        .iter_mut()
        .find(|s| s.component == StatusComponent::Time)
        .expect("time present");
    time.variant = Some(StatusVariant::ClockAmPm);
    components
        .iter_mut()
        .find(|s| s.component == StatusComponent::Shortcuts)
        .expect("keyhints present")
        .brief = true;

    let json = statusline_components_json(&components);
    let values = json.as_array().expect("array").clone();
    assert_eq!(parse_statusline_components(&values), components);
}

#[test]
fn parse_of_an_empty_array_yields_the_full_default_list() {
    // An empty stored array is a customized bar with everything removed, which
    // normalize backfills rather than leaving the user with no roster at all.
    let parsed = parse_statusline_components(&[] as &[Value]);
    assert_eq!(parsed.len(), STATUS_COMPONENT_COUNT);
    assert!(find(&parsed, StatusComponent::Shortcuts).enabled);
    assert!(
        parsed
            .iter()
            .filter(|setting| setting.component != StatusComponent::Shortcuts)
            .all(|setting| !setting.enabled),
        "optional backfilled entries stay off"
    );
}

#[test]
fn brief_defaults_off_and_only_applies_to_keyhints() {
    assert!(!StatusComponentSetting::new(StatusComponent::Shortcuts).brief);
    let parsed = parse_statusline_components(&[
        json!({"key": "shortcuts"}),
        json!({"key": "chips", "brief": true}),
    ]);
    assert!(!find(&parsed, StatusComponent::Shortcuts).brief);
    assert!(!find(&parsed, StatusComponent::Chips).brief);
}

#[test]
fn saved_brief_selection_becomes_a_keyhints_property_in_place() {
    let parsed = parse_statusline_components(&[
        json!({"key": "shortcuts", "enabled": false}),
        json!({"key": "chips", "enabled": true}),
        json!({"key": "keyhints_brief", "enabled": true, "low_priority": true}),
    ]);
    assert_eq!(parsed[0].component, StatusComponent::Chips);
    assert_eq!(parsed[1].component, StatusComponent::Shortcuts);
    assert!(parsed[1].enabled);
    assert!(parsed[1].brief);
    assert!(parsed[1].low_priority);
    let stored = statusline_components_json(&parsed);
    assert_eq!(stored[1]["key"], "shortcuts");
    assert_eq!(stored[1]["brief"], true);
    assert_eq!(
        parse_statusline_components(stored.as_array().unwrap()),
        parsed
    );
}

#[test]
fn merging_saved_keyhints_uses_full_when_enabled_and_preserves_disabled_state() {
    for full_enabled in [false, true] {
        for brief_enabled in [false, true] {
            let parsed = parse_statusline_components(&[
                json!({"key": "keyhints_brief", "enabled": brief_enabled}),
                json!({"key": "shortcuts", "enabled": full_enabled}),
            ]);
            let hints = find(&parsed, StatusComponent::Shortcuts);
            assert_eq!(hints.enabled, full_enabled || brief_enabled);
            assert_eq!(hints.brief, !full_enabled && brief_enabled);
            assert_eq!(parsed.len(), STATUS_COMPONENT_COUNT);
        }
    }
    let parsed = parse_statusline_components(&[json!({"key": "keyhints_brief"})]);
    let hints = find(&parsed, StatusComponent::Shortcuts);
    assert!(!hints.enabled);
    assert!(hints.brief);
}

#[test]
fn label_mode_cycles_both_ways_and_wraps() {
    assert_eq!(LabelMode::Text.cycle(true), LabelMode::Icon);
    assert_eq!(LabelMode::Icon.cycle(true), LabelMode::None);
    assert_eq!(LabelMode::None.cycle(true), LabelMode::Text);
    assert_eq!(LabelMode::Text.cycle(false), LabelMode::None);
}

#[test]
fn component_keys_and_variant_keys_round_trip() {
    for component in StatusComponent::ALL {
        assert_eq!(
            StatusComponent::from_key(component.as_str()),
            Some(component)
        );
        for variant in component.variants() {
            assert_eq!(StatusVariant::from_key(variant.as_str()), Some(*variant));
        }
    }
}

/// A component with a dial must also name it, or the customizer's detail pane
/// has a control with no heading.
#[test]
fn every_component_with_variants_has_a_variant_title() {
    for component in StatusComponent::ALL {
        assert_eq!(
            component.variants().is_empty(),
            component.variant_title().is_none(),
            "{} dial and heading disagree",
            component.as_str()
        );
    }
}
