use std::{array, collections::HashMap};

use chrono::NaiveDate;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::svc::SolitaireService;
use late_core::models::solitaire::{Game, GameParams};

pub const DIFFICULTIES: [&str; 2] = ["draw-1", "draw-3"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Suit {
    Hearts,
    Diamonds,
    Clubs,
    Spades,
}

impl Suit {
    pub fn short(self) -> &'static str {
        match self {
            Suit::Hearts => "H",
            Suit::Diamonds => "D",
            Suit::Clubs => "C",
            Suit::Spades => "S",
        }
    }

    fn is_red(self) -> bool {
        matches!(self, Suit::Hearts | Suit::Diamonds)
    }

    fn foundation_index(self) -> usize {
        match self {
            Suit::Hearts => 0,
            Suit::Diamonds => 1,
            Suit::Clubs => 2,
            Suit::Spades => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub suit: Suit,
    pub rank: u8,
}

impl Card {
    pub fn label(self) -> String {
        let rank = match self.rank {
            1 => "A".to_string(),
            11 => "J".to_string(),
            12 => "Q".to_string(),
            13 => "K".to_string(),
            n => n.to_string(),
        };
        format!("{rank:<2}{}", self.suit.short())
    }

    fn can_stack_on(self, target: Card) -> bool {
        self.rank + 1 == target.rank && self.suit.is_red() != target.suit.is_red()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableauCard {
    pub card: Card,
    pub face_up: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Daily,
    Personal,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Daily => "daily",
            Mode::Personal => "personal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Stock,
    Waste,
    Foundation(usize),
    Tableau(usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Waste,
    Foundation(usize),
    Tableau { col: usize, row: usize },
}

#[derive(Clone)]
struct Snapshot {
    seed: u64,
    stock: Vec<Card>,
    waste: Vec<Card>,
    foundations: [Vec<Card>; 4],
    tableau: [Vec<TableauCard>; 7],
    is_game_over: bool,
}

pub struct State {
    pub user_id: Uuid,
    pub mode: Mode,
    pub selected_difficulty: usize,
    pub seed: u64,
    pub stock: Vec<Card>,
    pub waste: Vec<Card>,
    pub foundations: [Vec<Card>; 4],
    pub tableau: [Vec<TableauCard>; 7],
    pub cursor: Focus,
    pub selection: Option<Selection>,
    pub is_game_over: bool,
    pub scroll_offset: u16,
    undo_stack: Vec<Snapshot>,
    daily_snapshots: HashMap<String, Snapshot>,
    personal_snapshots: HashMap<String, Snapshot>,
    pub svc: SolitaireService,
}

impl State {
    pub fn new(user_id: Uuid, svc: SolitaireService, saved_games: Vec<Game>) -> Self {
        let today = svc.today();
        let mut daily_snapshots = HashMap::new();
        let mut personal_snapshots = HashMap::new();

        for &difficulty_key in &DIFFICULTIES {
            let daily_snapshot = saved_games
                .iter()
                .find(|game| {
                    game.mode == "daily"
                        && game.difficulty_key == difficulty_key
                        && is_current_daily_game(game.puzzle_date, today)
                })
                .map(snapshot_from_game)
                .unwrap_or_else(|| snapshot_from_seed(svc.get_daily_seed(difficulty_key)));
            daily_snapshots.insert(difficulty_key.to_string(), daily_snapshot);

            if let Some(snapshot) = saved_games
                .iter()
                .find(|game| game.mode == "personal" && game.difficulty_key == difficulty_key)
                .map(snapshot_from_game)
            {
                personal_snapshots.insert(difficulty_key.to_string(), snapshot);
            }
        }

        let mut state = Self {
            user_id,
            mode: Mode::Daily,
            selected_difficulty: 0,
            seed: 0,
            stock: Vec::new(),
            waste: Vec::new(),
            foundations: array::from_fn(|_| Vec::new()),
            tableau: array::from_fn(|_| Vec::new()),
            cursor: Focus::Stock,
            selection: None,
            is_game_over: false,
            scroll_offset: 0,
            undo_stack: Vec::new(),
            daily_snapshots,
            personal_snapshots,
            svc,
        };
        state.load_mode_snapshot_for_selected_difficulty();
        state
    }

    pub fn difficulty_key(&self) -> &'static str {
        DIFFICULTIES[self.selected_difficulty]
    }

    pub fn draw_count(&self) -> usize {
        match self.difficulty_key() {
            "draw-3" => 3,
            _ => 1,
        }
    }

    pub fn show_daily(&mut self) {
        self.store_active_snapshot();
        self.mode = Mode::Daily;
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn show_personal(&mut self) {
        self.store_active_snapshot();
        self.mode = Mode::Personal;
        if !self.personal_snapshots.contains_key(self.difficulty_key()) {
            self.personal_snapshots.insert(
                self.difficulty_key().to_string(),
                snapshot_from_seed(random_seed()),
            );
        }
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn next_difficulty(&mut self) {
        self.store_active_snapshot();
        self.selected_difficulty = (self.selected_difficulty + 1) % DIFFICULTIES.len();
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn prev_difficulty(&mut self) {
        self.store_active_snapshot();
        self.selected_difficulty =
            (self.selected_difficulty + DIFFICULTIES.len() - 1) % DIFFICULTIES.len();
        self.load_mode_snapshot_for_selected_difficulty();
    }

    pub fn new_personal_board(&mut self) {
        self.store_active_snapshot();
        self.personal_snapshots.insert(
            self.difficulty_key().to_string(),
            snapshot_from_seed(random_seed()),
        );
        self.mode = Mode::Personal;
        self.load_mode_snapshot_for_selected_difficulty();
        self.save_async();
    }

    pub fn reset_board(&mut self) {
        if self.is_game_over {
            return;
        }
        let snapshot = snapshot_from_seed(self.seed);
        self.apply_snapshot(snapshot.clone());
        self.replace_mode_snapshot_for_selected_difficulty(snapshot);
        self.save_async();
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(3);
    }

    pub fn move_horizontal(&mut self, delta: isize) {
        self.cursor = match self.cursor {
            Focus::Stock => top_focus(delta.clamp(0, 5) as usize),
            Focus::Waste => top_focus((1isize + delta).clamp(0, 5) as usize),
            Focus::Foundation(idx) => top_focus((idx as isize + 2 + delta).clamp(0, 5) as usize),
            Focus::Tableau(col, row) => {
                let next_col = (col as isize + delta).clamp(0, 6) as usize;
                Focus::Tableau(next_col, row.min(self.max_tableau_row(next_col)))
            }
        };
        self.clamp_cursor();
    }

    pub fn move_vertical(&mut self, delta: isize) {
        self.cursor = match self.cursor {
            Focus::Stock | Focus::Waste | Focus::Foundation(_) if delta > 0 => {
                let col = top_to_tableau_col(self.cursor);
                Focus::Tableau(col, self.max_tableau_row(col))
            }
            Focus::Stock | Focus::Waste | Focus::Foundation(_) => self.cursor,
            Focus::Tableau(col, row) => {
                if delta < 0 {
                    if row == 0 {
                        top_focus(tableau_to_top_index(col))
                    } else {
                        Focus::Tableau(col, row - 1)
                    }
                } else {
                    Focus::Tableau(col, (row + 1).min(self.max_tableau_row(col)))
                }
            }
        };
        self.clamp_cursor();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(snapshot) = self.undo_stack.pop() {
            self.seed = snapshot.seed;
            self.stock = snapshot.stock;
            self.waste = snapshot.waste;
            self.foundations = snapshot.foundations;
            self.tableau = snapshot.tableau;
            self.is_game_over = snapshot.is_game_over;
            self.selection = None;
            self.store_active_snapshot();
            self.save_async();
            true
        } else {
            false
        }
    }

    pub fn activate(&mut self) -> bool {
        if matches!(self.cursor, Focus::Stock) {
            self.selection = None;
            self.push_undo();
            if self.draw_from_stock() {
                self.store_active_snapshot();
                self.save_async();
                return true;
            }
            self.undo_stack.pop();
            return false;
        }

        if self.is_game_over {
            return false;
        }

        if let Some(selection) = self.selection {
            if self.selection_matches_focus(selection) {
                self.selection = None;
                return true;
            }
            self.push_undo();
            if self.try_move(selection, self.cursor) {
                self.selection = None;
                self.after_mutation();
                return true;
            }
            self.undo_stack.pop();
            return false;
        }

        if let Some(selection) = self.selection_from_focus() {
            self.selection = Some(selection);
            true
        } else {
            false
        }
    }

    pub fn auto_move(&mut self) -> bool {
        if self.is_game_over {
            return false;
        }
        let selection = self.selection.or_else(|| self.selection_from_focus());
        let Some(selection) = selection else {
            return false;
        };

        self.push_undo();
        for foundation_idx in 0..4 {
            if self.try_move(selection, Focus::Foundation(foundation_idx)) {
                self.selection = None;
                self.after_mutation();
                return true;
            }
        }
        self.undo_stack.pop();
        false
    }

    pub fn auto_foundation_all(&mut self) -> bool {
        if self.is_game_over {
            return false;
        }
        self.selection = None;
        self.push_undo();
        let mut moved_any = false;
        loop {
            let mut moved = false;
            if self.waste_top().is_some() {
                for fi in 0..4 {
                    if self.try_move_to_foundation(Selection::Waste, fi) {
                        moved = true;
                        break;
                    }
                }
            }
            for col in 0..7 {
                if let Some(last) = self.tableau[col].last()
                    && last.face_up
                {
                    let row = self.tableau[col].len() - 1;
                    for fi in 0..4 {
                        if self.try_move_to_foundation(Selection::Tableau { col, row }, fi) {
                            moved = true;
                            break;
                        }
                    }
                }
            }
            if moved {
                moved_any = true;
            } else {
                break;
            }
        }
        if moved_any {
            self.after_mutation();
        } else {
            self.undo_stack.pop();
        }
        moved_any
    }

    pub fn score(&self) -> usize {
        self.foundations.iter().map(Vec::len).sum()
    }

    pub fn cursor_label(&self) -> String {
        match self.cursor {
            Focus::Stock => "stock".to_string(),
            Focus::Waste => "waste".to_string(),
            Focus::Foundation(idx) => format!("foundation {}", idx + 1),
            Focus::Tableau(col, row) => format!("tableau {}:{}", col + 1, row + 1),
        }
    }

    pub fn selection_label(&self) -> String {
        match self.selection {
            Some(Selection::Waste) => "waste".to_string(),
            Some(Selection::Foundation(idx)) => format!("foundation {}", idx + 1),
            Some(Selection::Tableau { col, row }) => format!("tableau {}:{}", col + 1, row + 1),
            None => "none".to_string(),
        }
    }

    pub fn waste_top(&self) -> Option<Card> {
        self.waste.last().copied()
    }

    pub fn visible_waste(&self) -> &[Card] {
        let count = self.draw_count().min(self.waste.len());
        &self.waste[self.waste.len().saturating_sub(count)..]
    }

    pub fn foundation_top(&self, idx: usize) -> Option<Card> {
        self.foundations
            .get(idx)
            .and_then(|pile| pile.last())
            .copied()
    }

    pub fn card_text(card: Card) -> String {
        card.label()
    }

    pub fn visible_tableau_card(&self, col: usize, row: usize) -> Option<TableauCard> {
        self.tableau
            .get(col)
            .and_then(|pile| pile.get(row))
            .copied()
    }

    pub fn max_tableau_height(&self) -> usize {
        self.tableau.iter().map(Vec::len).max().unwrap_or(0).max(1)
    }

    fn draw_from_stock(&mut self) -> bool {
        let draw_count = self.draw_count();
        draw_stock_once(&mut self.stock, &mut self.waste, draw_count)
    }

    fn selection_from_focus(&self) -> Option<Selection> {
        match self.cursor {
            Focus::Stock => None,
            Focus::Waste => self.waste.last().map(|_| Selection::Waste),
            Focus::Foundation(idx) => self.foundations[idx]
                .last()
                .map(|_| Selection::Foundation(idx)),
            Focus::Tableau(col, row) => {
                let pile = &self.tableau[col];
                let card = pile.get(row)?;
                if !card.face_up {
                    let first_face_up = pile.iter().position(|c| c.face_up)?;
                    Some(Selection::Tableau {
                        col,
                        row: first_face_up,
                    })
                } else {
                    Some(Selection::Tableau { col, row })
                }
            }
        }
    }

    fn selection_matches_focus(&self, selection: Selection) -> bool {
        match (selection, self.cursor) {
            (Selection::Waste, Focus::Waste) => true,
            (Selection::Foundation(a), Focus::Foundation(b)) => a == b,
            (Selection::Tableau { col: a, row: ar }, Focus::Tableau(b, br)) => a == b && ar == br,
            _ => false,
        }
    }

    fn try_move(&mut self, selection: Selection, target: Focus) -> bool {
        match target {
            Focus::Stock | Focus::Waste => false,
            Focus::Foundation(idx) => self.try_move_to_foundation(selection, idx),
            Focus::Tableau(col, _) => self.try_move_to_tableau(selection, col),
        }
    }

    fn try_move_to_foundation(&mut self, selection: Selection, foundation_idx: usize) -> bool {
        let Some(card) = self
            .peek_selection(selection)
            .filter(|cards| cards.len() == 1)
            .and_then(|cards| cards.first().copied())
        else {
            return false;
        };

        if card.suit.foundation_index() != foundation_idx {
            return false;
        }

        let can_place = match self.foundations[foundation_idx].last().copied() {
            Some(top) => top.suit == card.suit && card.rank == top.rank + 1,
            None => card.rank == 1,
        };
        if !can_place {
            return false;
        }

        let moved = self.remove_selection(selection);
        if moved.len() != 1 {
            return false;
        }
        self.foundations[foundation_idx].push(moved[0]);
        true
    }

    fn try_move_to_tableau(&mut self, selection: Selection, target_col: usize) -> bool {
        let Some(cards) = self.peek_selection(selection) else {
            return false;
        };
        let moving_first = cards[0];

        let can_place = match self.tableau[target_col]
            .iter()
            .rev()
            .find(|card| card.face_up)
            .map(|card| card.card)
        {
            Some(target_card) => moving_first.can_stack_on(target_card),
            None => moving_first.rank == 13,
        };
        if !can_place {
            return false;
        }

        if matches!(selection, Selection::Tableau { col, .. } if col == target_col) {
            return false;
        }

        let moved = self.remove_selection(selection);
        if moved.is_empty() {
            return false;
        }

        self.tableau[target_col].extend(moved.into_iter().map(|card| TableauCard {
            card,
            face_up: true,
        }));
        true
    }

    fn peek_selection(&self, selection: Selection) -> Option<Vec<Card>> {
        match selection {
            Selection::Waste => self.waste.last().copied().map(|card| vec![card]),
            Selection::Foundation(idx) => {
                self.foundations[idx].last().copied().map(|card| vec![card])
            }
            Selection::Tableau { col, row } => {
                let pile = self.tableau.get(col)?;
                let slice = pile.get(row..)?;
                if slice.iter().any(|card| !card.face_up) {
                    return None;
                }
                Some(slice.iter().map(|entry| entry.card).collect())
            }
        }
    }

    fn remove_selection(&mut self, selection: Selection) -> Vec<Card> {
        match selection {
            Selection::Waste => self.waste.pop().into_iter().collect(),
            Selection::Foundation(idx) => self.foundations[idx].pop().into_iter().collect(),
            Selection::Tableau { col, row } => {
                let moved = self.tableau[col].split_off(row);
                self.reveal_tableau_top(col);
                moved.into_iter().map(|entry| entry.card).collect()
            }
        }
    }

    fn reveal_tableau_top(&mut self, col: usize) {
        if let Some(last) = self.tableau[col].last_mut()
            && !last.face_up
        {
            last.face_up = true;
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.current_snapshot());
    }

    fn after_mutation(&mut self) {
        self.check_for_win();
        self.store_active_snapshot();
        self.save_async();
    }

    fn check_for_win(&mut self) {
        if self.foundations.iter().all(|pile| pile.len() == 13) {
            self.is_game_over = true;
            if self.mode == Mode::Daily {
                self.svc.record_win_task(
                    self.user_id,
                    self.difficulty_key().to_string(),
                    self.score() as i32,
                );
            }
        }
    }

    fn replace_mode_snapshot_for_selected_difficulty(&mut self, snapshot: Snapshot) {
        match self.mode {
            Mode::Daily => {
                self.daily_snapshots
                    .insert(self.difficulty_key().to_string(), snapshot);
            }
            Mode::Personal => {
                self.personal_snapshots
                    .insert(self.difficulty_key().to_string(), snapshot);
            }
        }
    }

    fn store_active_snapshot(&mut self) {
        self.replace_mode_snapshot_for_selected_difficulty(self.current_snapshot());
    }

    fn current_snapshot(&self) -> Snapshot {
        Snapshot {
            seed: self.seed,
            stock: self.stock.clone(),
            waste: self.waste.clone(),
            foundations: self.foundations.clone(),
            tableau: self.tableau.clone(),
            is_game_over: self.is_game_over,
        }
    }

    fn load_mode_snapshot_for_selected_difficulty(&mut self) {
        let key = self.difficulty_key();
        let snapshot = match self.mode {
            Mode::Daily => self.daily_snapshots.get(key).cloned(),
            Mode::Personal => self.personal_snapshots.get(key).cloned(),
        }
        .unwrap_or_else(|| {
            let snapshot = if self.mode == Mode::Daily {
                snapshot_from_seed(self.svc.get_daily_seed(key))
            } else {
                snapshot_from_seed(random_seed())
            };
            if self.mode == Mode::Daily {
                self.daily_snapshots
                    .insert(key.to_string(), snapshot.clone());
            } else {
                self.personal_snapshots
                    .insert(key.to_string(), snapshot.clone());
            }
            snapshot
        });
        self.apply_snapshot(snapshot);
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot) {
        self.seed = snapshot.seed;
        self.stock = snapshot.stock;
        self.waste = snapshot.waste;
        self.foundations = snapshot.foundations;
        self.tableau = snapshot.tableau;
        self.is_game_over = snapshot.is_game_over;
        self.cursor = Focus::Stock;
        self.selection = None;
        self.scroll_offset = 0;
        self.undo_stack.clear();
        self.clamp_cursor();
    }

    fn save_async(&self) {
        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            difficulty_key: self.difficulty_key().to_string(),
            puzzle_date: puzzle_date_for_mode(self.mode, self.svc.today()),
            puzzle_seed: self.seed as i64,
            stock: serde_json::to_value(&self.stock).unwrap_or_default(),
            waste: serde_json::to_value(&self.waste).unwrap_or_default(),
            foundations: serde_json::to_value(&self.foundations).unwrap_or_default(),
            tableau: serde_json::to_value(&self.tableau).unwrap_or_default(),
            is_game_over: self.is_game_over,
            score: self.score() as i32,
        });
    }

    fn clamp_cursor(&mut self) {
        if let Focus::Tableau(col, row) = self.cursor {
            self.cursor = Focus::Tableau(col, row.min(self.max_tableau_row(col)));
        }
    }

    fn max_tableau_row(&self, col: usize) -> usize {
        self.tableau
            .get(col)
            .map_or(0, |pile| pile.len().saturating_sub(1))
    }
}

fn snapshot_from_game(game: &Game) -> Snapshot {
    Snapshot {
        seed: game.puzzle_seed as u64,
        stock: serde_json::from_value(game.stock.clone()).unwrap_or_default(),
        waste: serde_json::from_value(game.waste.clone()).unwrap_or_default(),
        foundations: serde_json::from_value(game.foundations.clone())
            .unwrap_or_else(|_| array::from_fn(|_| Vec::new())),
        tableau: serde_json::from_value(game.tableau.clone())
            .unwrap_or_else(|_| array::from_fn(|_| Vec::new())),
        is_game_over: game.is_game_over,
    }
}

fn snapshot_from_seed(seed: u64) -> Snapshot {
    let mut deck = deck();
    shuffle(&mut deck, seed);

    let mut tableau: [Vec<TableauCard>; 7] = array::from_fn(|_| Vec::new());
    let mut cursor = 0;
    for (col, pile) in tableau.iter_mut().enumerate() {
        for row in 0..=col {
            pile.push(TableauCard {
                card: deck[cursor],
                face_up: row == col,
            });
            cursor += 1;
        }
    }

    Snapshot {
        seed,
        stock: deck[cursor..].to_vec(),
        waste: Vec::new(),
        foundations: array::from_fn(|_| Vec::new()),
        tableau,
        is_game_over: false,
    }
}

fn deck() -> Vec<Card> {
    let mut cards = Vec::with_capacity(52);
    for suit in [Suit::Hearts, Suit::Diamonds, Suit::Clubs, Suit::Spades] {
        for rank in 1..=13 {
            cards.push(Card { suit, rank });
        }
    }
    cards
}

fn shuffle(deck: &mut [Card], seed: u64) {
    let mut state = seed;
    for idx in (1..deck.len()).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let swap_idx = (state as usize) % (idx + 1);
        deck.swap(idx, swap_idx);
    }
}

fn random_seed() -> u64 {
    OsRng.next_u64()
}

fn draw_stock_once(stock: &mut Vec<Card>, waste: &mut Vec<Card>, draw_count: usize) -> bool {
    if stock.is_empty() {
        if waste.is_empty() {
            return false;
        }
        *stock = waste.iter().rev().copied().collect();
        waste.clear();
        return true;
    }

    let draw_count = draw_count.min(stock.len());
    for _ in 0..draw_count {
        if let Some(card) = stock.pop() {
            waste.push(card);
        }
    }
    true
}

fn puzzle_date_for_mode(mode: Mode, today: NaiveDate) -> Option<NaiveDate> {
    match mode {
        Mode::Daily => Some(today),
        Mode::Personal => None,
    }
}

fn is_current_daily_game(saved_date: Option<NaiveDate>, today: NaiveDate) -> bool {
    saved_date == Some(today)
}

fn top_focus(index: usize) -> Focus {
    match index {
        0 => Focus::Stock,
        1 => Focus::Waste,
        2 => Focus::Foundation(0),
        3 => Focus::Foundation(1),
        4 => Focus::Foundation(2),
        _ => Focus::Foundation(3),
    }
}

fn top_to_tableau_col(focus: Focus) -> usize {
    match focus {
        Focus::Stock => 0,
        Focus::Waste => 1,
        Focus::Foundation(idx) => (idx + 2).min(6),
        Focus::Tableau(col, _) => col,
    }
}

fn tableau_to_top_index(col: usize) -> usize {
    match col {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> State {
        let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy db");
        State::new(
            Uuid::nil(),
            SolitaireService::new(
                db.clone(),
                tokio::sync::broadcast::channel(4).0,
                crate::app::arcade::chips::svc::ChipService::new(db),
            ),
            Vec::new(),
        )
    }

    #[test]
    fn seeded_deal_uses_full_deck() {
        let snapshot = snapshot_from_seed(42);
        let count = snapshot.stock.len()
            + snapshot.waste.len()
            + snapshot.foundations.iter().map(Vec::len).sum::<usize>()
            + snapshot.tableau.iter().map(Vec::len).sum::<usize>();
        assert_eq!(count, 52);
        assert_eq!(snapshot.stock.len(), 24);
    }

    #[test]
    fn draw_one_draws_one_card() {
        let mut stock = vec![
            Card {
                suit: Suit::Hearts,
                rank: 1,
            },
            Card {
                suit: Suit::Spades,
                rank: 13,
            },
        ];
        let mut waste = Vec::new();
        assert!(draw_stock_once(&mut stock, &mut waste, 1));
        assert_eq!(stock.len(), 1);
        assert_eq!(waste.len(), 1);
    }

    #[test]
    fn draw_three_draws_up_to_three_cards() {
        let mut stock = vec![
            Card {
                suit: Suit::Hearts,
                rank: 1,
            },
            Card {
                suit: Suit::Spades,
                rank: 13,
            },
            Card {
                suit: Suit::Clubs,
                rank: 7,
            },
            Card {
                suit: Suit::Diamonds,
                rank: 10,
            },
        ];
        let mut waste = Vec::new();
        assert!(draw_stock_once(&mut stock, &mut waste, 3));
        assert_eq!(stock.len(), 1);
        assert_eq!(waste.len(), 3);
        assert_eq!(waste.last().map(|card| card.rank), Some(13));
    }

    #[test]
    fn moving_from_tableau_reveals_next_card() {
        let mut state = test_state();
        state.tableau[0] = vec![TableauCard {
            card: Card {
                suit: Suit::Clubs,
                rank: 8,
            },
            face_up: true,
        }];
        state.tableau[1] = vec![
            TableauCard {
                card: Card {
                    suit: Suit::Hearts,
                    rank: 8,
                },
                face_up: false,
            },
            TableauCard {
                card: Card {
                    suit: Suit::Hearts,
                    rank: 7,
                },
                face_up: true,
            },
        ];

        assert!(state.try_move(Selection::Tableau { col: 1, row: 1 }, Focus::Tableau(0, 0)));
        assert!(state.tableau[1][0].face_up);
    }

    #[test]
    fn ace_can_move_to_matching_foundation() {
        let mut state = test_state();
        state.waste = vec![Card {
            suit: Suit::Spades,
            rank: 1,
        }];
        assert!(state.try_move(Selection::Waste, Focus::Foundation(3)));
    }
}
