use tokio::sync::broadcast;

use super::event::ActivityEvent;

pub type ActivitySender = broadcast::Sender<ActivityEvent>;
pub type ActivityReceiver = broadcast::Receiver<ActivityEvent>;

pub const ACTIVITY_HISTORY_MAX_EVENTS: usize = 100;

pub fn new(capacity: usize) -> (ActivitySender, ActivityReceiver) {
    broadcast::channel::<ActivityEvent>(capacity)
}
