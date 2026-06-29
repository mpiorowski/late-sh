use crate::{
    direction::Direction, game::POWER_UPS_PER_ROOM, minotaur::Minotaur, utils::convert_rgb_to_rgba,
    Entity, IntoDirection, Position, View, MAX_MAZE_ID,
};
use anyhow::Result as AppResult;
use image::{Rgb, Rgba, RgbaImage};
use itertools::Itertools;
use knossos::maze::{self, GrowingTree, Method};
use rand::{
    seq::{IndexedRandom, IteratorRandom},
    RngExt, SeedableRng,
};
use rand_chacha::ChaCha8Rng;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct Maze {
    id: usize,
    random_seed: u64,
    rng: ChaCha8Rng,
    width: usize,
    height: usize,
    wall_size: usize,
    passage_size: usize,
    image: RgbaImage,
    valid_positions: HashSet<Position>,
    entrance: Vec<Position>,
    exit: Vec<Position>,
    pub power_up_positions: Vec<Position>,
    visible_positions_cache: HashMap<(Position, Direction, View), HashSet<Position>>, // (x, y, direction, type) -> visible positions
    success_rate: (usize, usize), //pass/attempted
}

impl Maze {
    const DEFAULT_WALL_SIZE: usize = 2;
    const DEFAULT_PASSAGE_SIZE: usize = 2;
    const MARGIN_SIZE: usize = 0;

    fn insert_valid_position(&mut self, position: Position) {
        self.valid_positions.insert(position);

        let (x, y) = position;
        self.image
            .put_pixel(x as u32, y as u32, Self::background_color());
    }

    fn build_entrance(&mut self) {
        let rng = &mut self.rng;

        let entrance_y = rng.random_range(
            Self::MARGIN_SIZE + self.wall_size
                ..self.image.height() as usize - Self::MARGIN_SIZE - self.wall_size - 1,
        ) / 2
            * 2;
        self.entrance = {
            let starting_x = if self.id == 0 {
                Self::MARGIN_SIZE + self.wall_size
            } else {
                0
            };
            let mut x = starting_x;
            loop {
                if self.is_valid_position((x, entrance_y))
                    && self.is_valid_position((x, entrance_y + 1))
                {
                    break;
                }

                self.insert_valid_position((x, entrance_y));
                self.insert_valid_position((x, entrance_y + 1));

                x += 1;
            }

            vec![(starting_x, entrance_y), (starting_x, entrance_y + 1)]
        };
    }

    fn build_exit(&mut self) {
        let rng = &mut self.rng;

        let exit_y = rng.random_range(
            Self::MARGIN_SIZE + self.wall_size
                ..self.image.height() as usize - Self::MARGIN_SIZE - self.wall_size - 1,
        ) / 2
            * 2;
        self.exit = {
            let max_x = self.image.width() as usize - Self::MARGIN_SIZE - 1;
            let mut x = max_x;

            loop {
                if self.is_valid_position((x, exit_y)) && self.is_valid_position((x, exit_y + 1)) {
                    break;
                }

                self.insert_valid_position((x, exit_y));
                self.insert_valid_position((x, exit_y + 1));
                x -= 1;
            }

            vec![(max_x, exit_y), (max_x, exit_y + 1)]
        };
    }

    fn build_extra_rooms(&mut self) {
        let rng = &mut self.rng;
        // Add random rooms. The number of rooms deoends on the maze size.
        let number_of_rooms = rng.random_range(4..=((self.width + self.height) / 2).max(5));
        let mut new_valid_positions = Vec::new();
        for _ in 0..number_of_rooms {
            let room_width = rng.random_range(4..=((self.width + self.height) / 6).max(5));
            let room_height = rng.random_range(4..=((self.width + self.height) / 6).max(5));

            let room_x = rng.random_range(
                Self::MARGIN_SIZE + self.wall_size
                    ..self.image.width() as usize - room_width - Self::MARGIN_SIZE - self.wall_size,
            );
            let room_y = rng.random_range(
                Self::MARGIN_SIZE + self.wall_size
                    ..self.image.height() as usize
                        - room_height
                        - Self::MARGIN_SIZE
                        - self.wall_size,
            );

            for y in room_y..room_y + room_height {
                for x in room_x..room_x + room_width {
                    new_valid_positions.push((x, y));
                }
            }
        }

        for &position in new_valid_positions.iter() {
            self.insert_valid_position(position);
        }
    }

    fn random_valid_position(&self) -> Position {
        self.valid_positions
            .iter()
            .choose(&mut rand::rng())
            .copied()
            .unwrap()
    }

