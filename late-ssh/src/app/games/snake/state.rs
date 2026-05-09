use ratatui::style::Color;
use uuid::Uuid;

use super::svc::SnakeService;
use rand::{Rng};
use std::time::Duration;
use std::sync::Mutex;

pub struct State {
    pub user_id: Uuid,
    pub score: i32,
    pub best_score: i32,
    pub is_game_over: bool,
    pub is_paused: bool,
    pub svc: SnakeService,
    pub tick: Mutex<i32>,
    pub level: Level,
    pub field: Field,
    pub cobra: Cobra,
    pub set_exit: bool,
    pub input_queue: Vec<u8>,
    last_key: Option<u8>,
}

impl State {
    pub fn new(
        user_id: Uuid,
        svc: SnakeService,
        best_score: i32,
        height: u32,
        width: u32,
    ) -> Self {
        let field = Field::new_empty(height - 3, width - 2);
        let cobra = Cobra::new(&field, 3);
        let mut state = Self {
            score: 0,
            tick: Mutex::new(0),
            level: Level::new(1),
            field,
            cobra,
            set_exit: false,
            is_game_over: false,
            input_queue: vec!(),
            last_key: None,
            best_score,
            is_paused: false,
            svc,
            user_id,
        };
        state.reset_level(true);
        state.reset_game();
        state
    }
    pub fn persist_state(&self) {}
    pub fn restore(&self) {}
    pub fn toggle_pause(&self) {}

    fn show_game_over(&mut self) {
        //utilprint("Game Over!".red());
        println!(
            "Congratulations you've reached level {} and your score was {:05}!",
            self.level.number, self.score
        );
        println!("Press R to try again! Or Q to exit!");
        let _tick = self.tick.lock();
        // blocks until lock is dropped
        // while self.tick.is_locked() {
        //     std::thread::sleep(Duration::from_secs(1));
        //     let queue = self.input_queue.lock().unwrap();
        //     let key = queue.first();
        //     if let Some(EventType::KeyPress(Key::KeyR)) = key {
        //         println!("Restarting Game...");
        //         std::thread::sleep(Duration::from_secs(1));
        //         break;
        //     } else if let Some(EventType::KeyPress(Key::KeyQ)) = key {
        //         println!("Quiting Game...");
        //         std::thread::sleep(Duration::from_secs(1));
        //         break;
        //     }
        // }
        self.is_game_over = false;
    }

    fn bye(&mut self) {
        println!("You pressed Q, Good bye!!!");
    }

    pub fn handle_key(&mut self) -> bool {
        let key = self.input_queue.pop();
        let mut accel = false;
        if let Some(k) = key {
            let key_dir = self.cobra.dir_from_key(&k);
            if let Some(d) = key_dir
                && d != self.cobra.head_dir {
                accel = true;
            }
            if key == self.last_key {
                accel = true;
            }
            self.last_key = key;
        }
        if let Some(k) = self.last_key {
            let dir = self.cobra.dir_from_key(&k);
            if let Some(d) = dir && d != self.cobra.head_dir {
                self.cobra.set_direction(d);
            }
        }
        accel
    }

    fn kill_cobra(&mut self) {
        if self.cobra.lives > 0 {
            self.cobra.lives -= 1;
        } else {
            self.is_game_over = true;
        }
        println!("You died, restarting level!");
        std::thread::sleep(std::time::Duration::from_secs(2));
        self.reset_level(true);
    }

    pub fn reset_game(&mut self) {
        {
            let mut tick = self.tick.lock().unwrap();
            *tick = 0;
        }
        self.cobra.lives = 3;
        self.score = 0;
        self.level.number = 1;
        self.reset_level(true);
    }

    pub fn reset_level(&mut self, reset_cobra: bool) {
        if reset_cobra {
            self.cobra.reset(&self.field);
        }
        self.field.things.clear();
        self.field.gen_things(self.level.number, &self.cobra);
        self.last_key = None;
        self.next_tick();
    }

    fn level_up(&mut self) {
        self.level.number += 1;
        self.reset_level(false);
    }

