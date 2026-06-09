use crate::{
    entity::Entity,
    hero::{GameCommand, HeroState},
    minotaur::Minotaur,
    utils::{is_transparent, random_minotaur_name, to_player_name},
    AlarmLevel, GameColors, Hero, IntoDirection, Maze, PlayerId,
};
use anyhow::{anyhow, Result as AppResult};
use image::{Rgba, RgbaImage};
use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

pub const MAX_MAZE_ID: usize = 7;
pub const POWER_UPS_PER_ROOM: usize = 3;

pub struct Game {
    mazes: [Maze; MAX_MAZE_ID],
    taken_names: HashSet<String>,
    heros: HashMap<PlayerId, Hero>,
    hero_rooms: [Vec<PlayerId>; MAX_MAZE_ID],
    top_heros_map: HashMap<PlayerId, (String, usize, Duration)>,
    top_heros: Vec<(PlayerId, String, usize, Duration)>,
    minotaurs: HashMap<PlayerId, Minotaur>,
    minotaur_rooms: [Vec<PlayerId>; MAX_MAZE_ID],
    top_minotaurs_map: HashMap<PlayerId, (String, usize, usize)>,
    top_minotaurs: Vec<(PlayerId, String, usize, usize)>,
}

impl Game {
    const RESPAWN_INTERVAL: Duration = Duration::from_millis(1500);

    fn should_update_hero_record(&self, hero_id: PlayerId) -> bool {
        let hero = if let Some(hero) = self.get_hero(&hero_id) {
            hero
        } else {
            return false;
        };

        let &(_, record_maze_id, record_timer) =
            if let Some(record) = self.top_heros_map.get(&hero_id) {
                record
            } else {
                return true;
            };

        if let Some(duration) = hero.has_won() {
            if record_maze_id < MAX_MAZE_ID {
                return true;
            }
            return record_timer > duration;
        }

        if record_maze_id < hero.maze_id() {
            return true;
        } else if record_maze_id > hero.maze_id() {
            return false;
        }

        // Equal maze id record --> compare timer
        record_timer > hero.elapsed_duration_from_start()
    }

    fn update_hero_record(&mut self, hero_id: PlayerId) {
        if self.should_update_hero_record(hero_id) {
            let hero = if let Some(hero) = self.get_hero(&hero_id) {
                hero
            } else {
                return;
            };

            let record = if let Some(duration) = hero.has_won() {
                (hero.name().to_string(), MAX_MAZE_ID, duration)
            } else {
                (
                    hero.name().to_string(),
                    hero.maze_id(),
                    hero.elapsed_duration_from_start(),
                )
            };
            self.top_heros_map.insert(hero_id, record);
            self.update_top_heros();
        }
    }

    fn update_top_heros(&mut self) {
        self.top_heros = self
            .top_heros_map
            .iter()
            .map(|(&id, (name, record_maze_id, duration))| {
                (id, name.clone(), *record_maze_id, *duration)
            })
            .sorted_by(|a, b| {
                if b.2 == a.2 {
                    a.3.cmp(&b.3)
                } else {
                    b.2.cmp(&a.2)
                }
            })
            .collect_vec();
    }

    fn update_top_minotaurs(&mut self) {
        self.top_minotaurs = self
            .minotaurs
            .values()
            .sorted_by(|a, b| {
                if b.kills == a.kills {
                    b.maze_id().cmp(&a.maze_id())
                } else {
                    b.kills.cmp(&a.kills)
                }
            })
            .map(|minotaur| {
                (
                    minotaur.id(),
                    minotaur.name().to_string(),
                    minotaur.maze_id(),
                    minotaur.kills,
                )
            })
            .collect_vec();
    }

    pub fn update_time_step() -> Duration {
        Duration::from_millis(25)
    }

    pub fn draw_time_step() -> Duration {
        Duration::from_millis(50)
    }

