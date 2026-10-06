use super::*;
use crate::app::arcade::solitaire::state::Suit;

const SUITS: [Suit; 4] = [Suit::Hearts, Suit::Diamonds, Suit::Clubs, Suit::Spades];

fn won_foundations() -> [Vec<Card>; 4] {
    std::array::from_fn(|pile| {
        (1..=13)
            .map(|rank| Card {
                suit: SUITS[pile],
                rank,
            })
            .collect()
    })
}

fn animation(width: u16, height: u16) -> WinAnimation {
    let mut anim = WinAnimation::new(&won_foundations(), 0xFACE_B00C);
    anim.set_viewport(Viewport {
        width,
        height,
        origin_x: 0,
    });
    anim
}

fn painted(anim: &WinAnimation) -> usize {
    let view = anim.viewport();
    (0..view.height)
        .flat_map(|y| (0..view.width).map(move |x| (x, y)))
        .filter(|(x, y)| anim.ink(*x, *y).is_some())
        .count()
}

/// Cards leave the way a player would take them off the piles: the kings
/// first, cycling the four foundations.
#[test]
fn launches_kings_first() {
    let mut anim = animation(80, 24);
    anim.advance();
    let first = anim.flyers[0].card;
    assert_eq!(first.rank, 13);
    assert_eq!(first.suit, Suit::Spades);
}

#[test]
fn queues_the_whole_deck() {
    let anim = animation(80, 24);
    assert_eq!(anim.queue.len(), 52);
}

#[test]
fn a_flying_card_paints_the_board() {
    let mut anim = animation(80, 24);
    assert_eq!(painted(&anim), 0);
    for _ in 0..8 {
        anim.advance();
    }
    assert!(painted(&anim) > 0, "the cascade painted nothing");
}

/// The trail is the whole point: a cell a card has passed over stays painted
/// for the rest of the run.
#[test]
fn the_trail_only_grows() {
    let mut anim = animation(80, 24);
    let mut last = 0;
    for _ in 0..60 {
        anim.advance();
        let now = painted(&anim);
        assert!(now >= last, "the trail lost cells: {last} -> {now}");
        last = now;
    }
}

/// Every card has to reach an edge and leave, or the cascade never ends and
/// the `YOU WON!` card never appears.
#[test]
fn runs_itself_out() {
    let mut anim = animation(80, 24);
    let mut frames = 0;
    while !anim.is_finished() {
        anim.advance();
        frames += 1;
        assert!(frames < 2_000, "the cascade never finished");
    }
    assert!(!anim.advance(), "a finished cascade still asks for frames");
}

#[test]
fn skipping_finishes_it() {
    let mut anim = animation(80, 24);
    anim.advance();
    anim.skip_to_end();
    assert!(anim.is_finished());
    assert!(painted(&anim) > 0, "the skipped cascade left no cards");
}

/// A resize cannot be remapped honestly, so the canvas starts over rather
/// than showing card art stretched across the wrong cells.
#[test]
fn a_resize_clears_the_canvas() {
    let mut anim = animation(80, 24);
    for _ in 0..20 {
        anim.advance();
    }
    assert!(painted(&anim) > 0);
    anim.set_viewport(Viewport {
        width: 100,
        height: 30,
        origin_x: 0,
    });
    assert_eq!(painted(&anim), 0);
    assert_eq!(anim.viewport().width, 100);
}

/// A board with no room to bounce gets no cascade, and says so at once so the
/// win card is not held back forever.
#[test]
fn a_short_board_skips_the_cascade() {
    let mut anim = animation(80, 6);
    anim.advance();
    assert!(anim.is_finished());
}

/// Cards are launched from the foundation slots the player is looking at,
/// shifted by wherever the board itself was drawn.
#[test]
fn launches_from_the_foundation_slots() {
    let mut anim = WinAnimation::new(&won_foundations(), 7);
    anim.set_viewport(Viewport {
        width: 120,
        height: 30,
        origin_x: 21,
    });
    anim.advance();
    let x = anim.flyers[0].x - anim.flyers[0].vx;
    assert_eq!(x as i32, 21 + FOUNDATION_X + FOUNDATION_STEP * 3);
}

/// The deal's seed drives the throws, so replaying the same board replays the
/// same cascade.
#[test]
fn the_same_seed_throws_the_same_way() {
    let run = |seed: u64| {
        let mut anim = WinAnimation::new(&won_foundations(), seed);
        anim.set_viewport(Viewport {
            width: 80,
            height: 24,
            origin_x: 0,
        });
        for _ in 0..40 {
            anim.advance();
        }
        anim.flyers.iter().map(|f| f.vx).collect::<Vec<_>>()
    };
    assert_eq!(run(99), run(99));
    assert_ne!(run(99), run(100));
}
