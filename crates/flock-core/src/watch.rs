use std::time::Duration;

pub const WATCH_SYNC_TIMEOUT: Duration = Duration::from_secs(30);
pub const NOT_STARTED: &str = "watch not started";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchState {
    Pending(String),
    Synced,
    Refreshing(String),
    Failed(String),
}

impl WatchState {
    pub fn pending(step: impl Into<String>) -> Self {
        Self::Pending(step.into())
    }

    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Synced | Self::Refreshing(_))
    }

    pub fn combine(states: impl IntoIterator<Item = Self>) -> Self {
        let mut pending = None;
        let mut refreshing = None;
        for state in states {
            match state {
                Self::Failed(reason) => return Self::Failed(reason),
                Self::Pending(step) => {
                    pending.get_or_insert(step);
                }
                Self::Refreshing(reason) => {
                    refreshing.get_or_insert(reason);
                }
                Self::Synced => {}
            }
        }
        if let Some(step) = pending {
            return Self::Pending(step);
        }
        refreshing.map_or(Self::Synced, Self::Refreshing)
    }

    pub fn expire(self, elapsed: Duration) -> Self {
        match self {
            Self::Pending(step) if elapsed >= WATCH_SYNC_TIMEOUT => {
                let seconds = WATCH_SYNC_TIMEOUT.as_secs();
                Self::Failed(format!("{step}: no data within {seconds} s"))
            }
            state => state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_win_then_the_first_pending_step_then_synced() {
        assert_eq!(WatchState::combine([]), WatchState::Synced);
        let pending = [
            WatchState::Synced,
            WatchState::pending("Listing Pod"),
            WatchState::pending("Listing Event"),
        ];
        assert_eq!(
            WatchState::combine(pending),
            WatchState::pending("Listing Pod")
        );
        let failed = [
            WatchState::pending("Listing Pod"),
            WatchState::Failed("forbidden".into()),
        ];
        assert_eq!(
            WatchState::combine(failed),
            WatchState::Failed("forbidden".into())
        );
    }

    #[test]
    fn loaded_data_refreshing_ranks_below_a_page_that_never_loaded() {
        let refreshing = WatchState::Refreshing("watch dropped".into());
        assert!(refreshing.is_loaded());
        assert!(WatchState::Synced.is_loaded());
        assert!(!WatchState::pending(NOT_STARTED).is_loaded());
        assert!(!WatchState::Failed("forbidden".into()).is_loaded());
        assert_eq!(
            WatchState::combine([WatchState::Synced, refreshing.clone()]),
            refreshing
        );
        assert_eq!(
            WatchState::combine([refreshing.clone(), WatchState::pending("Listing Pod")]),
            WatchState::pending("Listing Pod")
        );
        assert_eq!(refreshing.clone().expire(WATCH_SYNC_TIMEOUT), refreshing);
    }

    #[test]
    fn pending_expires_into_a_failure_that_keeps_its_step() {
        let pending = WatchState::pending(NOT_STARTED);
        assert_eq!(pending.clone().expire(Duration::ZERO), pending);
        assert_eq!(
            pending.expire(WATCH_SYNC_TIMEOUT),
            WatchState::Failed("watch not started: no data within 30 s".into())
        );
        assert_eq!(
            WatchState::Synced.expire(WATCH_SYNC_TIMEOUT),
            WatchState::Synced
        );
    }
}