    pub fn new() -> AppResult<Self> {
        let mut mazes: [Maze; MAX_MAZE_ID] = (0..MAX_MAZE_ID)
            .map(|maze_id| Maze::new(maze_id).build())
            .collect::<AppResult<Vec<Maze>>>()?
            .try_into()
            .expect("MAX_MAZE_ID mismatch");

        let mut minotaurs = HashMap::new();
        let mut minotaur_rooms = [const { Vec::new() }; MAX_MAZE_ID];

        for maze in mazes.iter_mut() {
            let mut maze_minotaurs = vec![];
            for index in 0..maze.id() {
                let name = format!("{}#{}{}", random_minotaur_name(), maze.id(), index);
                let minotaur = maze.spawn_minotaur(name);
                maze_minotaurs.push(minotaur.id());
                minotaurs.insert(minotaur.id(), minotaur);
            }
            minotaur_rooms[maze.id()] = maze_minotaurs;
        }

        Ok(Self {
            mazes,
            heros: HashMap::new(),
            hero_rooms: [const { Vec::new() }; MAX_MAZE_ID],
            taken_names: HashSet::new(),
            top_heros_map: HashMap::new(),
            top_heros: vec![],
            minotaurs,
            minotaur_rooms,
            top_minotaurs_map: HashMap::new(),
            top_minotaurs: vec![],
        })
    }

    pub fn top_heros(&self) -> &Vec<(PlayerId, String, usize, Duration)> {
        &self.top_heros
    }

    pub fn top_minotaurs(&self) -> &Vec<(PlayerId, String, usize, usize)> {
        &self.top_minotaurs
    }

    pub fn minotaurs_in_maze(&self, maze_id: usize) -> usize {
        self.minotaur_rooms[maze_id].len()
    }

    pub fn alarm_level(&self, hero_id: &PlayerId) -> (AlarmLevel, usize) {
        if let Some(hero) = self.get_hero(hero_id) {
            let maze_minotaurs = &self.minotaur_rooms[hero.maze_id()];
            if !maze_minotaurs.is_empty() {
                let mut alarm_level = AlarmLevel::NotChasing;
                let mut min_distance = usize::MAX;

                for minotaur_id in maze_minotaurs.iter() {
                    let minotaur = self.get_minotaur(minotaur_id).unwrap();
                    let distance = minotaur
                        .position()
                        .distance_squared(hero.position().into_direction(&hero.direction()));

                    if distance < min_distance {
                        min_distance = distance;
                    }

                    if minotaur.is_chasing(*hero_id) {
                        alarm_level = AlarmLevel::ChasingHero;
                    } else if minotaur.is_chasing_someone() && alarm_level < AlarmLevel::ChasingHero
                    {
                        alarm_level = AlarmLevel::ChasingOtherHero;
                    }
                }

                return (alarm_level, min_distance);
            }
        }
        (AlarmLevel::NoMinotaurs, usize::MAX)
    }

    pub fn add_player(&mut self, player_id: PlayerId, name: &str) {
        let rng = &mut rand::rng();
        let mut player_name = to_player_name(rng, name);
        while self.taken_names.contains(&player_name) {
            player_name = to_player_name(rng, name);
        }
        self.taken_names.insert(player_name.clone());

        let maze = &mut self.mazes[0];
        let mut hero = Hero::new(player_id, player_name, maze.hero_starting_position());
        maze.increase_attempted();

        let visible_positions =
            maze.get_and_cache_visible_positions(hero.position(), hero.direction(), hero.view());
        hero.update_past_visible_positions(visible_positions);

        self.hero_rooms[maze.id()].push(hero.id());

        self.top_heros_map.insert(
            hero.id(),
            (
                hero.name().to_string(),
                0,
                hero.elapsed_duration_from_start(),
            ),
        );

        self.update_top_heros();

        self.heros.insert(player_id, hero);
    }

    pub fn remove_player(&mut self, player_id: &PlayerId) {
        self.heros.remove(player_id);
    }

    pub fn get_hero(&self, id: &PlayerId) -> Option<&Hero> {
        self.heros.get(id)
    }

    pub fn get_minotaur(&self, id: &PlayerId) -> Option<&Minotaur> {
        self.minotaurs.get(id)
    }

    pub fn get_maze(&self, id: usize) -> &Maze {
        &self.mazes[id]
    }

    pub fn number_of_players(&self) -> usize {
        self.heros.len()
    }

