use uuid::Uuid;
use std::sync::{Arc, Mutex};
use super::svc::SnakeService;
use rdev::{Event, EventType, Key, listen};

pub struct State {
    pub user_id: Uuid,
    pub score: i32,
    pub best_score: i32,
    pub is_game_over: bool,
    pub is_paused: bool,
    pub svc: SnakeService,
    pub tick: i32,
    pub level: Level,
    pub field: Field,
    pub cobra: Cobra,
    pub set_exit: bool,
    pub game_over: bool,
    input_queue: Vec<u8>,
    last_key: Option<u8>,
    key_is_pressed: bool,
}

impl State {
    fn new(
        user_id: Uuid,
        svc: SnakeService,
        best_score: i32,
        height: u32,
        width: u32,
        input_queue: Arc<Mutex<Vec<EventType>>>,
        key_is_pressed: Arc<Mutex<bool>>,
        tick: i32,
    ) -> Self {
        let field = Field::new_empty(height - 3, width - 2);
        let cobra = Cobra::new(&field, 3);
        Self {
            score: 0,
            tick,
            level: Level::new(1),
            field,
            cobra,
            set_exit: false,
            game_over: false,
            input_queue,
            last_key: None,
            key_is_pressed,
            best_score: 0,
            is_game_over: false,
            is_paused: false,
            svc,
            user_id
        }
    }
    fn persist_state(&self) {}
    fn restore(&self) {}
    fn toggle_pause(&self) {}

    fn show_game_over(&mut self) {
        self.clear_screen();
        utilprint("Game Over!".red());
        println!(
            "Congratulations you've reached level {} and your score was {:05}!",
            self.level.number, self.score
        );
        println!("Press R to try again! Or Q to exit!");
        let _tick = self.tick.lock();
        // blocks until lock is dropped
        while self.tick.is_locked() {
            std::thread::sleep(Duration::from_secs(1));
            let queue = self.input_queue.lock().unwrap();
            let key = queue.first();
            if let Some(EventType::KeyPress(Key::KeyR)) = key {
                println!("Restarting Game...");
                std::thread::sleep(Duration::from_secs(1));
                break;
            } else if let Some(EventType::KeyPress(Key::KeyQ)) = key {
                println!("Quiting Game...");
                std::thread::sleep(Duration::from_secs(1));
                break;
            }
        }
        self.game_over = false;
    }

    fn bye(&mut self) {
        self.clear_screen();
        println!("You pressed Q, Good bye!!!");
    }

    fn handle_key(&mut self) -> bool {
        let mut queue = self.input_queue.lock().unwrap();
        let key = queue.pop();
        let mut accel = false;
        if let Some(k) = key {
            let key_dir = self.cobra.dir_from_key(&k);
            if let Some(d) = key_dir
                && d != self.cobra.head_dir
            {
                accel = true;
            } else if *self.key_is_pressed.lock().unwrap() {
                accel = true;
            }
            self.last_key = key;
        }
        if let Some(k) = self.last_key {
            let dir = self.cobra.dir_from_key(&k);
            if let EventType::KeyPress(Key::KeyQ) = k {
                self.set_exit = true;
            } else if let Some(d) = dir {
                self.cobra.set_direction(d);
            }
        }
        accel
    }

    fn kill_cobra(&mut self) {
        if self.cobra.lives > 0 {
            self.cobra.lives -= 1;
        } else {
            self.game_over = true;
        }
        self.clear_screen();
        println!("You died, restarting level!");
        std::thread::sleep(std::time::Duration::from_secs(2));
        self.reset_level(true);
    }

    fn reset_game(&mut self) {
        let mut tick = self.tick.lock();
        *tick = 0;
        drop(tick);
        self.cobra.lives = 3;
        self.score = 0;
        self.level.number = 1;
        self.reset_level(true);
    }

