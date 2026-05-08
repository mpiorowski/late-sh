use super::event::{ActivityCategory, ActivityEvent};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityFilter {
    categories: &'static [ActivityCategory],
}

impl ActivityFilter {
    pub const fn dashboard() -> Self {
        Self {
            categories: &[
                ActivityCategory::Session,
                ActivityCategory::Vote,
                ActivityCategory::Game,
                ActivityCategory::Bonsai,
            ],
        }
    }

    pub fn includes(&self, event: &ActivityEvent) -> bool {
        self.categories.contains(&event.category())
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::app::activity::event::ActivityEvent;

    #[test]
    fn dashboard_filter_includes_public_activity() {
        let event = ActivityEvent::joined(Uuid::nil(), "user");

        assert!(ActivityFilter::dashboard().includes(&event));
    }
}
