use super::engine::{goalie::Goalie, player::Player, puck::Puck};
use crate::{
    collision_detection::{are_colliding, inelastic_collision},
    constants::*,
    engine::area::Area,
    traits::{Body, ColliderType, Entity, Sprite},
    types::*,
    utils::*,
};
use glam::{U16Vec2, Vec2};
use image::RgbaImage;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
pub enum GameState {
    Starting {
        time: Instant,
    },
    Running,
    AfterGoal {
        time: Instant,
        scored: GameSide,
    },
    Ending {
        time: Instant,
        winner: Option<GameSide>,
        by_disconnect: bool,
    },
}

#[derive(Debug, Default)]
pub struct GameData {
    pub player: Player,
    pub goalie: Goalie,
    pub area: Area,
    pub score: u8,
}

impl GameData {
    pub fn new(side: GameSide) -> Self {
        Self {
            player: Player::new(side),
            goalie: Goalie::new(side),
            area: Area::new(side),
            score: 0,
        }
    }

    pub fn reset(&mut self) {
        self.player.reset();
    }

    pub fn handle_command(&mut self, puck: &mut Puck, cmd: GameCommand) {
        let player = &mut self.player;
        if player.shooting_state.is_shooting() {
            let shooting_modifier = match cmd {
                GameCommand::Up => Vec2::NEG_Y * SHOOTING_DIRECTION_MODIFIER,
                GameCommand::Down => Vec2::Y * SHOOTING_DIRECTION_MODIFIER,
                GameCommand::Left => Vec2::NEG_X * SHOOTING_DIRECTION_MODIFIER,
                GameCommand::Right => Vec2::X * SHOOTING_DIRECTION_MODIFIER,
                GameCommand::Shoot => Vec2::ZERO,
            };
            let current = player.shooting_state.direction.unwrap_or(player.velocity);
            player.shooting_state.direction = Some(
                (current + shooting_modifier).clamp_length_max(SHOOTING_DIRECTION_MAX_MAGNITUDE),
            );
            return;
        }

        if cmd == GameCommand::Shoot && player.after_shooting_counter == 0.0 {
            if puck.possession == Some(player.side) {
                player.velocity *= SHOOTING_VELOCITY_DAMPING;
                puck.velocity *= SHOOTING_VELOCITY_DAMPING;
                player.new_orientation = Some(player.orientation.previous());
                player
                    .shooting_state
                    .shoot(player.orientation.shooting_direction());
            }
            return;
        }

        let natural_orientation = match cmd {
            GameCommand::Up => {
                apply_axis_input(&mut player.velocity.y, -1.0);
                Orientation::UpLeft
            }
            GameCommand::Down => {
                apply_axis_input(&mut player.velocity.y, 1.0);
                Orientation::DownRight
            }
            GameCommand::Left => {
                apply_axis_input(&mut player.velocity.x, -1.0);
                Orientation::DownLeft
            }
            GameCommand::Right => {
                apply_axis_input(&mut player.velocity.x, 1.0);
                Orientation::UpRight
            }
            GameCommand::Shoot => player.orientation,
        };

        if player.velocity.length() > 0.0 && player.orientation != natural_orientation {
            let diff = (natural_orientation as isize - player.orientation as isize + 8) % 8;
            player.new_orientation = Some(if diff > 4 {
                player.orientation.previous()
            } else {
                player.orientation.next()
            });
        }
    }
}

fn apply_axis_input(axis: &mut f32, direction: f32) {
    let opposing = axis.signum() != direction.signum() && *axis != 0.0;
    let delta = if opposing { DECELERATION } else { ACCELERATION };
    *axis += direction * delta;
}

pub struct Game {
    pub id: uuid::Uuid,
    pub red_data: GameData,
    pub blue_data: GameData,
    pub puck: Puck,
    pub skate_traces: VecDeque<U16Vec2>,
    pub timer: u128,
    pub last_tick: Instant,
    pub state: GameState,
    pub palette: Palette,
    /// Solo practice: the local player controls Red; Blue has no human
    /// player and Blue's goalie wanders at random.
    pub practice_mode: bool,
}

impl Game {
    pub const DURATION_MILLISECONDS: u128 = 90 * 1000;
    pub const STARTING_DELAY_MILLISECONDS: u64 = 3000;
    const AFTER_GOAL_DELAY_MILLISECONDS: u128 = 2000;

    pub fn new() -> Self {
        Self::with_practice(false)
    }

    pub fn new_practice() -> Self {
        Self::with_practice(true)
    }