    fn set_power_ups_position(&mut self, amount: usize) {
        self.power_up_positions = self
            .valid_positions
            .iter()
            .filter(|&&position| {
                self.entrance
                    .iter()
                    .all(|entrance| entrance.distance(position) > 6.0)
                    && self.exit.iter().all(|exit| exit.distance(position) > 6.0)
            })
            .sample(&mut rand::rng(), amount)
            .into_iter()
            .copied()
            .collect_vec();
    }

    fn color(id: usize) -> Rgba<u8> {
        let a = (id.min(MAX_MAZE_ID) as f64) / MAX_MAZE_ID as f64;
        // red = Rgba([208, 28, 28, 125]);
        // whiteblueish = Rgba([210, 240, 255, 125]);

        Rgba([
            (a * 208.0 + (1.0 - a) * 210.0) as u8,
            (a * 28.0 + (1.0 - a) * 240.0) as u8,
            (a * 28.0 + (1.0 - a) * 255.0) as u8,
            125,
        ])
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn background_color() -> Rgba<u8> {
        Rgba([0; 4])
    }

    /// Sets a maze width and returns itself
    pub const fn width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Sets a maze height and returns itself
    pub const fn height(mut self, height: usize) -> Self {
        self.height = height;
        self
    }

    /// Sets a maze rng and returns itself
    pub fn random_seed(mut self, random_seed: u64) -> Self {
        self.rng = ChaCha8Rng::seed_from_u64(random_seed);
        self
    }

    /// Sets a maze wall_size and returns itself
    pub const fn wall_size(mut self, wall_size: usize) -> Self {
        self.wall_size = wall_size;
        self
    }

    /// Sets a maze passage_size and returns itself
    pub const fn passage_size(mut self, passage_size: usize) -> Self {
        self.passage_size = passage_size;
        self
    }

    pub fn new(id: usize) -> Self {
        // SeedableRng::from_os_rng was removed in rand 0.10; seed the maze RNG
        // from the thread RNG instead, which itself seeds from the OS.
        let mut rng = ChaCha8Rng::from_rng(&mut rand::rng());
        let random_seed = rng.random();

        log::debug!("new maze seed {random_seed}");
        Self {
            id,
            random_seed,
            rng,
            width: 0,
            height: 0,
            wall_size: Self::DEFAULT_WALL_SIZE,
            passage_size: Self::DEFAULT_PASSAGE_SIZE,
            image: RgbaImage::new(0, 0),
            valid_positions: HashSet::new(),
            entrance: Vec::new(),
            exit: Vec::new(),
            power_up_positions: Vec::new(),
            visible_positions_cache: HashMap::new(),
            success_rate: (0, 0),
        }
    }

    pub fn build(mut self) -> AppResult<Self> {
        if self.width == 0 {
            self.width = self
                .rng
                .random_range(16 + 2 * (self.id / 4)..=(20 + 2 * (self.id / 2)).min(32));
        }

        if self.height == 0 {
            self.height = self
                .rng
                .random_range(4 + 2 * (self.id / 4)..=(6 + 2 * (self.id / 2)).min(20));
        }

        let Rgba([r, g, b, _]) = Self::color(self.id);
        let Rgba([br, bg, bb, _]) = Self::background_color();

        let knossos_maze = maze::OrthogonalMazeBuilder::new()
            .width(self.width)
            .height(self.height)
            .algorithm(Box::new(GrowingTree::new(Method::Newest75Random25)))
            .seed(Some(self.random_seed))
            .build();

        let maze_image_wrapper = knossos_maze.format(
            maze::Image::new()
                .wall(self.wall_size)
                .passage(self.passage_size)
                .margin(Self::MARGIN_SIZE)
                .background(knossos::Color::RGB(br, bg, bb))
                .foreground(knossos::Color::RGB(r, g, b)),
        );

        self.image = convert_rgb_to_rgba(&maze_image_wrapper.into_inner(), Rgb([0; 3]));

        self.valid_positions = self
            .image
            .enumerate_pixels()
            .filter(|(_, _, pixel)| pixel[3] == 0)
            .map(|(x, y, _)| (x as usize, y as usize))
            .collect();

        self.build_entrance();
        self.build_exit();
        self.build_extra_rooms();
        self.set_power_ups_position(POWER_UPS_PER_ROOM);

        Ok(self)
    }

    pub fn spawn_minotaur(&mut self, name: String) -> Minotaur {
        let mut position = self.random_valid_position();
        while !self.is_valid_minotaur_position(position) {
            position = self.random_valid_position()
        }

        let speed = (self.id as u64 / 3).min(6);
        let aggression = (0.5 + 0.1 * (self.id / 2) as f64).min(1.0);
        let vision = (4 + self.id / 3).min(7);
        let minotaur = Minotaur::new(name, self.id, position, speed, vision, aggression);
        self.get_and_cache_visible_positions(position, minotaur.direction(), minotaur.view());

        minotaur
    }

    pub fn get_and_cache_visible_positions(
        &mut self,
        position: Position,
        direction: Direction,
        view: View,
    ) -> HashSet<Position> {
        let cache_key = (position, direction, view);
        if let Some(visible_positions) = self.visible_positions_cache.get(&cache_key) {
            return visible_positions.clone();
        }

        if view == View::Full {
            let mut visible_positions = HashSet::new();
            for &(x, y) in self.valid_positions.iter() {
                visible_positions.insert((x, y));
            }

            self.visible_positions_cache
                .insert(cache_key, visible_positions.clone());
            return visible_positions;
        }

        let (x, y) = position;
        let view_radius = view.radius();

        let mut visible_positions = HashSet::new();
        for dy in
            y.saturating_sub(view_radius)..=(y + view_radius).min(self.image().height() as usize)
        {
            for dx in
                x.saturating_sub(view_radius)..=(x + view_radius).min(self.image().width() as usize)
            {
                // Origin is always visible
                if x == dx && y == dy {
                    visible_positions.insert((dx, dy));
                    continue;
                }

                if visible_positions.contains(&(dx, dy)) {
                    continue;
                }

                // Position must be unobstructed by walls.
                // We check this by drawing a line from the position to (x, y) and check that the positions on the line are valid positions.

                // vertical line
                if x == dx {
                    let iter = if y < dy {
                        (y..=dy).collect_vec()
                    } else {
                        (dy..=y).rev().collect_vec()
                    };

                    'inner: for ly in iter {
                        // The wall visible as well.
                        visible_positions.insert((x, ly));
                        if !self.is_valid_position((x, ly)) {
                            break 'inner;
                        }
                    }
                }
                //horizontal line
                else if y == dy {
                    let iter = if x < dx {
                        (x..=dx).collect_vec()
                    } else {
                        (dx..=x).rev().collect_vec()
                    };

                    'inner: for lx in iter {
                        // The wall visible as well.
                        visible_positions.insert((lx, y));
                        if !self.is_valid_position((lx, y)) {
                            break 'inner;
                        }
                    }
                }
                // generic line
                else {
                    let mut line = bresenham_line((x as i32, y as i32), (dx as i32, dy as i32));

                    if line[0] != (x, y) {
                        line.reverse();
                    };

                    'inner: for index in 0..line.len() {
                        let (lx, ly) = line[index];
                        // The wall visible as well.
                        visible_positions.insert((lx, ly));
                        if !self.is_valid_position((lx, ly)) {
                            break 'inner;
                        }

                        if index < line.len() - 1 {
                            // Check if we are moving through a wall in a diagonal
                            let (next_x, next_y) = line[index + 1];

                            // 4 cases
                            if next_x == lx + 1
                                && next_y + 1 == ly
                                && self.is_valid_position((next_x, next_y))
                                && !self.is_valid_position((lx + 1, ly))
                                && !self.is_valid_position((lx, ly - 1))
                            {
                                break 'inner;
                            }

                            if next_x == lx + 1
                                && next_y == ly + 1
                                && self.is_valid_position((next_x, next_y))
                                && !self.is_valid_position((lx + 1, ly))
                                && !self.is_valid_position((lx, ly + 1))
                            {
                                break 'inner;
                            }

                            if next_x + 1 == lx
                                && next_y + 1 == ly
                                && self.is_valid_position((next_x, next_y))
                                && !self.is_valid_position((lx - 1, ly))
                                && !self.is_valid_position((lx, ly - 1))
                            {
                                break 'inner;
                            }

                            if next_x + 1 == lx
                                && next_y == ly + 1
                                && self.is_valid_position((next_x, next_y))
                                && !self.is_valid_position((lx - 1, ly))
                                && !self.is_valid_position((lx, ly + 1))
                            {
                                break 'inner;
                            }
                        }
                    }
                }
            }
        }

        // Filter out-of-bounds positions.
        visible_positions = visible_positions
            .iter()
            .filter(|(x, y)| {
                *x < self.image().width() as usize && *y < self.image().height() as usize
            })
            .map(|(x, y)| (*x, *y))
            .collect();

        // Limit view to relevant cone depending on the direction.
        visible_positions = visible_positions
            .iter()
            .filter(|(x, y)| {
                let dx = *x as i32 - position.0 as i32;
                let dy = *y as i32 - position.1 as i32;
                match view {
                    View::Cone { .. } => match direction {
                        Direction::North => dx >= dy && dx <= -dy,
                        Direction::East => dy <= dx && dy >= -dx,
                        Direction::South => dx <= dy && dx >= -dy,
                        Direction::West => dy >= dx && dy <= -dx,
                        Direction::NorthEast => dx >= 0 && dy <= 0,
                        Direction::SouthEast => dx >= 0 && dy >= 0,
                        Direction::SouthWest => dx <= 0 && dy >= 0,
                        Direction::NorthWest => dx <= 0 && dy <= 0,
                    },
                    View::Plane { .. } => match direction {
                        Direction::North => dy < 0,
                        Direction::East => dx > 0,
                        Direction::South => dy > 0,
                        Direction::West => dx < 0,
                        Direction::NorthEast => dx > dy,
                        Direction::SouthEast => dx > -dy,
                        Direction::SouthWest => dx < dy,
                        Direction::NorthWest => dx < -dy,
                    },
                    View::Circle { .. } => true,
                    _ => unreachable!(),
                }
            })
            .map(|(x, y)| (*x, *y))
            .collect();

        self.visible_positions_cache
            .insert(cache_key, visible_positions.clone());

        visible_positions
    }

    pub fn get_cached_visible_positions(
        &self,
        position: Position,
        direction: Direction,
        view: View,
    ) -> HashSet<Position> {
        let cache_key = (position, direction, view);
        self.visible_positions_cache
            .get(&cache_key)
            .expect("Visible positions should have been cached")
            .clone()
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    pub fn save_image(&self, name: &str) -> AppResult<()> {
        self.image.save(name)?;
        Ok(())
    }

    pub fn is_valid_position(&self, position: Position) -> bool {
        self.valid_positions.contains(&position)
    }

    pub fn is_valid_minotaur_position(&self, position: Position) -> bool {
        let entrances = self.entrance_positions();
        self.valid_positions.contains(&position)
            && entrances.iter().all(|p| p.distance(position) > 6.0)
    }

    pub fn is_entrance_position(&self, position: Position) -> bool {
        self.entrance.contains(&position)
    }

    pub fn is_exit_position(&self, position: Position) -> bool {
        self.exit.contains(&position)
    }

    pub fn entrance_positions(&self) -> &Vec<Position> {
        &self.entrance
    }

    pub fn exit_positions(&self) -> &Vec<Position> {
        &self.exit
    }

    pub fn hero_starting_position(&self) -> Position {
        let rng = &mut rand::rng();
        *self.entrance.choose(rng).unwrap()
    }

    pub fn increase_attempted(&mut self) {
        self.success_rate.1 += 1;
    }

    pub fn decrease_attempted(&mut self) {
        self.success_rate.1 -= 1;
    }

    pub fn increase_passed(&mut self) {
        self.success_rate.0 += 1;
    }

    pub fn decrease_passed(&mut self) {
        self.success_rate.0 -= 1;
    }

    pub fn success_rate(&self) -> f64 {
        self.success_rate.0 as f64 / self.success_rate.1 as f64
    }
}

