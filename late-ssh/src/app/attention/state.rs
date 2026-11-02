//! Attention: the guest that lists what waits on you and steps back the
//! moment the list is empty. Every row is a thing the session already
//! knows about (a daily match on your move or with a result you have not
//! looked at, a DM or a mention unread, a companion uncared for today),
//! and each row clears on the act that answers it: the care rows on the
//! UTC day, so they come back each morning, the rest on the read or the
//! move. Nothing here fetches: `items` orders what the glue hands it
//! (`App::attention_items`), and `ui.rs` paints the rows.

use uuid::Uuid;

/// One row of the list, in the order `items` puts them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// A daily match on your move.
    YourMove {
        game: &'static str,
        opponent: String,
    },
    /// A finished daily match whose result you have not looked at.
    Result {
        game: &'static str,
        opponent: String,
    },
    /// A DM with unread messages; `peer` is the other person's name.
    Dm {
        room_id: Uuid,
        peer: String,
        unread: i64,
    },
    /// An unread mention; `room` is `None` for a room with no name.
    Mention {
        room_id: Uuid,
        message_id: Uuid,
        actor: String,
        room: Option<String>,
    },
    /// The tank has not eaten today (owners only).
    FeedTank,
    /// The tree has not been watered today.
    WaterTree,
    /// The pet has not been petted today (owners only).
    PetPet,
}

impl Item {
    /// The row's words.
    pub fn label(&self) -> String {
        match self {
            Item::YourMove { game, opponent } => format!("your move: {game} vs {opponent}"),
            Item::Result { game, opponent } => format!("result in: {game} vs {opponent}"),
            Item::Dm { peer, unread, .. } => match unread {
                1 => format!("@{peer}: one unread"),
                n => format!("@{peer}: {n} unread"),
            },
            Item::Mention { actor, room, .. } => match room {
                Some(room) => format!("@{actor} mentioned you in #{room}"),
                None => format!("@{actor} mentioned you"),
            },
            Item::FeedTank => "the tank is unfed today".to_string(),
            Item::WaterTree => "the tree is unwatered today".to_string(),
            Item::PetPet => "the pet is unpetted today".to_string(),
        }
    }

    /// What Enter on the row does, in one word, drawn at the row's end.
    pub fn act(&self) -> &'static str {
        match self {
            Item::YourMove { .. } => "play",
            Item::Result { .. } => "look",
            Item::Dm { .. } | Item::Mention { .. } => "read",
            Item::FeedTank => "feed",
            Item::WaterTree => "water",
            Item::PetPet => "pet",
        }
    }

    /// The glyph before the words.
    pub fn glyph(&self) -> &'static str {
        match self {
            Item::YourMove { .. } => "▶",
            Item::Result { .. } => "■",
            Item::Dm { .. } => "✉",
            Item::Mention { .. } => "@",
            Item::FeedTank | Item::WaterTree | Item::PetPet => "∘",
        }
    }
}

/// A daily match as the list needs it: the game's label and the other
/// player's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub game: &'static str,
    pub opponent: String,
}

/// What the session knows, gathered by the glue each frame and each key.
/// The care flags are already gated on ownership: an unowned tank or pet
/// is never due.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inputs {
    /// Matches on your move, newest first.
    pub my_moves: Vec<Match>,
    /// Finished matches you have not looked at, newest finish first.
    pub results: Vec<Match>,
    /// DMs with unread messages, the most unread first.
    pub dms: Vec<Item>,
    /// Unread mentions, newest first.
    pub mentions: Vec<Item>,
    pub tank_due: bool,
    pub tree_due: bool,
    pub pet_due: bool,
}

/// The list, in the order a glance wants: what blocks someone else (your
/// moves), then what finished, then people (DMs, mentions), then the
/// companions. Empty when nothing waits, and the guest is down.
pub fn items(inputs: Inputs) -> Vec<Item> {
    let mut rows: Vec<Item> = Vec::new();
    rows.extend(inputs.my_moves.into_iter().map(|m| Item::YourMove {
        game: m.game,
        opponent: m.opponent,
    }));
    rows.extend(inputs.results.into_iter().map(|m| Item::Result {
        game: m.game,
        opponent: m.opponent,
    }));
    rows.extend(inputs.dms);
    rows.extend(inputs.mentions);
    if inputs.tank_due {
        rows.push(Item::FeedTank);
    }
    if inputs.tree_due {
        rows.push(Item::WaterTree);
    }
    if inputs.pet_due {
        rows.push(Item::PetPet);
    }
    rows
}

/// Today's care for one companion: tended, still due, or not owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chore {
    Done,
    Due,
    NotOwned,
}

impl Chore {
    pub fn of(owned: bool, done_today: bool) -> Self {
        match (owned, done_today) {
            (false, _) => Chore::NotOwned,
            (true, true) => Chore::Done,
            (true, false) => Chore::Due,
        }
    }
}

/// The three once-a-day chores, read off the session by the glue
/// (`App::care_today`): what the care rows and the status line's Care
/// count come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Care {
    pub bonsai: Chore,
    pub tank: Chore,
    pub pet: Chore,
}

impl Care {
    /// How many owned companions still wait on today's care.
    pub fn due_count(&self) -> usize {
        [self.bonsai, self.tank, self.pet]
            .into_iter()
            .filter(|chore| *chore == Chore::Due)
            .count()
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