    fn with_practice(practice_mode: bool) -> Self {
        Self {
            red_data: GameData::new(GameSide::Red),
            blue_data: GameData::new(GameSide::Blue),
            puck: Puck::new(),
            skate_traces: VecDeque::new(),
            id: uuid::Uuid::new_v4(),
            timer: 0,
            last_tick: Instant::now(),
            state: GameState::Starting {
                time: Instant::now(),
            },
            palette: Palette::default(),
            practice_mode,
        }
    }

    fn data_mut(&mut self, side: GameSide) -> &mut GameData {
        match side {
            GameSide::Red => &mut self.red_data,
            GameSide::Blue => &mut self.blue_data,
        }
    }

    fn reset_after_goal(&mut self) {
        self.red_data.reset();
        self.blue_data.reset();
        self.puck = Puck::new();
        self.state = GameState::Starting {
            time: Instant::now(),
        };
        self.skate_traces.clear();
    }

    fn update_running(&mut self, deltatime: f32) -> AppResult<()> {
        for player in [&mut self.red_data.player, &mut self.blue_data.player] {
            player.update(deltatime);
            player.maybe_bounce_against_rect(PITCH_INNER_RECT, COFFICIENT_OF_WALL_BOUNCING);

            if are_colliding(player, &self.red_data.area).is_some() {
                inelastic_collision(player, &mut self.red_data.area, AREA_RESTITUTION);
            } else if are_colliding(player, &self.blue_data.area).is_some() {
                inelastic_collision(player, &mut self.blue_data.area, AREA_RESTITUTION);
            }
        }

        if !self.practice_mode {
            if let Some((a, b)) = are_colliding(&self.red_data.player, &self.blue_data.player) {
                if !matches!((a, b), (ColliderType::Catcher, ColliderType::Catcher)) {
                    // inelastic_collision resets both players to previous_position()
                    // (U16Vec2), discarding sub-pixel separation, so we read the
                    // floats now to keep a direction for the impulse normal.
                    let red_pre = self.red_data.player.position_float();
                    let blue_pre = self.blue_data.player.position_float();

                    inelastic_collision(
                        &mut self.red_data.player,
                        &mut self.blue_data.player,
                        PLAYER_PLAYER_RESTITUTION,
                    );
                    if are_colliding(&self.red_data.player, &self.blue_data.player).is_some() {
                        let mut normal = (blue_pre - red_pre).normalize_or_zero();
                        if normal == Vec2::ZERO {
                            // Float positions also coincided; pick any axis so
                            // they still separate.
                            normal = Vec2::X;
                            log::warn!(
                                "player-player overlap with coincident float positions: red={red_pre} blue={blue_pre}; using fallback normal"
                            );
                        }
                        self.red_data.player.velocity -= normal * PLAYER_SEPARATION_IMPULSE;
                        self.blue_data.player.velocity += normal * PLAYER_SEPARATION_IMPULSE;

                        if are_colliding(&self.red_data.player, &self.blue_data.player).is_some() {
                            log::warn!(
                                "player-player STILL colliding after impulse: \
                                 red_pre={red_pre} blue_pre={blue_pre} \
                                 v_red={} v_blue={} normal={normal} \
                                 colliders=({a:?},{b:?})",
                                self.red_data.player.velocity,
                                self.blue_data.player.velocity,
                            );
                        }
                    }
                }
            }
        }

        for player in [&mut self.red_data.player, &mut self.blue_data.player] {
            if let Some(new_orientation) = player.new_orientation {
                player.rotate(new_orientation);
                if are_colliding(player, &self.red_data.area).is_some()
                    || are_colliding(player, &self.blue_data.area).is_some()
                {
                    player.undo_rotation();
                }
            }
        }
        if !self.practice_mode
            && are_colliding(&self.red_data.player, &self.blue_data.player).is_some()
        {
            self.red_data.player.undo_rotation();
            self.blue_data.player.undo_rotation();
        }

        for player in [&self.red_data.player, &self.blue_data.player] {
            if player.position() != player.previous_position() {
                let head_position = player.position() + player.head_position_offset();
                self.skate_traces.push_back(head_position);
            }
        }
        while self.skate_traces.len() > SKATE_TRACE_LENGTH {
            self.skate_traces.pop_front();
        }

        self.red_data.goalie.align_to_player(&self.red_data.player);
        if self.practice_mode {
            self.blue_data.goalie.random_walk(deltatime);
        } else {
            self.blue_data
                .goalie
                .align_to_player(&self.blue_data.player);
        }

        self.puck.update(deltatime);

        let contact_sides: &[GameSide] = if self.practice_mode {
            &[GameSide::Red]
        } else {
            &[GameSide::Red, GameSide::Blue]
        };
        for &side in contact_sides {
            self.handle_puck_player_contact(side);
        }

        if let Some(side) = self.puck.possession {
            let (player, other) = if side == GameSide::Red {
                (&mut self.red_data.player, &mut self.blue_data.player)
            } else {
                (&mut self.blue_data.player, &mut self.red_data.player)
            };

            if let Some(direction) = player.shooting_state.shot_towards(deltatime) {
                player.after_shooting_counter = AFTER_SHOOTING_COUNTER_MILLISECONDS;
                player.new_orientation = Some(player.orientation.next());
                self.puck.possession = None;
                self.puck.velocity = direction * SHOOTING_POWER;
            } else {
                self.puck.attach_to_player(player);
            }

            if other.shooting_state.is_shooting() {
                other.shooting_state.reset();
            }
        }

        for goalie in [&mut self.red_data.goalie, &mut self.blue_data.goalie] {
            let colliding = are_colliding(&self.puck, goalie).is_some();
            if colliding {
                inelastic_collision(&mut self.puck, goalie, GOALIE_RESTITUTION);
            }
            goalie.register_puck_contact(colliding, self.puck.possession.is_none());
        }

        if let Some(scored) = self.puck.has_scored() {
            self.data_mut(scored).score += 1;
            self.state = GameState::AfterGoal {
                time: Instant::now(),
                scored,
            };
        }

        Ok(())
    }

