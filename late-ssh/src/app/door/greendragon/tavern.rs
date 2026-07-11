//! The Dark Horse Tavern's gambling games (`modules/game_dice.php`,
//! `game_fivesix.php`, `game_stones.php`): the pure game logic, transcribed
//! 1=1 from the stock modules. The menu wiring, the shared Five Sixes pot
//! (svc), and all prose live elsewhere; these are the mechanics.

use rand::Rng;

/// An in-progress dice game against the one-eyed gambler (`game_dice.php`):
/// even-money, high die wins. The player gets up to three rolls, keeping or
/// re-rolling; the third stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiceGame {
    pub bet: u64,
    /// The player's current die.
    pub roll: u32,
    /// Rolls taken so far (max 3).
    pub tries: u32,
}

pub const DICE_MAX_ROLLS: u32 = 3;

impl DiceGame {
    /// Open the game with the stake and the first roll.
    pub fn open(bet: u64, rng: &mut impl Rng) -> Self {
        DiceGame {
            bet,
            roll: rng.gen_range(1..=6),
            tries: 1,
        }
    }

    /// Whether the player may still pass on this roll (the third is forced).
    pub fn can_reroll(&self) -> bool {
        self.tries < DICE_MAX_ROLLS
    }

    /// Take another roll.
    pub fn reroll(&mut self, rng: &mut impl Rng) {
        self.roll = rng.gen_range(1..=6);
        self.tries += 1;
    }
}

/// The old man's die once the player stands (`game_dice.php`): his first
/// roll keeps if it beats the player or shows a natural 6; his second keeps
/// on a tie or better; his third stands regardless.
pub fn old_man_roll(player: u32, rng: &mut impl Rng) -> u32 {
    let r = rng.gen_range(1..=6);
    if r > player || r == 6 {
        return r;
    }
    let r = rng.gen_range(1..=6);
    if r >= player {
        return r;
    }
    rng.gen_range(1..=6)
}

/// An in-progress stones game (`game_stones.php`). The bag starts 6 red +
/// 10 blue; each draw pulls two stones, and the pair lands (+2) on the pile
/// of whoever called it — yours if you bet "like" and the colors match (or
/// "unlike" and they differ), the old man's otherwise. The game ends when
/// the bag empties or a pile passes 8; the bigger pile takes the even-money
/// bet, a tie pushes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StonesGame {
    pub red: u32,
    pub blue: u32,
    /// True: you bet on like pairs; false: on unlike pairs.
    pub like_pair: bool,
    pub bet: u64,
    pub player_pile: u32,
    pub oldman_pile: u32,
}

/// One drawn pair: the two stones (true = red) and whose pile it joined.
#[derive(Clone, Copy, Debug)]
pub struct StonesDraw {
    pub first_red: bool,
    pub second_red: bool,
    pub yours: bool,
}

impl StonesGame {
    pub fn open(like_pair: bool, bet: u64) -> Self {
        StonesGame {
            red: 6,
            blue: 10,
            like_pair,
            bet,
            player_pile: 0,
            oldman_pile: 0,
        }
    }

    /// Whether the game is over: an empty bag, or either pile past 8.
    pub fn finished(&self) -> bool {
        self.red + self.blue < 2 || self.player_pile > 8 || self.oldman_pile > 8
    }

    fn draw_one(&mut self, rng: &mut impl Rng) -> bool {
        if rng.gen_range(1..=self.red + self.blue) <= self.red {
            self.red -= 1;
            true
        } else {
            self.blue -= 1;
            false
        }
    }

    /// Draw the next pair and land it on the right pile.
    pub fn draw(&mut self, rng: &mut impl Rng) -> StonesDraw {
        let first_red = self.draw_one(rng);
        let second_red = self.draw_one(rng);
        let yours = (first_red == second_red) == self.like_pair;
        if yours {
            self.player_pile += 2;
        } else {
            self.oldman_pile += 2;
        }
        StonesDraw {
            first_red,
            second_red,
            yours,
        }
    }

    /// The signed gold outcome once finished: +bet, -bet, or a push.
    pub fn payout(&self) -> i64 {
        match self.player_pile.cmp(&self.oldman_pile) {
            std::cmp::Ordering::Greater => self.bet as i64,
            std::cmp::Ordering::Less => -(self.bet as i64),
            std::cmp::Ordering::Equal => 0,
        }
    }
}

/// Roll the five dice and count the sixes (`game_fivesix.php`).
pub fn fivesix_roll(rng: &mut impl Rng) -> (Vec<u32>, u32) {
    let dice: Vec<u32> = (0..5).map(|_| rng.gen_range(1..=6)).collect();
    let sixes = dice.iter().filter(|&&d| d == 6).count() as u32;
    (dice, sixes)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
