use super::svc::GameKind;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RoomsFilter {
    #[default]
    All,
    Kind(GameKind),
}

impl RoomsFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Kind(GameKind::Blackjack) => "Blackjack",
            Self::Kind(GameKind::Poker) => "Poker",
            Self::Kind(GameKind::TicTacToe) => "Tic-Tac-Toe",
        }
    }

    pub fn matches_real(self, kind: GameKind) -> bool {
        match self {
            Self::All => true,
            Self::Kind(filter_kind) => filter_kind == kind,
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        let mut filters = Vec::with_capacity(GameKind::ALL.len() + 1);
        filters.push(Self::All);
        filters.extend(GameKind::ALL.iter().copied().map(Self::Kind));
        let idx = filters.iter().position(|f| *f == self).unwrap_or(0);
        let len = filters.len();
        let next = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        filters[next]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps_in_both_directions() {
        assert_eq!(
            RoomsFilter::All.cycle(true),
            RoomsFilter::Kind(GameKind::Blackjack)
        );
        assert_eq!(
            RoomsFilter::Kind(GameKind::TicTacToe).cycle(true),
            RoomsFilter::All
        );
        assert_eq!(
            RoomsFilter::All.cycle(false),
            RoomsFilter::Kind(GameKind::TicTacToe)
        );
    }

    #[test]
    fn all_matches_everything() {
        assert!(RoomsFilter::All.matches_real(GameKind::Blackjack));
        assert!(RoomsFilter::All.matches_real(GameKind::Poker));
        assert!(RoomsFilter::All.matches_real(GameKind::TicTacToe));
    }

    #[test]
    fn kind_filter_matches_only_that_kind() {
        assert!(RoomsFilter::Kind(GameKind::Blackjack).matches_real(GameKind::Blackjack));
        assert!(!RoomsFilter::Kind(GameKind::Blackjack).matches_real(GameKind::Poker));
        assert!(!RoomsFilter::Kind(GameKind::Blackjack).matches_real(GameKind::TicTacToe));
    }
}