    fn handle_puck_player_contact(&mut self, side: GameSide) {
        let (own, opp) = match side {
            GameSide::Red => (&mut self.red_data, &mut self.blue_data),
            GameSide::Blue => (&mut self.blue_data, &mut self.red_data),
        };

        // After shooting, briefly refuse to re-grab the puck so it has time to
        // fly out of our own catcher area.
        let just_shot = own.player.after_shooting_counter > 0.0;

        let collision = are_colliding(&self.puck, &own.player)
            .or_else(|| swept_rotation_catch(&self.puck, &own.player));
        match collision {
            // Catcher: grab a free puck OR steal from the opponent (with cooldown).
            Some((ColliderType::Puck, ColliderType::Catcher)) if !just_shot => {
                match self.puck.possession {
                    Some(_owner) if _owner == side.opposite() => {
                        if own.player.after_got_stolen_counter == 0.0 {
                            self.puck.possession = Some(side);
                            opp.player.after_got_stolen_counter =
                                AFTER_GOT_STOLEN_COUNTER_MILLISECONDS;
                            self.puck.attach_to_player(&own.player);
                        }
                    }
                    None => {
                        self.puck.possession = Some(side);
                        self.puck.attach_to_player(&own.player);
                    }
                    _ => {}
                }
            }
            // Stick: grab a free puck (no stealing).
            Some((ColliderType::Puck, ColliderType::Stick))
                if !just_shot && self.puck.possession.is_none() =>
            {
                self.puck.possession = Some(side);
                self.puck.attach_to_player(&own.player);
            }
            // Body: bounce when the puck is free; otherwise the owner's attach loop handles it.
            Some((ColliderType::Puck, ColliderType::Player)) if self.puck.possession.is_none() => {
                inelastic_collision(&mut self.puck, &mut own.player, PUCK_RESTITUTION);
            }
            _ => {}
        }
    }

    pub fn handle_command(&mut self, side: GameSide, cmd: GameCommand) {
        let data = match side {
            GameSide::Red => &mut self.red_data,
            GameSide::Blue => &mut self.blue_data,
        };
        data.handle_command(&mut self.puck, cmd);
    }

    fn compute_winner(&self) -> Option<GameSide> {
        match self.red_data.score.cmp(&self.blue_data.score) {
            std::cmp::Ordering::Greater => Some(GameSide::Red),
            std::cmp::Ordering::Less => Some(GameSide::Blue),
            std::cmp::Ordering::Equal => None,
        }
    }

    pub fn end_with_winner(&mut self, winner: Option<GameSide>, by_disconnect: bool) {
        self.state = GameState::Ending {
            time: Instant::now(),
            winner,
            by_disconnect,
        };
    }

    pub fn winner(&self) -> Option<GameSide> {
        match self.state {
            GameState::Ending { winner, .. } => winner,
            _ => None,
        }
    }

