use tokio::sync::broadcast;

use super::event::ActivityEvent;

pub type ActivitySender = broadcast::Sender<ActivityEvent>;
pub type ActivityReceiver = broadcast::Receiver<ActivityEvent>;

pub fn new(capacity: usize) -> (ActivitySender, ActivityReceiver) {
    broadcast::channel::<ActivityEvent>(capacity)
}
