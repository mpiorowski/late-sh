use crate::{entity::Entity, Direction, Hero, IntoDirection, Maze, PlayerId, Position};
use itertools::Itertools;
use rand::{seq::IteratorRandom, RngExt};
use std::time::Instant;

#[derive(Debug)]
pub struct Minotaur {
    id: PlayerId,
    name: String,
    chasing: Option<PlayerId>,
    maze_id: usize,
    position: Position,
    direction: Direction,
    speed: u64,
    vision: usize,
    aggression: f64,
    // pub view: View,
    last_update_time: Instant,
    pub kills: usize,
}

impl Minotaur {
    fn find_closest_hero(&mut self, visible_heros: &Vec<&Hero>) -> Option<PlayerId> {
        let closest_hero = visible_heros
            .iter()
            .min_by_key(|hero| self.position.distance_squared(hero.position()));

        closest_hero.map(|hero| hero.id())
    }

    pub fn new(
        name: String,
        maze_id: usize,
        position: Position,
        speed: u64,
        vision: usize,
        aggression: f64,
    ) -> Self {
        Self {
            id: PlayerId::new_v4(),
            name,
            chasing: None,
            maze_id,
            position,
            direction: Direction::North,
            speed,
            vision,
            aggression,
            last_update_time: Instant::now(),
            kills: 0,
        }
    }

    pub fn update(&mut self, maze: &mut Maze, visible_heros: Vec<&Hero>) {
        if let Some(hero_id) = self.chasing {
            if !visible_heros.iter().map(|h| h.id()).any(|id| id == hero_id) {
                self.chasing = None;
            }
        } else if let Some(hero_id) = self.find_closest_hero(&visible_heros) {
            self.chasing = Some(hero_id);
        }

        if self.last_update_time.elapsed() < self.movement_recovery_duration() {
            return;
        }

        let rng = &mut rand::rng();
        let (x, y) = self.position;

        let mut available_directions = [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ]
        .iter()
        .filter(|d| {
            let new_position = (
                (x as isize + d.as_offset().0).max(0) as usize,
                (y as isize + d.as_offset().1).max(0) as usize,
            );
            maze.is_valid_minotaur_position(new_position)
        })
        .collect_vec();

        if let Some(hero_id) = self.chasing {
            // Move toward chased hero
            if let Some(hero) = visible_heros.iter().find(|h| h.id() == hero_id) {
                let current_distance = self.position.distance(hero.position());
                available_directions = available_directions
                    .iter()
                    .filter(|d| {
                        let new_position = (
                            (x as isize + d.as_offset().0).max(0) as usize,
                            (y as isize + d.as_offset().1).max(0) as usize,
                        );
                        new_position.distance(hero.position()) < current_distance
                    }).copied()
                    .collect_vec();
            }
        }

        if !available_directions.is_empty() && rng.random_bool(self.aggression) {
            // Pick a random available direction
            let direction = available_directions.iter().choose(rng).unwrap();
            let new_position = (
                (x as isize + direction.as_offset().0).max(0) as usize,
                (y as isize + direction.as_offset().1).max(0) as usize,
            );
            self.position = new_position;
            self.last_update_time = Instant::now();
        }
    }

    pub fn is_chasing(&self, player_id: PlayerId) -> bool {
        self.chasing == Some(player_id)
    }

    pub fn is_chasing_someone(&self) -> bool {
        self.chasing.is_some()
    }
}

impl Entity for Minotaur {
    fn id(&self) -> PlayerId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn vision(&self) -> usize {
        self.vision
    }

    fn speed(&self) -> u64 {
        self.speed
    }

    fn position(&self) -> super::Position {
        self.position
    }

    fn direction(&self) -> Direction {
        self.direction
    }

    fn maze_id(&self) -> usize {
        self.maze_id
    }
}