    pub fn update(&mut self) -> AppResult<()> {
        let now = Instant::now();
        let deltatime = now.duration_since(self.last_tick).as_millis() as f32;

        match self.state {
            GameState::Starting { time } => {
                if time.elapsed() >= Duration::from_millis(Self::STARTING_DELAY_MILLISECONDS) {
                    self.state = GameState::Running;
                }
            }
            GameState::Running => {
                self.update_running(deltatime)?;
                if !self.practice_mode {
                    self.timer += deltatime as u128;
                    if self.timer > Self::DURATION_MILLISECONDS {
                        self.end_with_winner(self.compute_winner(), false);
                    }
                }
            }
            GameState::AfterGoal { time, .. } => {
                if now.duration_since(time).as_millis() >= Self::AFTER_GOAL_DELAY_MILLISECONDS {
                    self.reset_after_goal();
                }
            }
            GameState::Ending { .. } => {}
        }
        self.last_tick = now;

        Ok(())
    }

    pub fn draw(&self) -> AppResult<RgbaImage> {
        let mut img = PITCH_IMAGES
            .get(&self.palette)
            .expect("Pitch image should exist")
            .clone();
        self.composite_dynamic(&mut img)?;
        Ok(img)
    }

    fn composite_dynamic(&self, img: &mut RgbaImage) -> AppResult<()> {
        for trace in &self.skate_traces {
            img.put_pixel(
                trace.x as u32,
                trace.y as u32,
                self.palette.skate_trace_color(),
            );
        }
        let palette = self.palette;
        for (sprite, pos) in self.visible_sprites().into_iter().flatten() {
            img.copy_non_trasparent_from(sprite.image(palette), pos.x as u32, pos.y as u32)?;
        }
        Ok(())
    }

    /// Fixed-size stack array of (sprite, position) for everything that should
    /// be drawn this tick. The Blue player slot is `None` in practice mode.
    fn visible_sprites(&self) -> [Option<(&dyn Sprite, U16Vec2)>; 5] {
        [
            Some((
                &self.red_data.player as &dyn Sprite,
                self.red_data.player.position(),
            )),
            Some((
                &self.red_data.goalie as &dyn Sprite,
                self.red_data.goalie.position(),
            )),
            if self.practice_mode {
                None
            } else {
                Some((
                    &self.blue_data.player as &dyn Sprite,
                    self.blue_data.player.position(),
                ))
            },
            Some((
                &self.blue_data.goalie as &dyn Sprite,
                self.blue_data.goalie.position(),
            )),
            Some((&self.puck as &dyn Sprite, self.puck.position())),
        ]
    }
}