    pub fn get_field(&self) -> Vec<Vec<Option<&ThingOnScreen>>> {
        let mut grid: Vec<Vec<Option<&ThingOnScreen>>> =
            Vec::with_capacity(self.field.height as usize);
        for _ in 0..self.field.height {
            let mut row: Vec<Option<&ThingOnScreen>> =
                Vec::with_capacity(self.field.width as usize);
            for _ in 0..self.field.width {
                row.push(None);
            }
            grid.push(row);
        }

        for thing in &self.field.things {
            let (iy, ix) = thing.get_idx();
            if thing.effect.is_some() {
                grid[iy][ix] = Some(thing);
            }
        }
        for thing in &self.field.cobra_things {
            let (iy, ix) = thing.get_idx();
            grid[iy][ix] = Some(thing);
        }
        for thing in &self.field.edges {
            let (iy, ix) = thing.get_idx();
            grid[iy][ix] = Some(thing)
        }
        grid
    }

    fn score_up(&mut self, value: i32) {
        let mut multiplier = (1.max(self.level.number / 2)) as f32;
        if let CobraState::PoweredUp = self.cobra.state {
            multiplier *= 2.0;
        }
        self.score += (value as f32 * multiplier) as i32;
    }

    fn next_tick(&mut self) {
        let accel = self.handle_key();
        if self.set_exit {
            self.bye();
            return;
        }
        let mut effect: Option<CobraEffect> = None;
        if let CobraState::Alive = self.cobra.state {
            effect = self.cobra.move_cobra(&mut self.field);
        } else if let CobraState::PoweredUp = self.cobra.state {
            effect = self.cobra.move_cobra(&mut self.field);
            self.cobra.power_ticks_left -= 1;
            if self.cobra.power_ticks_left == 0 {
                self.cobra.state = CobraState::Alive;
            }
        }

        if let Some(CobraEffect::Blow) = effect {
            if self.cobra.lives == 0 {
                self.is_game_over = true;
                return;
            } else {
                self.kill_cobra();
            }
        } else if self.field.food_left() == 0 {
            self.level_up();
            self.score_up(100 * self.level.number as i32);
        }
        let mut min_delay = 1000.0;
        let mut cobra_speed = self.level.get_speed(&min_delay);
        if let CobraState::PoweredUp = self.cobra.state {
            cobra_speed *= 2.0;
        }
        if cobra_speed > 0.0 {
            min_delay /= cobra_speed;
        }
        let mut dur = Duration::from_secs_f32(min_delay);
        if accel {
            dur /= 2;
        }
        self.score_up(1);
        let mut tick = self.tick.lock().unwrap();
        *tick += 1;
    //     let tick = self.tick.try_lock_for(dur);
    //     if tick.is_some() {
    //         drop(tick);
    //     }
    // }
    }
}



#[derive(Clone, Copy, PartialEq)]
enum CobraEffect {
    Blow,
    Grow,
    PowerUp,
}

#[derive(Debug, Clone)]
struct Position {
    x: u32,
    y: u32,
}

impl Position {
    fn new(field: &Field) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            x: rng.gen_range(0..=(field.width - 1)),
            y: rng.gen_range(0..=(field.height - 1)),
        }
    }

    fn gen_without_collision(field: &Field, cobra: &Cobra) -> Self {
        let mut new_pos = Self::new(field);
        loop {
            if cobra.collide(&new_pos) {
                new_pos = Self::new(field);
                continue;
            }
            let mut has_collided = false;
            let mut all_things = Vec::new();
            all_things.extend(&field.things);
            all_things.extend(&field.edges);
            for thing in all_things {
                if thing.collide(&new_pos) {
                    has_collided = true;
                    break;
                }
            }
            if has_collided {
                new_pos = Self::new(field);
                continue;
            }
            break;
        }
        new_pos
    }
}

enum ThingKind {
    Food,
    Drug,
    Rock,
    Cobra,
    Edge,
}

pub struct ThingOnScreen {
    position: Position,
    pub value: String,
    pub color: Color,
    effect: Option<CobraEffect>,
    kind: ThingKind,
}

impl ThingOnScreen {
    fn from_kind_at_pos(kind: ThingKind, position: Position) -> Self {
        match kind {
            ThingKind::Food => Self {
                position,
                kind,
                color: Color::Yellow,
                effect: Some(CobraEffect::Grow),
                value: String::from("◉"),
            },
            ThingKind::Drug => Self {
                position,
                kind,
                color: Color::Magenta,
                effect: Some(CobraEffect::PowerUp),
                value: String::from("★"),
            },
            ThingKind::Rock => Self {
                position,
                kind,
                color: Color::Gray,
                effect: Some(CobraEffect::Blow),
                value: String::from("☠"),
            },
            ThingKind::Cobra => Self {
                position,
                kind,
                color: Color::Green,
                effect: None,
                value: String::from("━"),
            },
            _ => Self {
                position,
                kind,
                color: Color::White,
                effect: None,
                value: String::new(),
            },
        }
    }

