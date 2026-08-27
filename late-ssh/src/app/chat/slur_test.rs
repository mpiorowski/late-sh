use super::*;

use late_core::models::drink_round::{ROUND_PHRASES, contains_round_request};
use late_core::models::drinks::DRUNK_MAX_LEVEL;

/// A few dozen ordinary chat words, long enough that per-word odds average out.
const CORPUS: &str = "the deploy went through but the migration is still pending on staging \
    and nobody wants to touch it before the release window closes tomorrow morning \
    someone should probably check whether the cache warmed up correctly again";

fn changed_word_count(original: &str, slurred: &str) -> usize {
    original
        .split_whitespace()
        .zip(slurred.split_whitespace())
        .filter(|(before, after)| before != after)
        .count()
}

#[test]
fn a_sober_patron_types_exactly_what_they_meant() {
    for seed in 1..200 {
        assert_eq!(slur(CORPUS, 0, seed), CORPUS);
    }
}

#[test]
fn every_word_keeps_its_shape_even_wasted() {
    // The readability contract: a reader recognises a word by its first and
    // last character, so those never move. The `ing` contraction is the one
    // sanctioned exception.
    for seed in 1..200 {
        let slurred = slur(CORPUS, DRUNK_MAX_LEVEL, seed);
        // The hiccup is an inserted word, not a mangled one, so drop it before
        // lining the two word lists up.
        let spoken = slurred.split_whitespace().filter(|word| *word != "*hic*");
        for (before, after) in CORPUS.split_whitespace().zip(spoken) {
            assert_eq!(
                before.chars().next(),
                after.chars().next(),
                "seed {seed}: {before} -> {after} moved its first letter"
            );
            let kept_last = before.chars().next_back() == after.chars().next_back();
            let contracted = before.ends_with("ing") && after.ends_with("in'");
            assert!(
                kept_last || contracted,
                "seed {seed}: {before} -> {after} moved its last letter"
            );
        }
    }
}

#[test]
fn each_drink_reads_harder_than_the_last() {
    // The product curve: 1 and 2 read clean, 3 takes a beat, 4 takes effort.
    let sampled = CORPUS.split_whitespace().count() * 199;
    let hit_percent = |level: u8| {
        let changed: usize = (1..200)
            .map(|seed| changed_word_count(CORPUS, &slur(CORPUS, level, seed)))
            .sum();
        changed * 100 / sampled
    };

    let [tipsy, buzzed, sloshed, wasted] = [1, 2, 3, DRUNK_MAX_LEVEL].map(hit_percent);

    assert!(tipsy < 8, "tipsy should barely show, hit {tipsy}%");
    assert!(
        (12..28).contains(&buzzed),
        "buzzed should be plainly past tipsy but still read clean, hit {buzzed}%"
    );
    assert!(
        (28..45).contains(&sloshed),
        "sloshed should take a beat, hit {sloshed}%"
    );
    assert!(wasted >= 50, "wasted should take effort, hit {wasted}%");
    assert!(
        tipsy < buzzed && buzzed < sloshed && sloshed < wasted,
        "every drink should read harder: {tipsy} {buzzed} {sloshed} {wasted}"
    );
}

#[test]
fn the_things_that_carry_meaning_are_never_garbled() {
    // Each of these breaks a real feature if a typo lands in it: mentions
    // drive notifications and the mention highlight, slugs drive room jump,
    // links drive news cards, code is code, and the marker drives card
    // parsing. The quote line is someone else's words.
    let body = "> @alice: original message\n\
        @alice check https://late.sh/docs and #lounge for the `cargo nextest run` output ---NEWS---";

    for seed in 1..200 {
        let slurred = slur(body, DRUNK_MAX_LEVEL, seed);
        assert!(
            slurred.starts_with("> @alice: original message\n"),
            "{slurred}"
        );
        // The handle survives even though the ordinary words around it do not.
        assert!(
            slurred
                .lines()
                .nth(1)
                .is_some_and(|line| line.split_whitespace().next() == Some("@alice")),
            "{slurred}"
        );
        assert!(slurred.contains("https://late.sh/docs"), "{slurred}");
        assert!(slurred.contains("#lounge"), "{slurred}");
        assert!(slurred.contains("`cargo nextest run`"), "{slurred}");
        assert!(slurred.contains("---NEWS---"), "{slurred}");
    }
}

/// The one sentence that spends chips. A wasted patron is exactly who buys the
/// house a round, and @bartender matches the phrase literally, so a scramble or
/// a hiccup landing in it would break the feature for precisely the people it
/// is for. Every level, every seed, verbatim.
#[test]
fn an_order_for_a_round_survives_any_amount_of_drink() {
    for phrase in ROUND_PHRASES {
        let body = format!("@bartender it has been a long week, {phrase} tonight");
        for level in 1..=DRUNK_MAX_LEVEL {
            for seed in 1..200 {
                let slurred = slur(&body, level, seed);
                assert!(
                    slurred.contains(phrase),
                    "level {level} seed {seed} lost the order: {slurred}"
                );
                assert!(
                    contains_round_request(&slurred),
                    "level {level} seed {seed} no longer reads as a round: {slurred}"
                );
            }
        }
    }
}

/// The words around the order still take their beating: protecting the phrase
/// is a narrow carve-out, not a way to type soberly by mentioning drinks.
#[test]
fn protecting_the_order_does_not_sober_up_the_rest_of_the_line() {
    let body = format!("{CORPUS} round for everyone");
    let untouched = (1..200)
        .filter(|seed| slur(&body, DRUNK_MAX_LEVEL, *seed) == body)
        .count();
    assert_eq!(untouched, 0, "a wasted patron never types the rest cleanly");
}

#[test]
fn non_ascii_text_is_left_alone() {
    let body = "刚刚部署完成了 ok 🍻 gänsefüßchen";
    for seed in 1..200 {
        let slurred = slur(body, DRUNK_MAX_LEVEL, seed);
        assert!(slurred.contains("刚刚部署完成了"), "{slurred}");
        assert!(slurred.contains('🍻'), "{slurred}");
        assert!(slurred.contains("gänsefüßchen"), "{slurred}");
    }
}
