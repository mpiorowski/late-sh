//! The classic win cascade: once the last card lands, the foundations empty
//! themselves down the board, each card arcing off the floor and leaving its
//! own image behind until nothing is left but a heap of cards.
//!
//! Pure: physics in board cells, art stamped into a private character canvas.
//! `ui.rs` blits the canvas over the board and `state.rs` drives one step per
//! world tick; neither the terminal nor the theme is visible from here.

use super::state::{Card, to_playing_card};
use crate::app::games::cards::{AsciiCardTheme, OUTLINE_CARD_WIDTH};

/// The board draws `AsciiCardTheme::Outline`, so a flying card is 9x5 cells.
const CARD_W: i32 = OUTLINE_CARD_WIDTH as i32;
const CARD_H: i32 = 5;
const CARD_THEME: AsciiCardTheme = AsciiCardTheme::Outline;

/// Where the foundation piles sit on the board, in cells from the board's own
/// left edge. Stock and waste are always empty at a win, so the row is a fixed
/// `9 + 1` per slot: stock, waste, then the four foundations. The header line
/// puts the card art one row down.
const FOUNDATION_X: i32 = 20;
const FOUNDATION_STEP: i32 = 10;
const FOUNDATION_Y: i32 = 1;

/// Ticks between launches, at the 15fps hot cadence. Fifty-two cards at one
/// every three ticks runs the cascade out in about ten seconds.
const LAUNCH_INTERVAL: u32 = 3;
/// A ceiling on cards in the air at once. Without it a wide board (where a
/// card takes longer to leave) ends up with the whole deck in flight. Cards
/// drift slowly, so the cap sits high enough not to throttle the launches.
const MAX_IN_FLIGHT: usize = 20;

const GRAVITY: f32 = 0.28;
const BOUNCE: f32 = 0.80;
/// Initial hop, upward: a small lift off the pile, then a long fall.
const LAUNCH_VY: f32 = -0.3;
const LAUNCH_VY_SPREAD: f32 = 0.25;
const MIN_VX: f32 = 0.85;
const VX_SPREAD: f32 = 1.35;
/// A bounce this weak no longer lifts the card clear of the floor: let it
/// slide out sideways instead of juddering in place, restamping one spot.
const MIN_BOUNCE_VY: f32 = 0.25;

/// A board too short to bounce in gets no cascade at all.
const MIN_VIEW_HEIGHT: u16 = 9;

/// The cells the cascade is allowed to cover: the game content area, plus the
/// board's own left edge inside it, so launches line up with the foundations
/// the player is looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub width: u16,
    pub height: u16,
    pub origin_x: u16,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 80,
            height: 24,
            origin_x: 0,
        }
    }
}

/// One painted cell of card art. The suit's color is carried as red-or-black
/// so the renderer can resolve it against the live theme every frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ink {
    pub ch: char,
    pub red: bool,
}

#[derive(Clone, Copy)]
struct Flyer {
    card: Card,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    /// The cell the card was last stamped at, so a frame that moves it less
    /// than a whole cell does not repaint the same art.
    stamped: Option<(i32, i32)>,
}

pub struct WinAnimation {
    /// Cards still to launch, popped from the back: kings first, cycling the
    /// four piles, the way they come off the top of the foundations.
    queue: Vec<(Card, usize)>,
    flyers: Vec<Flyer>,
    canvas: Vec<Option<Ink>>,
    view: Viewport,
    /// Cards already thrown from each foundation, so the board can draw the
    /// piles emptying instead of leaving a full pile under the cards that
    /// supposedly left it.
    launched: [usize; 4],
    rng: u64,
    since_launch: u32,
}

impl WinAnimation {
    pub fn new(foundations: &[Vec<Card>; 4], seed: u64) -> Self {
        let mut queue = Vec::with_capacity(52);
        for rank in 1..=13usize {
            for (pile, cards) in foundations.iter().enumerate() {
                if let Some(card) = cards.get(rank - 1) {
                    queue.push((*card, pile));
                }
            }
        }

        let view = Viewport::default();
        Self {
            queue,
            flyers: Vec::new(),
            canvas: vec![None; view.width as usize * view.height as usize],
            view,
            launched: [0; 4],
            // The deal's own seed, so a given board always throws its cards
            // the same way. The odd bit only keeps the xorshift off zero,
            // which is its one fixed point.
            rng: (seed ^ 0x9E37_79B9_7F4A_7C15) | 1,
            since_launch: LAUNCH_INTERVAL,
        }
    }

    pub fn viewport(&self) -> Viewport {
        self.view
    }