// Returns the list of points from (x0, y0) to (x1, y1)
fn bresenham_line(from: (i32, i32), to: (i32, i32)) -> Vec<Position> {
    let mut result = Vec::new();

    let (mut x0, mut y0) = from;
    let (mut x1, mut y1) = to;

    let steep = (y1 - y0).abs() > (x1 - x0).abs();
    if steep {
        (x0, y0) = (y0, x0);
        (x1, y1) = (y1, x1);
    }
    if x0 > x1 {
        (x0, x1) = (x1, x0);
        (y0, y1) = (y1, y0);
    }

    let delta_x = x1 - x0;
    let delta_y = (y1 - y0).abs();
    let mut error = 0;
    let ystep = if y0 < y1 { 1 } else { -1 };
    let mut y = y0;

    for x in x0..=x1 {
        if steep {
            result.push((y as usize, x as usize))
        } else {
            result.push((x as usize, y as usize))
        }
        error += delta_y;
        if 2 * error >= delta_x {
            y += ystep;
            error -= delta_x;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::Maze;
    use crate::{game::MAX_MAZE_ID, AppResult};

    #[test]
    fn test_random_mazes_image() -> AppResult<()> {
        for id in 0..MAX_MAZE_ID {
            let maze = Maze::new(id);
            let name = format!("images/random_{}.png", id);
            maze.save_image(&name)?;
        }

        Ok(())
    }
}