    pub fn update(&mut self) {
        // Update heros
        for hero in self.heros.values_mut() {
            match hero.state {
                HeroState::WaitingToStart | HeroState::InMaze { .. } => {}
                HeroState::Dead { instant, .. } => {
                    if instant.elapsed() > Self::RESPAWN_INTERVAL {
                        // Move hero between rooms
                        self.hero_rooms[hero.maze_id()].retain(|id| *id != hero.id());
                        self.hero_rooms[0].push(hero.id());

                        let maze = &mut self.mazes[0];
                        hero.reset(maze.hero_starting_position());
                        let visible_positions = maze.get_and_cache_visible_positions(
                            hero.position(),
                            hero.direction(),
                            hero.view(),
                        );
                        hero.update_past_visible_positions(visible_positions);
                    }
                }

                HeroState::Victory { instant, .. } => {
                    if instant.elapsed() > Self::RESPAWN_INTERVAL {
                        // Move hero between rooms
                        self.hero_rooms[hero.maze_id()].retain(|id| *id != hero.id());
                        self.hero_rooms[0].push(hero.id());

                        let maze = &mut self.mazes[0];
                        hero.reset(maze.hero_starting_position());
                        let visible_positions = maze.get_and_cache_visible_positions(
                            hero.position(),
                            hero.direction(),
                            hero.view(),
                        );
                        hero.update_past_visible_positions(visible_positions);
                    }
                }
            }
        }

        // Update minotaurs
        let mut should_update_top_minotaurs = false;

        for minotaur in self.minotaurs.values_mut() {
            let maze_id = minotaur.maze_id();
            let maze = &mut self.mazes[maze_id];

            let visible_positions = maze.get_and_cache_visible_positions(
                minotaur.position(),
                minotaur.direction(),
                minotaur.view(),
            );

            let visible_heros = self
                .heros
                .values()
                .filter(|hero| {
                    !hero.is_dead()
                        && hero.maze_id() == maze_id
                        && visible_positions.contains(&hero.position())
                })
                .collect_vec();

            minotaur.update(maze, visible_heros);

            let catched_heros = self
                .heros
                .values()
                .filter(|hero| {
                    hero.maze_id() == maze_id
                        && hero.position() == minotaur.position()
                        && !hero.is_dead()
                })
                .map(|hero| hero.id())
                .collect_vec();

            for hero_id in catched_heros.iter() {
                if let Some(hero) = self.heros.get_mut(hero_id) {
                    if let HeroState::InMaze { instant } = hero.state {
                        hero.state = HeroState::Dead {
                            duration: instant.elapsed(),
                            instant: Instant::now(),
                        }
                    }
                }
            }

            minotaur.kills += catched_heros.len();
            self.top_minotaurs_map.insert(
                minotaur.id(),
                (
                    minotaur.name().to_string(),
                    minotaur.maze_id(),
                    minotaur.kills,
                ),
            );
            should_update_top_minotaurs = true;
        }

        if should_update_top_minotaurs {
            self.update_top_minotaurs();
        }
    }

    pub fn image_char_overrides(
        &self,
        player_id: PlayerId,
        image: &RgbaImage,
    ) -> AppResult<HashMap<(u32, u32), char>> {
        let hero = if let Some(hero) = self.get_hero(&player_id) {
            hero
        } else {
            return Err(anyhow!("Missing hero {player_id}"));
        };

        let maze = &self.mazes[hero.maze_id()];

        // Override empty positions.
        let visible_positions =
            maze.get_cached_visible_positions(hero.position(), hero.direction(), hero.view());
        let mut override_positions = visible_positions
            .iter()
            .filter(|(x, y)| {
                is_transparent(
                    image.get_pixel(*x as u32, *y as u32),
                    &Maze::background_color(),
                )
            })
            .map(|&(x, y)| ((x as u32, y as u32), '·'))
            .collect::<HashMap<(u32, u32), char>>();

        for &(x, y) in maze.entrance_positions().iter() {
            if !visible_positions.contains(&(x, y)) {
                continue;
            }

            if maze.id() > 0 {
                for (idx, c) in (maze.id() + 1 - 1).to_string().chars().enumerate() {
                    override_positions.insert((x as u32 + idx as u32 + 1, y as u32), c);
                }
                override_positions.insert((x as u32, y as u32), '←');
            }
        }

        for &(x, y) in maze.exit_positions().iter() {
            if !visible_positions.contains(&(x, y)) {
                continue;
            }

            for (idx, c) in (maze.id() + 1 + 1).to_string().chars().rev().enumerate() {
                override_positions.insert((x as u32 - idx as u32 - 1, y as u32), c);
            }
            override_positions.insert((x as u32, y as u32), '→');
        }

        Ok(override_positions)
    }

