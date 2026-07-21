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
        Some(NameFlair {
            effect: UsernameEffect::Shimmer,
            ends_at: now + Duration::hours(24),
        }),
    );
    set_user(
        &directory,
        other,
        Some(NameFlair {
            effect: UsernameEffect::Glow(GlowColor::Ember),
            ends_at: now - Duration::seconds(1),
        }),
    );

    let resolved = resolve_all(&snapshot(&directory), 0, now);
    assert!(resolved.contains_key(&user));
    assert!(
        !resolved.contains_key(&other),
        "expired flair must be skipped"
    );

    set_user(&directory, user, None);
    assert!(snapshot(&directory).is_empty() || !snapshot(&directory).contains_key(&user));

    set_all(
        &directory,
        vec![(
            other,
            NameFlair {
                effect: UsernameEffect::Gradient(GradientPair::Candy),
                ends_at: now + Duration::hours(1),
            },
        )],
    );
    let entries = snapshot(&directory);
    assert_eq!(entries.len(), 1);
    assert!(entries.contains_key(&other));
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
