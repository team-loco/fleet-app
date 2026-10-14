use std::time::Duration;

pub const WATCH_SYNC_TIMEOUT: Duration = Duration::from_secs(30);
pub const NOT_STARTED: &str = "watch not started";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchState {
    Pending(String),
    Synced,
    Failed(String),
}

impl WatchState {
    pub fn pending(step: impl Into<String>) -> Self {
        Self::Pending(step.into())
    }

    pub fn combine(states: impl IntoIterator<Item = Self>) -> Self {
        let mut pending = None;
        for state in states {
            match state {
                Self::Failed(reason) => return Self::Failed(reason),
                Self::Pending(step) => {
                    pending.get_or_insert(step);
                }
                Self::Synced => {}
            }
        }
        pending.map_or(Self::Synced, Self::Pending)
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