    pub fn draw(&self, player_id: PlayerId) -> AppResult<RgbaImage> {
        if let Some(hero) = self.heros.get(&player_id) {
            let (x, y) = hero.position();
            let maze_id = hero.maze_id();
            let maze = &self.mazes[maze_id];

            let maze_image = maze.image();

            // The base player image is black.
            let mut player_image =
                RgbaImage::from_pixel(maze_image.width(), maze_image.height(), Rgba([0; 4]));

            let visible_positions =
                maze.get_cached_visible_positions(hero.position(), hero.direction(), hero.view());

            for (&(dx, dy), instant) in hero.past_visible_positions().iter() {
                // Each position in the past_visible_positions is copied from the maze_image, with alpha channel depending on the time passed.
                let base_color = maze_image.get_pixel(dx as u32, dy as u32);

                let is_valid = maze.is_valid_position((dx, dy));

                let base_alpha = if is_valid { 0 } else { 125 };
                let mut alpha = if instant.elapsed() < hero.past_visibility_duration() {
                    base_alpha
                        - (base_alpha as f64 * instant.elapsed().as_millis() as f64
                            / hero.past_visibility_duration().as_millis() as f64)
                            as u8
                } else {
                    0
                };

                if visible_positions.contains(&(dx, dy)) {
                    // Each position in the visible_positions is copied from the maze_image, with alpha channel depending on the distance from the hero.
                    let distance = hero.position().distance((dx, dy));
                    alpha += ((255.0 - alpha as f64)
                        * (1.0 - distance / hero.view().radius() as f64))
                        as u8;
                }

                let pixel = Rgba([base_color[0], base_color[1], base_color[2], alpha]);
                player_image.put_pixel(dx as u32, dy as u32, pixel);
            }

            // Add powerup position
            for &(x, y) in maze.power_up_positions.iter() {
                if !hero.power_up_collected_at(maze_id, (x, y))
                    && visible_positions.contains(&(x, y))
                {
                    player_image.put_pixel(x as u32, y as u32, GameColors::POWER_UP);
                }
            }

            // Add other heros position
            for (p_id, any_hero) in self.heros.iter() {
                if *p_id != player_id && any_hero.maze_id() == hero.maze_id() {
                    let (ax, ay) = any_hero.position();
                    if visible_positions.contains(&(ax, ay)) {
                        player_image.put_pixel(ax as u32, ay as u32, GameColors::OTHER_HERO);
                    }
                }
            }

            // Add minotaurs position
            let maze_minotaurs = &self.minotaur_rooms[hero.maze_id()];
            for minotaur_id in maze_minotaurs.iter() {
                if let Some(minotaur) = self.get_minotaur(minotaur_id) {
                    let (mx, my) = minotaur.position();
                    if visible_positions.contains(&(mx, my)) {
                        if minotaur.is_chasing(hero.id()) {
                            player_image.put_pixel(
                                mx as u32,
                                my as u32,
                                GameColors::CHASING_MINOTAUR,
                            );
                        } else {
                            player_image.put_pixel(mx as u32, my as u32, GameColors::MINOTAUR);
                        }
                    }
                }
            }

            // Add hero position
            player_image.put_pixel(x as u32, y as u32, GameColors::HERO);

            return Ok(player_image);
        }
        Err(anyhow!("No hero with id {player_id}"))
    }

