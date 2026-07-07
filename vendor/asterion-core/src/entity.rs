use crate::{Direction, PlayerId, Position, View};
use std::time::Duration;

pub trait Entity {
    fn id(&self) -> PlayerId;
    fn name(&self) -> &str;
    fn vision(&self) -> usize;
    fn view(&self) -> View {
        if self.vision() <= 2 {
            return View::Cone {
                radius: 6 + self.vision(),
            };
        }

        if self.vision() <= 4 {
            return View::Plane {
                radius: 5 + self.vision(),
            };
        }

        if self.vision() <= 7 {
            return View::Circle {
                radius: 4 + self.vision(),
            };
        }

        View::Full
    }
    fn speed(&self) -> u64;
    fn movement_recovery_duration(&self) -> Duration {
        Duration::from_millis(50 + 200_u64.saturating_sub(25 * self.speed()))
    }
    fn position(&self) -> Position;
    fn direction(&self) -> Direction;
    fn maze_id(&self) -> usize;
}