    pub fn get_cobra_pixel(value: String, position: Position) -> Self {
        Self {
            position,
            kind: ThingKind::Cobra,
            color: Color::Green,
            effect: None,
            value,
        }
    }

    fn get_idx(&self) -> (usize, usize) {
        (self.position.y as usize, self.position.x as usize)
    }

    fn get_edge(x: u32, y: u32, height: u32, width: u32) -> Option<Self> {
        let mut value = String::new();
        if x == 0 && y == 0 {
            value += "╔"
        } else if x == width - 1 && y == height - 1 {
            value += "╝"
        } else if x == 0 && y == height - 1 {
            value += "╚"
        } else if y == 0 && x == width - 1 {
            value += "╗"
        } else if x == 0 || x == width - 1 {
            value += "║"
        } else if y == 0 || y == height - 1 {
            value += "═"
        }
        if value.len() > 0 {
            Some(Self {
                effect: Some(CobraEffect::Blow),
                position: Position { x, y },
                color: Color::White,
                kind: ThingKind::Edge,
                value,
            })
        } else {
            None
        }
    }

    fn gen_at_the_field(kind: ThingKind, field: &Field, cobra: &Cobra) -> Self {
        let position = Position::gen_without_collision(field, cobra);
        Self::from_kind_at_pos(kind, position)
    }

    fn collide(&self, pos: &Position) -> bool {
        pos.x == self.position.x && pos.y == self.position.y
    }
}

pub struct Level {
    number: u8,
}

impl std::fmt::Display for Level{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
       write!(f, "{}", self.number); 
       Ok(()) 
    }
}

impl Level {
    fn new(number: u8) -> Self {
        Self { number }
    }

    fn get_speed(&self, min_delay: &f32) -> f32 {
        self.number as f32 * min_delay
    }
}

#[derive(Debug, PartialEq)]
enum Direction {
    Up,
    Down,
    Right,
    Left,
}

#[derive(Debug)]
enum CobraState {
    Alive,
    PoweredUp,
    Dead,
}

pub struct Cobra {
    body: Vec<Position>,
    head_dir: Direction,
    state: CobraState,
    lives: u8,
    power_ticks_left: u8,
}

impl Cobra {
    fn new(field: &Field, lives: u8) -> Self {
        let body: Vec<Position> = Vec::new();
        let mut cobra = Self {
            body,
            head_dir: Direction::Right,
            state: CobraState::Alive,
            lives,
            power_ticks_left: 0,
        };
        cobra.reset(field);
        cobra
    }

    fn get_value(&self, index: usize) -> String {
        let mut value = String::new();
        if let CobraState::PoweredUp = self.state {
            value += "@M"
        } else {
            value += "@G"
        }
        let mut prev_thing: Option<&Position> = None;
        if index > 0 {
            prev_thing = self.body.get(0.max(index - 1));
        }
        let next_thing = self.body.get(index + 1);

        if prev_thing.is_some() && next_thing.is_some() {
            // is body
            value += "#25C9";
        } else if prev_thing.is_some() && next_thing.is_none() {
            // is head
            value += "#263B";
        } else if next_thing.is_some() && prev_thing.is_none() {
            // is tail
            value += "#25CF";
        }
        value
    }

    fn reset(&mut self, field: &Field) {
        self.body.clear();
        self.head_dir = Direction::Right;
        self.state = CobraState::Alive;
        let center_pos = field.get_center();
        let mut body_pos = center_pos.clone();
        let mut tail_pos = center_pos.clone();
        body_pos.x -= 1;
        tail_pos.x -= 2;
        self.body.push(center_pos);
        self.body.push(body_pos);
        self.body.push(tail_pos);
    }

    fn collide(&self, pos: &Position) -> bool {
        for body_part in &self.body {
            if pos.x == body_part.x && pos.y == body_part.y {
                return true;
            }
        }
        false
    }