    pub fn handle_command(&mut self, command: &GameCommand, hero_id: PlayerId) {
        let hero = if let Some(hero) = self.heros.get_mut(&hero_id) {
            hero
        } else {
            return;
        };

        if hero.state == HeroState::WaitingToStart {
            hero.state = HeroState::InMaze {
                instant: Instant::now(),
            }
        }

        match hero.state {
            HeroState::WaitingToStart => unreachable!(),
            HeroState::InMaze { instant } => {
                let maze_id = hero.maze_id();
                match command {
                    GameCommand::Move { direction } => {
                        hero.update_past_visible_positions(
                            self.mazes[maze_id].get_and_cache_visible_positions(
                                hero.position(),
                                hero.direction(),
                                hero.view(),
                            ),
                        );

                        if *direction != hero.direction() {
                            hero.set_direction(*direction);
                        }

                        if !hero.can_move() {
                            hero.update_past_visible_positions(
                                self.mazes[maze_id].get_and_cache_visible_positions(
                                    hero.position(),
                                    hero.direction(),
                                    hero.view(),
                                ),
                            );
                            return;
                        }

                        let (new_x, new_y) = hero.position().into_direction(direction);

                        if !self.mazes[maze_id].is_valid_position((new_x, new_y)) {
                            hero.update_past_visible_positions(
                                self.mazes[maze_id].get_and_cache_visible_positions(
                                    hero.position(),
                                    hero.direction(),
                                    hero.view(),
                                ),
                            );
                            return;
                        }

                        hero.set_position((new_x, new_y));
                        for &position in self.mazes[maze_id].power_up_positions.iter() {
                            if position == hero.position()
                                && !hero.power_up_collected_at(hero.maze_id(), hero.position())
                            {
                                hero.apply_random_power_up_at_position(hero.position());
                            }
                        }

                        // Transition between rooms
                        if self.mazes[maze_id].is_entrance_position(hero.position()) && maze_id > 0
                        {
                            let to = maze_id - 1;
                            self.mazes[maze_id].decrease_attempted();
                            self.mazes[to].decrease_passed();
                            hero.set_maze_id(to);

                            // Move hero between rooms
                            self.hero_rooms[maze_id].retain(|id| *id != hero.id());
                            self.hero_rooms[to].push(hero.id());

                            // If hero acquired max vision in this maze, reduce it by one.
                            if hero.vision() == Hero::MAX_VISION {
                                hero.decrease_vision();
                            }

                            for (idx, entrance) in
                                self.mazes[maze_id].entrance_positions().iter().enumerate()
                            {
                                if hero.position() == *entrance {
                                    hero.set_position(self.mazes[to].exit_positions()[idx]);
                                    break;
                                }
                            }
                        } else if self.mazes[maze_id].is_exit_position(hero.position()) {
                            let to = maze_id + 1;
                            self.mazes[maze_id].increase_passed();

                            // Move hero between rooms
                            self.hero_rooms[maze_id].retain(|id| *id != hero.id());

                            if to == MAX_MAZE_ID {
                                hero.state = HeroState::Victory {
                                    duration: instant.elapsed(),
                                    instant: Instant::now(),
                                };
                            } else {
                                hero.set_maze_id(to);
                                self.hero_rooms[to].push(hero.id());
                                self.mazes[to].increase_attempted();

                                // If hero acquired max vision in this maze, reduce it by one.
                                if hero.vision() == Hero::MAX_VISION {
                                    hero.decrease_vision();
                                }

                                for (idx, exit) in
                                    self.mazes[maze_id].exit_positions().iter().enumerate()
                                {
                                    if hero.position() == *exit {
                                        hero.set_position(self.mazes[to].entrance_positions()[idx]);
                                        break;
                                    }
                                }
                            }
                        }

                        hero.update_past_visible_positions(
                            self.mazes[hero.maze_id()].get_and_cache_visible_positions(
                                hero.position(),
                                hero.direction(),
                                hero.view(),
                            ),
                        );
                    }

                    GameCommand::TurnClockwise => {
                        hero.update_past_visible_positions(
                            self.mazes[maze_id].get_and_cache_visible_positions(
                                hero.position(),
                                hero.direction(),
                                hero.view(),
                            ),
                        );
                        hero.set_direction(hero.direction().rotate_clockwise());
                        hero.update_past_visible_positions(
                            self.mazes[maze_id].get_and_cache_visible_positions(
                                hero.position(),
                                hero.direction(),
                                hero.view(),
                            ),
                        );
                    }

                    GameCommand::TurnCounterClockwise => {
                        hero.update_past_visible_positions(
                            self.mazes[maze_id].get_and_cache_visible_positions(
                                hero.position(),
                                hero.direction(),
                                hero.view(),
                            ),
                        );
                        hero.set_direction(hero.direction().rotate_counter_clockwise());
                        hero.update_past_visible_positions(
                            self.mazes[maze_id].get_and_cache_visible_positions(
                                hero.position(),
                                hero.direction(),
                                hero.view(),
                            ),
                        );
                    }

                    GameCommand::CycleUiOptions => hero.cycle_ui_options(),
                }
            }
            _ => {}
        }

        self.update_hero_record(hero_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{Game, MAX_MAZE_ID};
    use crate::{game::utils::to_player_name, AppResult, PlayerId};
    use rand::RngExt;
    use std::time::Duration;

    #[test]
    fn test_top_heros() -> AppResult<()> {
        let mut game = Game::new()?;

        let rng = &mut rand::rng();

        for _ in 0..100 {
            game.top_heros_map.insert(
                PlayerId::new_v4(),
                (
                    to_player_name(rng, "name"),
                    rng.random_range(0..=MAX_MAZE_ID),
                    Duration::from_millis(rng.random_range(15000..150000)),
                ),
            );
        }

        game.update_top_heros();
        for index in 0..game.top_heros.len() {
            let (_, _, maze_id, timer) = game.top_heros[index];
            let (_, _, next_maze_id, next_timer) = game.top_heros[index];

            assert!(maze_id > next_maze_id || timer <= next_timer);
        }

        Ok(())
    }
}
