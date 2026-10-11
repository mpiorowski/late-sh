use super::{input::FocusedItem, state::Shelf};
use uuid::Uuid;
pub(crate) type MouseState = crate::app::common::mouse::MouseState<Target, Pane>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pane {
    PeopleList,
    PeopleDetail,
    JobsList,
    JobsDetail,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Shelf(Shelf),
    Key(u8),
    Person(Uuid),
    Job(Uuid),
    Item(FocusedItem, u8),
    Copy(String),
    Profile(Uuid, String),
    Back,
}
