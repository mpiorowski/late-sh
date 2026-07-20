use uuid::Uuid;
use crate::app::door::greendragon::commentary::*;

fn line(user: Uuid, today: bool) -> CommentLine {
    CommentLine {
        user_id: Some(user),
        name: "Tester".into(),
        body: "hello".into(),
        today,
        day: 0,
    }
}

#[test]
fn allowance_is_half_the_window_rounded_up() {
    // round(25/2)=13, round(20/2)=10, round(10/2)=5, round(30/2)=15.
    assert_eq!(posts_allowed(CommentRoom::Village.display_limit()), 13);
    assert_eq!(posts_allowed(CommentRoom::Inn.display_limit()), 10);
    assert_eq!(posts_allowed(CommentRoom::DarkHorse.display_limit()), 5);
    assert_eq!(posts_allowed(CommentRoom::Gardens.display_limit()), 15);
}

#[test]
fn posts_left_counts_only_my_posts_from_today() {
    let me = Uuid::from_u128(1);
    let other = Uuid::from_u128(2);
    let lines = vec![
        line(me, true),
        line(me, true),
        line(me, false), // yesterday's post scrolled back in: free
        line(other, true),
    ];
    assert_eq!(posts_left(&lines, me, CommentRoom::DarkHorse), 3);
    assert_eq!(posts_left(&lines, other, CommentRoom::DarkHorse), 4);
}

#[test]
fn says_rooms_store_the_body_untouched() {
    assert_eq!(
        prepare_post("  hello there  ", "says").unwrap(),
        "hello there"
    );
}

#[test]
fn verb_rooms_bake_the_venue_verb() {
    assert_eq!(
        prepare_post("who turned out the light", "despairs").unwrap(),
        ":despairs, \"who turned out the light\""
    );
    // An explicit emote keeps its own action, any venue.
    assert_eq!(
        prepare_post(":rattles his chains", "despairs").unwrap(),
        ":rattles his chains"
    );
}

#[test]
fn empty_and_bare_marker_posts_are_rejected() {
    for raw in ["", "   ", ":", "::", "/me", " /me "] {
        assert!(prepare_post(raw, "says").is_none(), "{raw:?}");
    }
}

#[test]
fn long_runs_are_broken_like_upstream() {
    let raw = "a".repeat(100);
    let broken = prepare_post(&raw, "says").unwrap();
    // A space after char 45, then after 46 more (the breaker starts the
    // next window's count at zero, like upstream's consumed `$2`).
    assert_eq!(
        broken,
        format!("{} {} {}", "a".repeat(45), "a".repeat(46), "a".repeat(9))
    );
}

#[test]
fn composition_quotes_speech_and_unfolds_emotes() {
    assert_eq!(compose_line("Ada", "hello"), "Ada says, \"hello\"");
    assert_eq!(compose_line("Ada", ":waves"), "Ada waves");
    assert_eq!(compose_line("Ada", "/me waves"), "Ada waves");
    assert_eq!(
        compose_line("Ada", ":despairs, \"why\""),
        "Ada despairs, \"why\""
    );
    // System lines render bare.
    assert_eq!(compose_line("", "The ground shakes."), "The ground shakes.");
}

#[test]
fn verb_rooms_shrink_the_typing_budget() {
    assert_eq!(max_post_len("says"), 200);
    assert_eq!(max_post_len("despairs"), 181);
}

#[test]
fn clan_halls_are_exempt_from_the_allowance() {
    // talkform skips the posts-today count for clan-* sections entirely;
    // the shared waiting area is NOT exempt (window 25, allowance 13).
    let me = Uuid::from_u128(1);
    let hall = CommentRoom::ClanHall(Uuid::from_u128(9));
    let flood: Vec<CommentLine> = (0..25).map(|_| line(me, true)).collect();
    assert_eq!(posts_left(&flood, me, hall), usize::MAX);
    assert_eq!(posts_left(&flood, me, CommentRoom::Waiting), 0);
    assert_eq!(posts_left(&[], me, CommentRoom::Waiting), posts_allowed(25));
}

#[test]
fn sober_lines_pass_the_drinks_hook_untouched() {
    let mut rng = rand::thread_rng();
    let (body, verb) = apply_drunkenness("a round for the house", "says", 0, &mut rng);
    assert_eq!(body, "a round for the house");
    assert_eq!(verb, "says");
}

#[test]
fn drunk_lines_slur_and_the_verb_turns_drunkenly_past_fifty() {
    let mut rng = rand::thread_rng();
    let (body, verb) = apply_drunkenness("a round for the house", "says", 60, &mut rng);
    // Past 50 the verb gains "drunkenly" (which then bakes, since it no
    // longer equals "says"), and the line itself slurs.
    assert_eq!(verb, "drunkenly says");
    assert_ne!(body, "a round for the house");
    // Emotes keep their line even blind drunk (upstream skips the
    // markers); only the verb carries the state.
    let (body, verb) = apply_drunkenness(":falls off the stool", "says", 100, &mut rng);
    assert_eq!(body, ":falls off the stool");
    assert_eq!(verb, "drunkenly says");
}

#[test]
fn drunkenize_grows_the_line_and_collapses_adjacent_hics() {
    let mut rng = rand::thread_rng();
    for _ in 0..50 {
        let out = drunkenize("well met stranger", 100, &mut rng);
        // Every replacement lengthens (a doubled letter or a *hic*)...
        assert!(out.len() > "well met stranger".len());
        // ...and back-to-back hics collapse (upstream's cleanup loop).
        assert!(!out.contains("*hic**hic*"));
    }
}

#[test]
fn clan_rooms_have_their_own_sections() {
    let id = Uuid::from_u128(9);
    assert_eq!(CommentRoom::ClanHall(id).section(), format!("clan-{id}"));
    assert_eq!(CommentRoom::Waiting.section(), "waiting");
    // The hall's verb is only the fallback; the custom say line
    // overrides it at the call sites.
    assert_eq!(CommentRoom::ClanHall(id).verb(), "says");
    assert_eq!(CommentRoom::ClanHall(id).display_limit(), 25);
    assert_eq!(CommentRoom::Waiting.display_limit(), 25);
}