    /// How many cards have left the given foundation pile.
    pub fn launched_from(&self, pile: usize) -> usize {
        self.launched.get(pile).copied().unwrap_or(0)
    }

    /// Point the cascade at the area the board is actually drawn in. A resize
    /// mid-flight cannot be remapped honestly (the trail is art, not cards),
    /// so it starts the canvas over and lets the cards still in the air keep
    /// falling.
    pub fn set_viewport(&mut self, view: Viewport) {
        if view == self.view {
            return;
        }
        self.view = view;
        self.canvas = vec![None; view.width as usize * view.height as usize];
        if view.height < MIN_VIEW_HEIGHT {
            self.queue.clear();
            self.flyers.clear();
        }
    }

    pub fn is_finished(&self) -> bool {
        self.queue.is_empty() && self.flyers.is_empty()
    }

    /// Advance one frame. Returns whether anything moved, so a finished
    /// cascade stops asking the render loop for frames.
    pub fn advance(&mut self) -> bool {
        if self.is_finished() {
            return false;
        }
        if self.view.height < MIN_VIEW_HEIGHT {
            self.queue.clear();
            self.flyers.clear();
            return true;
        }

        self.since_launch = self.since_launch.saturating_add(1);
        if self.since_launch >= LAUNCH_INTERVAL && self.flyers.len() < MAX_IN_FLIGHT {
            self.since_launch = 0;
            self.launch_next();
        }

        let floor = (self.view.height as i32 - CARD_H) as f32;
        let right_edge = self.view.width as f32;
        let mut stamps = Vec::new();
        self.flyers.retain_mut(|flyer| {
            // Stamp where the card is before it moves, so the first image
            // sits right on its pile.
            let cell = (flyer.x.round() as i32, flyer.y.round() as i32);
            if flyer.stamped != Some(cell) {
                flyer.stamped = Some(cell);
                stamps.push((flyer.card, cell));
            }

            flyer.vy += GRAVITY;
            flyer.x += flyer.vx;
            flyer.y += flyer.vy;

            if flyer.y >= floor {
                flyer.y = floor;
                flyer.vy = -flyer.vy * BOUNCE;
                if flyer.vy > -MIN_BOUNCE_VY {
                    flyer.vy = 0.0;
                }
            }

            flyer.x + CARD_W as f32 > 0.0 && flyer.x < right_edge
        });

        for (card, (x, y)) in stamps {
            self.stamp(card, x, y);
        }
        true
    }

    /// Run the cascade out at once: the player pressed a key, which in this
    /// game has always meant "enough". The cap is only a guard against a
    /// pathological viewport; a normal run lands in a few hundred frames.
    pub fn skip_to_end(&mut self) {
        for _ in 0..5_000 {
            if !self.advance() {
                break;
            }
        }
        self.queue.clear();
        self.flyers.clear();
    }

    pub fn ink(&self, x: u16, y: u16) -> Option<Ink> {
        if x >= self.view.width || y >= self.view.height {
            return None;
        }
        self.canvas[y as usize * self.view.width as usize + x as usize]
    }

    fn launch_next(&mut self) {
        let Some((card, pile)) = self.queue.pop() else {
            return;
        };
        self.launched[pile] += 1;
        let to_the_right = self.next_rand() & 1 == 0;
        let speed = MIN_VX + self.rand_unit() * VX_SPREAD;
        let vy = LAUNCH_VY - self.rand_unit() * LAUNCH_VY_SPREAD;
        self.flyers.push(Flyer {
            card,
            x: (self.view.origin_x as i32 + FOUNDATION_X + FOUNDATION_STEP * pile as i32) as f32,
            y: FOUNDATION_Y as f32,
            vx: if to_the_right { speed } else { -speed },
            vy,
            stamped: None,
        });
    }

    fn stamp(&mut self, card: Card, left: i32, top: i32) {
        let red = card.suit.is_red();
        let width = self.view.width as i32;
        let height = self.view.height as i32;
        for (row, line) in CARD_THEME
            .render_face_lines(to_playing_card(card))
            .into_iter()
            .enumerate()
        {
            let y = top + row as i32;
            if y < 0 || y >= height {
                continue;
            }
            for (col, ch) in line.chars().enumerate() {
                let x = left + col as i32;
                if x < 0 || x >= width {
                    continue;
                }
                self.canvas[y as usize * self.view.width as usize + x as usize] =
                    Some(Ink { ch, red });
            }
        }
    }

    fn next_rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 32) as u32
    }

    fn rand_unit(&mut self) -> f32 {
        self.next_rand() as f32 / u32::MAX as f32
    }
}

#[cfg(test)]
#[path = "win_anim_test.rs"]
mod win_anim_test;