    fn reset_level(&mut self, reset_cobra: bool) {
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

    fn get_field(&mut self) -> Vec<Vec<Option<&ThingOnScreen>>> {
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

    fn clear_screen(&mut self) {
        print!("\x1B[2J\x1B[1;1H");
        io::stdout().flush().unwrap();
    }

    fn score_up(&mut self, value: i32) {
        let mut multiplier = (1.max(self.level.number / 2)) as f32;
        if let CobraState::PoweredUp = self.cobra.state {
            multiplier *= 2.0;
        }
        self.score += (value as f32 * multiplier) as i32;
    }

    fn render(&mut self) {
        self.clear_screen();
        println!(
            "Field: {}x{}  Score: {:05}   lives: {}  Queue: {},   Head Dir: {:?}  Level:{} Food left: {} State: {:?}",
            &self.field.height,
            &self.field.width,
            &self.score,
            &self.cobra.lives,
            &self.input_queue.lock().unwrap().len(),
            &self.cobra.head_dir,
            &self.level.number,
            self.field.food_left(),
            &self.cobra.state
        );

        let field = self.get_field();
        for l in field {
            let mut line = String::with_capacity(l.len());
            for p in l {
                if let Some(pixel) = p {
                    line += &pixel.value[..]
                } else {
                    line += " "
                }
            }
            utilprint(line);
        }
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
                self.game_over = true;
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
        self.render();
        let mut tick = self.tick.lock();
        *tick += 1;
        let tick = self.tick.try_lock_for(dur);
        if tick.is_some() {
            drop(tick);
        }
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
        let mut rng = rand::rng();
        Self {
            x: rng.random_range(0..=(field.width - 1)),
            y: rng.random_range(0..=(field.height - 1)),
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

struct ThingOnScreen {
    position: Position,
    value: String,
    effect: Option<CobraEffect>,
    kind: ThingKind,
}

impl ThingOnScreen {
    fn from_kind_at_pos(kind: ThingKind, position: Position) -> Self {
        match kind {
            ThingKind::Food => Self {
                position,
                kind,
                effect: Some(CobraEffect::Grow),
                value: String::from("@Y#25C9"),
            },
            ThingKind::Drug => Self {
                position,
                kind,
                effect: Some(CobraEffect::PowerUp),
                value: String::from("@M#2605"),
            },
            ThingKind::Rock => Self {
                position,
                kind,
                effect: Some(CobraEffect::Blow),
                value: String::from("@W#2620"),
            },
            ThingKind::Cobra => Self {
                position,
                kind,
                effect: None,
                value: String::from("@G#2501"),
            },
            _ => Self {
                position,
                kind,
                effect: None,
                value: String::new(),
            },
        }
    }

    fn get_cobra_pixel(value: String, position: Position) -> Self {
        Self {
            position,
            kind: ThingKind::Cobra,
            effect: None,
            value,
        }
    }

    fn get_idx(&self) -> (usize, usize) {
        (self.position.y as usize, self.position.x as usize)
    }

    fn get_edge(x: u32, y: u32, height: u32, width: u32) -> Option<Self> {
        let mut value = String::from("@W");
        if x == 0 && y == 0 {
            value += "#2554"
        } else if x == width - 1 && y == height - 1 {
            value += "#255D"
        } else if x == 0 && y == height - 1 {
            value += "#255A"
        } else if y == 0 && x == width - 1 {
            value += "#2557"
        } else if x == 0 || x == width - 1 {
            value += "#2551"
        } else if y == 0 || y == height - 1 {
            value += "#2550"
        }
        if value.contains("#") {
            Some(Self {
                effect: Some(CobraEffect::Blow),
                position: Position { x, y },
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

struct Level {
    number: u8,
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

struct Cobra {
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

    fn dir_from_key(&self, key: &EventType) -> Option<Direction> {
        match key {
            EventType::KeyPress(Key::UpArrow) => Some(Direction::Up),
            EventType::KeyPress(Key::DownArrow) => Some(Direction::Down),
            EventType::KeyPress(Key::RightArrow) => Some(Direction::Right),
            EventType::KeyPress(Key::LeftArrow) => Some(Direction::Left),
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

struct Field {
    edges: Vec<ThingOnScreen>,
    things: Vec<ThingOnScreen>,
    cobra_things: Vec<ThingOnScreen>,
    height: u32,
    width: u32,
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