/// Detect a catch that happened while the player was rotating this tick by
/// re-running the granular pixel check against the previous orientation's
/// stick/catcher hit_box. Approximates the swept arc with two snapshots, which
/// closes the worst gap (stick passes over puck mid-rotation) without
/// computing the full arc.
fn swept_rotation_catch(puck: &Puck, player: &Player) -> Option<(ColliderType, ColliderType)> {
    if !player.just_rotated() {
        return None;
    }
    let prev_hit_box = player.previous_hit_box();
    let player_pos = player.position();
    let puck_pos = puck.position();
    for (&puck_point, &_puck_collider) in puck.hit_box().iter() {
        let world = puck_pos + puck_point;
        if world.x < player_pos.x || world.y < player_pos.y {
            continue;
        }
        let local = world - player_pos;
        if let Some(&player_collider) = prev_hit_box.get(&local) {
            if matches!(player_collider, ColliderType::Stick | ColliderType::Catcher) {
                return Some((ColliderType::Puck, player_collider));
            }
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::engine::goalie;
    use crate::traits::ColliderType;
    use core::time;
    use glam::{I16Vec2, U16Vec2};
    use image::{Pixel, Rgba};
    use log::LevelFilter;
    use log4rs::append::file::FileAppender;
    use log4rs::config::{Appender, Root};
    use log4rs::encode::pattern::PatternEncoder;
    use log4rs::Config;
    use ratatui::backend::CrosstermBackend;
    use ratatui::layout::{Constraint, Layout};
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;
    use ratatui::Terminal;

    fn img_to_lines<'a>(img: &RgbaImage) -> Vec<Line<'a>> {
        let mut lines: Vec<Line> = vec![];
        let width = img.width();
        let height = img.height();
        for y in (0..height - 1).step_by(2) {
            let mut line: Vec<Span> = vec![];
            for x in 0..width {
                let top = img.get_pixel(x, y).to_rgba();
                let btm = img.get_pixel(x, y + 1).to_rgba();
                if top[3] == 0 && btm[3] == 0 {
                    line.push(Span::raw(" "));
                } else if top[3] > 0 && btm[3] == 0 {
                    let [r, g, b, _] = top.0;
                    line.push(Span::styled("▀", Style::default().fg(Color::Rgb(r, g, b))));
                } else if top[3] == 0 && btm[3] > 0 {
                    let [r, g, b, _] = btm.0;
                    line.push(Span::styled("▄", Style::default().fg(Color::Rgb(r, g, b))));
                } else {
                    let [fr, fg, fb, _] = top.0;
                    let [br, bg, bb, _] = btm.0;
                    line.push(Span::styled(
                        "▀",
                        Style::default()
                            .fg(Color::Rgb(fr, fg, fb))
                            .bg(Color::Rgb(br, bg, bb)),
                    ));
                }
            }
            lines.push(Line::from(line));
        }
        lines
    }

    fn init() -> AppResult<()> {
        let logfile_path = store_path("sshattrick.log")?;
        let logfile = FileAppender::builder()
            .append(false)
            .encoder(Box::new(PatternEncoder::new("{l} - {m}\n")))
            .build(logfile_path)?;

        let config = Config::builder()
            .appender(Appender::builder().build("logfile", Box::new(logfile)))
            .build(Root::builder().appender("logfile").build(LevelFilter::Info))?;

        log4rs::init_config(config)?;

        Ok(())
    }

    #[test]
    fn test_puck_position_with_rotation() -> AppResult<()> {
        let mut player = Player::new(GameSide::Red);
        player.set_position(U16Vec2::new(50, 40));
        let mut puck = Puck::new();

        puck.set_position(player.catcher_position());

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;

        terminal.clear()?;

        let palette = Palette::Dark;

        for _ in 0..16 {
            puck.set_position(player.catcher_position());
            terminal.draw(|frame| {
                let mut img = PITCH_IMAGES
                    .get(&palette)
                    .expect("Pitch image should exist")
                    .clone();

                img.copy_non_trasparent_from(
                    &player.image(palette),
                    player.position().x as u32,
                    player.position().y as u32,
                )
                .unwrap();

                img.copy_non_trasparent_from(
                    &puck.image(palette),
                    puck.position().x as u32,
                    puck.position().y as u32,
                )
                .unwrap();

                let split = Layout::vertical([Constraint::Length(5), Constraint::Min(1)])
                    .split(frame.area());

                let info = Paragraph::new(format!("Orientation {}", player.orientation as u8));
                frame.render_widget(info, split[0]);

                let paragraph = Paragraph::new(img_to_lines(&img));
                frame.render_widget(paragraph, split[1]);
            })?;
            player.rotate(player.orientation.next());
            std::thread::sleep(time::Duration::from_millis(500));
        }

        terminal.clear()?;

        Ok(())
    }

    #[test]
    fn test_goalie_boundaries() -> AppResult<()> {
        let mut red_goalie = goalie::Goalie::new(GameSide::Red);
        let mut blue_goalie = goalie::Goalie::new(GameSide::Blue);

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let palette = Palette::Dark;
        std::thread::sleep(time::Duration::from_millis(500));

        for idx in 0..32 {
            terminal.draw(|frame| {
                let mut img = PITCH_IMAGES
                    .get(&palette)
                    .expect("Pitch image should exist")
                    .clone();

                img.copy_non_trasparent_from(
                    &red_goalie.image(palette),
                    red_goalie.position().x as u32,
                    red_goalie.position().y as u32,
                )
                .unwrap();

                img.copy_non_trasparent_from(
                    &blue_goalie.image(palette),
                    blue_goalie.position().x as u32,
                    blue_goalie.position().y as u32,
                )
                .unwrap();

                let split = Layout::vertical([Constraint::Length(5), Constraint::Min(1)])
                    .split(frame.area());

                let info = Paragraph::new(format!(
                    "Red position {} - Blue position {} - Size {} ",
                    red_goalie.position(),
                    blue_goalie.position(),
                    red_goalie.size(),
                ));
                frame.render_widget(info, split[0]);

                let paragraph = Paragraph::new(img_to_lines(&img));
                frame.render_widget(paragraph, split[1]);
            })?;

            let new_position = (red_goalie.position().as_i16vec2()
                + I16Vec2::new(0, if idx > 10 { 1 } else { -1 }))
            .as_u16vec2();
            red_goalie.set_position(new_position);

            let new_position = (blue_goalie.position().as_i16vec2()
                + I16Vec2::new(0, if idx > 10 { 1 } else { -1 }))
            .as_u16vec2();
            blue_goalie.set_position(new_position);
            std::thread::sleep(time::Duration::from_millis(250));
        }

        terminal.clear()?;
        Ok(())
    }

    #[test]
    fn test_goalie_position_with_rotation() -> AppResult<()> {
        let mut player = Player::new(GameSide::Red);
        player.set_position(U16Vec2::new(15, 40));
        let mut puck = Puck::new();
        puck.set_position(player.catcher_position());

        let mut goalie = goalie::Goalie::new(GameSide::Red);
        goalie.align_to_player(&player);

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let palette = Palette::Dark;

        for idx in 0..16 {
            puck.set_position(player.catcher_position());
            goalie.align_to_player(&player);

            terminal.draw(|frame| {
                let mut img = PITCH_IMAGES
                    .get(&palette)
                    .expect("Pitch image should exist")
                    .clone();

                img.copy_non_trasparent_from(
                    &player.image(palette),
                    player.position().x as u32,
                    player.position().y as u32,
                )
                .unwrap();

                img.copy_non_trasparent_from(
                    &puck.image(palette),
                    puck.position().x as u32,
                    puck.position().y as u32,
                )
                .unwrap();

                img.copy_non_trasparent_from(
                    &goalie.image(palette),
                    goalie.position().x as u32,
                    goalie.position().y as u32,
                )
                .unwrap();

                let split = Layout::vertical([Constraint::Length(5), Constraint::Min(1)])
                    .split(frame.area());

                let info = Paragraph::new(format!("Orientation {}", player.orientation as u8));
                frame.render_widget(info, split[0]);

                let paragraph = Paragraph::new(img_to_lines(&img));
                frame.render_widget(paragraph, split[1]);
            })?;

            let new_position =
                (player.position().as_i16vec2() + I16Vec2::new(0, idx % 3 - 1)).as_u16vec2();
            player.set_position(new_position);
            player.rotate(player.orientation.next());
            std::thread::sleep(time::Duration::from_millis(500));
        }

        terminal.clear()?;
        Ok(())
    }

    #[test]
    fn test_goalie_areas() -> AppResult<()> {
        let red_area = Area::new(GameSide::Red);
        let blue_area = Area::new(GameSide::Blue);
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let palette = Palette::Dark;
        terminal.draw(|frame| {
            let mut img = PITCH_IMAGES
                .get(&palette)
                .expect("Pitch image should exist")
                .clone();

            for area in [red_area, blue_area].iter() {
                for (point, collider_type) in area.hit_box().iter() {
                    let pixel = match collider_type {
                        ColliderType::GoalieAreaHorizontalSide => Rgba::from([255, 255, 0, 55]),
                        ColliderType::GoalieAreaVerticalSize => Rgba::from([0, 255, 255, 55]),
                        _ => unreachable!(),
                    };
                    let g_point = area.position() + point;
                    img.put_pixel(g_point.x as u32, g_point.y as u32, pixel);
                }
            }

            let split =
                Layout::vertical([Constraint::Length(5), Constraint::Min(1)]).split(frame.area());

            let paragraph = Paragraph::new(img_to_lines(&img));
            frame.render_widget(paragraph, split[1]);
        })?;
        std::thread::sleep(time::Duration::from_millis(5000));
        terminal.clear()?;
        Ok(())
    }

    #[test]
    fn test_puck_boundaries() -> AppResult<()> {
        let mut game = Game::new();
        game.state = GameState::Running;

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        for _ in 0..100 {
            if let Err(e) = game.update() {
                log::error!("Update error: {e}");
            }
            terminal.draw(|frame| {
                let split = Layout::vertical([Constraint::Length(5), Constraint::Min(1)])
                    .split(frame.area());
                let image = game.draw().expect("update error");
                let paragraph = Paragraph::new(img_to_lines(&image));
                frame.render_widget(paragraph, split[1]);
            })?;
            std::thread::sleep(time::Duration::from_millis(50));
        }

        game.puck.set_velocity(Vec2::new(0.075, 0.0));

        for _ in 0..100 {
            if let Err(e) = game.update() {
                log::error!("Update error: {e}");
            }
            terminal.draw(|frame| {
                let split = Layout::vertical([Constraint::Length(5), Constraint::Min(1)])
                    .split(frame.area());
                let image = game.draw().expect("update error");
                let paragraph = Paragraph::new(img_to_lines(&image));
                frame.render_widget(paragraph, split[1]);
            })?;
            std::thread::sleep(time::Duration::from_millis(50));
        }

        terminal.clear()?;
        Ok(())
    }

    #[test]
    fn test_goal_areas() -> AppResult<()> {
        let mut puck = Puck::new();
        puck.set_position(U16Vec2::new(MAX_X - 20, 30));
        puck.set_velocity(Vec2::new(0.02, 0.0));
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.clear()?;
        let palette = Palette::Dark;

        let mut last_tick = Instant::now();
        let mut score = 0;
        let mut y = 0.0;
        loop {
            let now = Instant::now();
            let deltatime = now.duration_since(last_tick).as_millis() as f32;
            puck.update(deltatime);

            terminal.draw(|frame| {
                let split = Layout::vertical([Constraint::Length(5), Constraint::Min(1)])
                    .split(frame.area());

                let mut img = PITCH_IMAGES
                    .get(&palette)
                    .expect("Pitch image should exist")
                    .clone();

                img.copy_non_trasparent_from(
                    &puck.image(palette),
                    puck.position().x as u32,
                    puck.position().y as u32,
                )
                .unwrap();

                let info = format!("Score {}", score);
                let paragraph = Paragraph::new(info);
                frame.render_widget(paragraph, split[0]);
                let paragraph = Paragraph::new(img_to_lines(&img));
                frame.render_widget(paragraph, split[1]);
            })?;

            if puck.has_scored().is_some() {
                score += 1;
                y += 1.0;
                puck.set_position(U16Vec2::new(MAX_X - 20, 30 + y as u16));
                puck.set_velocity(Vec2::new(0.05, 0.0));
            } else if puck.velocity.x < 0.0 {
                y += 1.0;
                puck.set_position(U16Vec2::new(MAX_X - 20, 30 + y as u16));
                puck.set_velocity(Vec2::new(0.05, 0.0));
            }

            if y > 30.0 {
                break;
            }

            std::thread::sleep(time::Duration::from_millis(20));
            last_tick = now;
        }

        assert!(score == GOALIE_AREA_HEIGHT - 2 + 1);
        Ok(())
    }

    #[test]
    fn test_player_hitbox_with_rotation() -> AppResult<()> {
        let mut player = Player::new(GameSide::Red);
        player.set_position(U16Vec2::new(50, 40));
        let mut puck = Puck::new();
        puck.set_position(player.catcher_position());

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let palette = Palette::Dark;

        for _ in 0..16 {
            puck.set_position(player.catcher_position());
            terminal.draw(|frame| {
                let mut img = PITCH_IMAGES
                    .get(&palette)
                    .expect("Pitch image should exist")
                    .clone();

                img.copy_non_trasparent_from(
                    &player.image(palette),
                    player.position().x as u32,
                    player.position().y as u32,
                )
                .unwrap();

                img.copy_non_trasparent_from(
                    &puck.image(palette),
                    puck.position().x as u32,
                    puck.position().y as u32,
                )
                .unwrap();

                for (point, collider_type) in player.hit_box().iter() {
                    let img_point = player.position() + point;
                    let pixel = match collider_type {
                        ColliderType::Player => Rgba([255, 55, 55, 255]),
                        ColliderType::Stick => Rgba([55, 255, 125, 255]),
                        ColliderType::Catcher => Rgba([55, 125, 255, 255]),
                        _ => unreachable!(),
                    };
                    img.put_pixel(img_point.x as u32, img_point.y as u32, pixel);
                }

                let split = Layout::vertical([Constraint::Length(5), Constraint::Min(1)])
                    .split(frame.area());

                let info = Paragraph::new(format!("Orientation {}", player.orientation as u8));
                frame.render_widget(info, split[0]);

                let paragraph = Paragraph::new(img_to_lines(&img));
                frame.render_widget(paragraph, split[1]);
            })?;
            player.rotate(player.orientation.next());
            std::thread::sleep(time::Duration::from_millis(500));
        }

        terminal.clear()?;
        Ok(())
    }

    #[test]
    fn test_player_puck_collisions() -> AppResult<()> {
        init()?;
        let mut game = Game::new();
        game.state = GameState::Running;

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        game.puck.set_position(U16Vec2::new(80, 40));
        game.puck.set_velocity(Vec2::new(0.1, 0.0));

        let start = Instant::now();
        loop {
            if let Err(e) = game.update() {
                log::error!("Update error: {e}");
            }

            terminal.draw(|frame| {
                let split = Layout::vertical([Constraint::Length(5), Constraint::Min(1)])
                    .split(frame.area());

                let player = &game.blue_data.player;
                if let Some(colliders) = are_colliding(player, &game.puck) {
                    let paragraph = Paragraph::new(format!("Collision detected: {:#?}", colliders));
                    frame.render_widget(paragraph, split[0]);
                }

                let mut image = game.draw().expect("update error");

                for (point, collider_type) in player.hit_box().iter() {
                    let img_point = player.position() + point;
                    let pixel = match collider_type {
                        ColliderType::Player => Rgba([255, 55, 55, 255]),
                        ColliderType::Stick => Rgba([55, 255, 125, 255]),
                        ColliderType::Catcher => Rgba([55, 125, 255, 255]),
                        _ => unreachable!(),
                    };
                    image.put_pixel(img_point.x as u32, img_point.y as u32, pixel);
                }

                for (point, collider_type) in game.puck.hit_box().iter() {
                    let img_point = game.puck.position() + point;
                    let pixel = match collider_type {
                        ColliderType::Puck => Rgba([0, 155, 255, 255]),
                        _ => unreachable!(),
                    };
                    image.put_pixel(img_point.x as u32, img_point.y as u32, pixel);
                }

                let paragraph = Paragraph::new(img_to_lines(&image));
                frame.render_widget(paragraph, split[1]);
            })?;

            if start.elapsed() > Duration::from_millis(5000) {
                break;
            }
        }

        terminal.clear()?;
        Ok(())
    }

    fn position_puck_over(player: &Player, target_local: U16Vec2) -> Puck {
        let mut puck = Puck::new();
        let puck_first_local = *puck.hit_box().iter().next().expect("puck has pixels").0;
        let target_world = player.position() + target_local;
        puck.set_position(target_world - puck_first_local);
        puck
    }

    fn find_collider_in(player: &Player, kind: ColliderType) -> U16Vec2 {
        *player
            .previous_hit_box()
            .iter()
            .find(|(_, &ct)| ct == kind)
            .unwrap_or_else(|| panic!("previous hit_box has no {kind:?} pixel"))
            .0
    }

    #[test]
    fn swept_rotation_catch_returns_none_without_rotation() {
        let mut player = Player::new(GameSide::Red);
        player.set_position(U16Vec2::new(50, 40));
        assert!(!player.just_rotated());

        let mut puck = Puck::new();
        puck.set_position(player.position());

        assert_eq!(swept_rotation_catch(&puck, &player), None);
    }

    #[test]
    fn swept_rotation_catch_returns_none_when_puck_far_away() {
        let mut player = Player::new(GameSide::Red);
        player.set_position(U16Vec2::new(50, 40));
        player.rotate(Orientation::Up);
        assert!(player.just_rotated());

        let mut puck = Puck::new();
        puck.set_position(U16Vec2::new(120, 70));

        assert_eq!(swept_rotation_catch(&puck, &player), None);
    }

    #[test]
    fn swept_rotation_catch_detects_stick_in_previous_orientation() {
        let mut player = Player::new(GameSide::Red);
        player.set_position(U16Vec2::new(50, 40));
        player.rotate(Orientation::Up);

        let stick_local = find_collider_in(&player, ColliderType::Stick);
        let puck = position_puck_over(&player, stick_local);

        let result = swept_rotation_catch(&puck, &player);
        assert!(
            matches!(result, Some((ColliderType::Puck, ColliderType::Stick))),
            "expected Stick hit, got {result:?}",
        );
    }

    #[test]
    fn swept_rotation_catch_detects_catcher_in_previous_orientation() {
        let mut player = Player::new(GameSide::Red);
        player.set_position(U16Vec2::new(50, 40));
        player.rotate(Orientation::Up);

        let catcher_local = find_collider_in(&player, ColliderType::Catcher);
        let puck = position_puck_over(&player, catcher_local);

        let result = swept_rotation_catch(&puck, &player);
        assert!(
            matches!(result, Some((ColliderType::Puck, ColliderType::Catcher))),
            "expected Catcher hit, got {result:?}",
        );
    }

    #[test]
    fn new_practice_flips_the_practice_flag() {
        let regular = Game::new();
        assert!(!regular.practice_mode);
        let practice = Game::new_practice();
        assert!(practice.practice_mode);
        // The visible sprite set should be smaller in practice (no Blue player).
        let practice_count = practice
            .visible_sprites()
            .iter()
            .filter(|s| s.is_some())
            .count();
        let regular_count = regular
            .visible_sprites()
            .iter()
            .filter(|s| s.is_some())
            .count();
        assert!(practice_count < regular_count);
    }

    #[test]
    fn just_rotated_clears_after_update_body() {
        let mut player = Player::new(GameSide::Red);
        player.rotate(Orientation::Up);
        assert!(player.just_rotated(), "rotation should set the flag");

        player.update(0.0);
        assert!(
            !player.just_rotated(),
            "update_body should clear previous_orientation back to current",
        );
    }
}
