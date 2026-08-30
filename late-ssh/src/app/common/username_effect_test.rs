use chrono::Duration;
use chrono::Utc;
use late_core::models::username_effect::UsernameEffect;
use late_core::models::username_effect::{GlowColor, GradientPair};
use ratatui::style::{Color, Style};
use uuid::Uuid;

use crate::app::common::username_effect::*;

#[test]
fn char_color_hits_gradient_endpoints() {
    let style = NameStyle::TwoTone(Color::Rgb(0, 0, 0), Color::Rgb(200, 100, 50));
    assert_eq!(char_color(style, 0, 8), Color::Rgb(0, 0, 0));
    assert_eq!(char_color(style, 7, 8), Color::Rgb(200, 100, 50));
}

#[test]
fn char_color_single_char_does_not_divide_by_zero() {
    let style = NameStyle::TwoTone(Color::Rgb(10, 20, 30), Color::Rgb(200, 100, 50));
    assert_eq!(char_color(style, 0, 1), Color::Rgb(10, 20, 30));
    let solid = NameStyle::Solid(Color::Rgb(1, 2, 3));
    assert_eq!(char_color(solid, 0, 1), Color::Rgb(1, 2, 3));
}

#[test]
fn shimmer_cycles_with_period_six_and_moving_endpoints() {
    for phase in 0..12 {
        let NameStyle::TwoTone(from, to) = resolve(UsernameEffect::Shimmer, phase) else {
            panic!("shimmer must resolve to a two-tone style");
        };
        let NameStyle::TwoTone(next_from, _) = resolve(UsernameEffect::Shimmer, phase + 1) else {
            panic!("shimmer must resolve to a two-tone style");
        };
        // The trailing endpoint becomes the next phase's leading one.
        assert_eq!(to, next_from);
        assert_eq!(
            resolve(UsernameEffect::Shimmer, phase + 6),
            NameStyle::TwoTone(from, to)
        );
    }
}

#[test]
fn glow_and_gradient_resolve_ignore_phase() {
    let glow = UsernameEffect::Glow(GlowColor::Sky);
    assert_eq!(resolve(glow, 0), resolve(glow, 99));
    let gradient = UsernameEffect::Gradient(GradientPair::Ocean);
    assert_eq!(resolve(gradient, 0), resolve(gradient, 99));
}

#[test]
fn directory_set_expire_replace() {
    let directory = new_directory();
    let user = Uuid::now_v7();
    let other = Uuid::now_v7();
    let now = Utc::now();

    set_user(
        &directory,
        user,
        NameFlair {
            effect: Some(FlairEffect {
                effect: UsernameEffect::Shimmer,
                ends_at: now + Duration::hours(24),
            }),
            title: None,
            milestone: None,
        },
    );
    set_user(
        &directory,
        other,
        NameFlair {
            effect: Some(FlairEffect {
                effect: UsernameEffect::Glow(GlowColor::Ember),
                ends_at: now - Duration::seconds(1),
            }),
            title: None,
            milestone: None,
        },
    );

    let resolved = resolve_all(&snapshot(&directory), None, 0, now);
    assert!(resolved[&user].style.is_some());
    assert!(
        !resolved.contains_key(&other),
        "expired flair must be skipped"
    );

    set_user(&directory, user, NameFlair::default());
    assert!(!snapshot(&directory).contains_key(&user));

    set_all(
        &directory,
        vec![(
            other,
            NameFlair {
                effect: Some(FlairEffect {
                    effect: UsernameEffect::Gradient(GradientPair::Candy),
                    ends_at: now + Duration::hours(1),
                }),
                title: None,
                milestone: None,
            },
        )],
    );
    let entries = snapshot(&directory);
    assert_eq!(entries.len(), 1);
    assert!(entries.contains_key(&other));
}