    fn head_collide(&self, pos: &Position) -> bool {
        let mut collides = false;
        for body_part in &self.body[..(self.body.len() - 2)] {
            if pos.x == body_part.x && pos.y == body_part.y {
                collides = true;
                break;
            }
        }
        collides
    }

    fn dir_from_key(&self, key: &u8) -> Option<Direction> {
        match key {
            b'A' => Some(Direction::Up),
            b'B' => Some(Direction::Down),
            b'C' => Some(Direction::Right),
            b'D' => Some(Direction::Left),
            _ => None,
        }
    }

    fn set_direction(&mut self, direction: Direction) {
        self.head_dir = direction
    }

    fn move_cobra(&mut self, field: &mut Field) -> Option<CobraEffect> {
        let neck_i = self.body.len() - 1;
        let neck = &self.body[neck_i];
        let mut head_pos = neck.clone();

        // Move head
        match self.head_dir {
            Direction::Up => head_pos.y = (head_pos.y as i32 - 1).max(0) as u32,
            Direction::Down => {
                head_pos.y = (head_pos.y as i32 + 1).min((field.height - 1) as i32) as u32
            }
            Direction::Right => {
                head_pos.x = (head_pos.x as i32 + 1).min((field.width - 1) as i32) as u32
            }
            Direction::Left => head_pos.x = (head_pos.x as i32 - 1).max(0) as u32,
        }

        let mut effect: Option<CobraEffect> = None;
        // Check for self collision
        if self.head_collide(&head_pos) {
            effect = Some(CobraEffect::Blow);
        }

        // Handles edge collision
        for thing in &mut field.edges {
            if thing.collide(&head_pos) {
                effect = thing.effect;
                break;
            }
        }

        // Check for thing colision
        for thing in &mut field.things {
            if thing.collide(&head_pos) {
                effect = thing.effect;
                // Consumes thing effect
                thing.effect = None;
                break;
            }
        }
        // Create new neck as can change depending on move Direction
        self.body.push(head_pos);
        match effect {
            Some(CobraEffect::Blow) => self.state = CobraState::Dead,
            Some(CobraEffect::PowerUp) => {
                self.state = CobraState::PoweredUp;
                self.power_ticks_left = u8::MAX;
                self.body.remove(0);
            }
            Some(CobraEffect::Grow) => (),
            // move cobra
            _ => {
                self.body.remove(0);
            }
        }
        field.cobra_things.clear();
        for (p, position) in self.body.iter().enumerate() {
            let value = self.get_value(p);
            field
                .cobra_things
                .push(ThingOnScreen::get_cobra_pixel(value, position.clone()));
        }
        effect
    }
}

pub struct Field {
    edges: Vec<ThingOnScreen>,
    things: Vec<ThingOnScreen>,
    cobra_things: Vec<ThingOnScreen>,
    pub height: u32,
    pub width: u32,
}

impl Field {
    fn get_center(&self) -> Position {
        Position {
            x: self.width / 2,
            y: self.height / 2,
        }
    }

    fn get_edges(height: u32, width: u32) -> Vec<ThingOnScreen> {
        let mut edges = Vec::new();
        for i in 0..width {
            for j in 0..height {
                let edge = ThingOnScreen::get_edge(i, j, height, width);
                if let Some(e) = edge {
                    edges.push(e);
                }
            }
        }
        edges
    }

    fn new_empty(height: u32, width: u32) -> Self {
        Self {
            edges: Self::get_edges(height, width),
            things: Vec::new(),
            cobra_things: Vec::new(),
            height,
            width,
        }
    }

    fn gen_things(&mut self, level: u8, cobra: &Cobra) {
        let nthings: u8 = 4 + (2 * level);
        // Add rocks
        for _ in 0..(nthings - level) {
            let rock = ThingOnScreen::gen_at_the_field(ThingKind::Rock, self, cobra);
            self.things.push(rock);
        }
        // Add food
        for _ in 0..(level * 2) as usize {
            let food = ThingOnScreen::gen_at_the_field(ThingKind::Food, self, cobra);
            self.things.push(food);
        }
        // Add Drug
        let drug = ThingOnScreen::gen_at_the_field(ThingKind::Drug, self, cobra);
        self.things.push(drug);
    }

    fn food_left(&self) -> i32 {
        let mut food_left = 0;
        for thing in &self.things {
            if let ThingKind::Food = thing.kind
                && thing.effect.is_some()
            {
                food_left += 1;
            }
        }
        food_left
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    
}
