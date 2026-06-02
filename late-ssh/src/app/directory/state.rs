#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectoryTab {
    Profiles,
    Projects,
    Pinstar,
}

impl DirectoryTab {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Profiles => Self::Projects,
            Self::Projects => Self::Pinstar,
            Self::Pinstar => Self::Profiles,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Profiles => Self::Pinstar,
            Self::Projects => Self::Profiles,
            Self::Pinstar => Self::Projects,
        }
    }
}

pub(crate) struct DirectoryState {
    pub(crate) tab: DirectoryTab,
}

impl DirectoryState {
    pub(crate) fn new() -> Self {
        Self {
            tab: DirectoryTab::Profiles,
        }
    }

    pub(crate) fn select(&mut self, tab: DirectoryTab) {
        self.tab = tab;
    }
}
