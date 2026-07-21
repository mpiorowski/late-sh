use crate::app::door::greendragon::tavern::*;
use rand::{SeedableRng, rngs::StdRng};

#[test]
fn old_man_keeps_a_winning_first_roll() {
    // Against a player 1, any first roll beats or ties-at-6: he never
    // rolls again, so across seeds his die is always >= the player's.
    let mut rng = StdRng::seed_from_u64(1);
    for _ in 0..200 {
        assert!(old_man_roll(1, &mut rng) >= 1);
    }
    // Against a player 6 his only keeps are a 6 (roll 1 or 2) or a forced
    // third; verify the function terminates and stays in range.
    for _ in 0..200 {
        let r = old_man_roll(6, &mut rng);
        assert!((1..=6).contains(&r));
    }
}

#[test]
fn dice_game_forces_the_third_roll() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut g = DiceGame::open(50, &mut rng);
    assert!(g.can_reroll());
    g.reroll(&mut rng);
    assert!(g.can_reroll());
    g.reroll(&mut rng);
    assert!(!g.can_reroll());
}

#[test]
fn stones_conserves_the_bag_and_ends() {
    for seed in 0..200 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut g = StonesGame::open(seed % 2 == 0, 10);
        let mut draws = 0;
        while !g.finished() {
            g.draw(&mut rng);
            draws += 1;
            assert!(draws <= 8, "bag holds at most 8 pairs");
        }
        // Every drawn pair landed on exactly one pile.
        assert_eq!(g.player_pile + g.oldman_pile, draws * 2);
        assert_eq!(16 - (g.red + g.blue), draws * 2);
        // The end came from the bag or a pile passing 8.
        assert!(g.red + g.blue < 2 || g.player_pile > 8 || g.oldman_pile > 8);
    }
}

#[test]
fn stones_payout_is_even_money() {
    let mut g = StonesGame::open(true, 25);
    g.player_pile = 10;
    g.oldman_pile = 4;
    assert_eq!(g.payout(), 25);
    g.player_pile = 4;
    g.oldman_pile = 10;
    assert_eq!(g.payout(), -25);
    g.player_pile = 8;
    g.oldman_pile = 8;
    assert_eq!(g.payout(), 0);
}

#[test]
fn fivesix_counts_sixes() {
    let mut rng = StdRng::seed_from_u64(3);
    for _ in 0..100 {
        let (dice, sixes) = fivesix_roll(&mut rng);
        assert_eq!(dice.len(), 5);
        assert_eq!(sixes as usize, dice.iter().filter(|&&d| d == 6).count());
    }
}
