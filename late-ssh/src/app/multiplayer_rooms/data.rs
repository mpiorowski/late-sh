pub struct RoomCard<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub game: &'a str,
    pub status: &'a str,
    pub seats: &'a str,
}

pub const ROOMS: [RoomCard<'static>; 2] = [
    RoomCard {
        slug: "bj-001",
        title: "Blackjack Room One",
        game: "Blackjack",
        status: "Open",
        seats: "1 / 5 seated",
    },
    RoomCard {
        slug: "bj-002",
        title: "Blackjack Room Two",
        game: "Blackjack",
        status: "Open",
        seats: "0 / 5 seated",
    },
];