#[test]
fn title_and_color_resolve_and_expire_independently() {
    let directory = new_directory();
    let user = Uuid::now_v7();
    let now = Utc::now();

    set_user(
        &directory,
        user,
        NameFlair {
            effect: Some(FlairEffect {
                effect: UsernameEffect::Glow(GlowColor::Sky),
                ends_at: now + Duration::hours(1),
            }),
            title: Some(FlairTitle {
                text: "the insufferable".to_string(),
                ends_at: now + Duration::hours(2),
            }),
            milestone: None,
        },
    );

    let resolved = resolve_all(&snapshot(&directory), None, 0, now);
    assert_eq!(
        resolved[&user].style,
        Some(NameStyle::Solid(Color::Rgb(120, 180, 255)))
    );
    assert_eq!(resolved[&user].title.as_deref(), Some("the insufferable"));

    // The color lapses first: the title outlives it, and the entry stays.
    let later = now + Duration::minutes(90);
    let resolved = resolve_all(&snapshot(&directory), None, 0, later);
    assert_eq!(resolved[&user].style, None);
    assert_eq!(resolved[&user].title.as_deref(), Some("the insufferable"));

    // Both lapsed: the user drops out of the resolved map entirely.
    let much_later = now + Duration::hours(3);
    assert!(!resolve_all(&snapshot(&directory), None, 0, much_later).contains_key(&user));
}

/// The crown is not a rental, so it has no directory entry of its own: a
/// holder who has never bought anything still has to reach the renderers,
/// and losing the crown must not take a live effect down with it.
#[test]
fn the_crown_resolves_for_a_holder_with_nothing_else_bought() {
    let directory = new_directory();
    let bare_holder = Uuid::now_v7();
    let decorated = Uuid::now_v7();
    let now = Utc::now();

    set_user(
        &directory,
        decorated,
        NameFlair {
            effect: Some(FlairEffect {
                effect: UsernameEffect::Glow(GlowColor::Gold),
                ends_at: now + Duration::hours(1),
            }),
            title: None,
            milestone: None,
        },
    );

    // A holder with an empty wallet's worth of flair still gets an entry.
    let resolved = resolve_all(&snapshot(&directory), Some(bare_holder), 0, now);
    assert!(resolved[&bare_holder].crown);
    assert_eq!(resolved[&bare_holder].style, None);
    assert!(!resolved[&decorated].crown);

    // The crown is additive: taking it never disturbs a live effect.
    let resolved = resolve_all(&snapshot(&directory), Some(decorated), 0, now);
    assert!(resolved[&decorated].crown);
    assert!(resolved[&decorated].style.is_some());
    assert!(!resolved.contains_key(&bare_holder));

    // A vacant crown leaves nobody wearing it, and nobody else changed.
    let resolved = resolve_all(&snapshot(&directory), None, 0, now);
    assert!(!resolved[&decorated].crown);
    assert!(resolved[&decorated].style.is_some());
}

#[test]
fn styled_name_spans_keeps_base_bg_and_modifiers() {
    use ratatui::style::Modifier;
    let base = Style::default()
        .bg(Color::Rgb(40, 40, 40))
        .add_modifier(Modifier::BOLD);
    let spans = styled_name_spans("mat", NameStyle::Solid(Color::Rgb(255, 200, 80)), base);
    assert_eq!(spans.len(), 3);
    for span in &spans {
        assert_eq!(span.style.bg, Some(Color::Rgb(40, 40, 40)));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(span.style.fg, Some(Color::Rgb(255, 200, 80)));
    }
}

/// A milestone is permanent, so it survives a resolve that drops every
/// expired rental, and an owner who has bought nothing else still gets an
/// entry: without one the dearest purchase in the shop would render nothing.
#[test]
fn a_burn_milestone_outlives_the_rentals_beside_it() {
    let directory = new_directory();
    let lapsed = Uuid::now_v7();
    let bare = Uuid::now_v7();
    let now = Utc::now();

    set_user(
        &directory,
        lapsed,
        NameFlair {
            effect: Some(FlairEffect {
                effect: UsernameEffect::Shimmer,
                ends_at: now - Duration::seconds(1),
            }),
            title: None,
            milestone: Some("\u{1F30B}".to_string()),
        },
    );
    set_user(
        &directory,
        bare,
        NameFlair {
            effect: None,
            title: None,
            milestone: Some("\u{1F9E8}".to_string()),
        },
    );

    let resolved = resolve_all(&snapshot(&directory), None, 0, now);
    assert_eq!(resolved[&lapsed].style, None);
    assert_eq!(resolved[&lapsed].milestone.as_deref(), Some("\u{1F30B}"));
    assert_eq!(resolved[&bare].milestone.as_deref(), Some("\u{1F9E8}"));
}
