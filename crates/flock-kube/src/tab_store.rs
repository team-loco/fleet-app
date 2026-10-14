use std::path::{Path, PathBuf};

use flock_core::tabs::Session;
use jiff::Timestamp;

use crate::error::Result;
use crate::store::{load, write};

const TABS_FILE: &str = "tabs.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStore {
    path: PathBuf,
    session: Session,
    set_aside: Option<PathBuf>,
}

impl TabStore {
    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_at(dir, Timestamp::now())
    }

    pub fn open_at(dir: &Path, now: Timestamp) -> Result<Self> {
        let path = dir.join(TABS_FILE);
        let loaded = load::<Session>(&path, now)?;
        Ok(Self {
            path,
            session: loaded.value.unwrap_or_default(),
            set_aside: loaded.set_aside,
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn set_aside(&self) -> Option<&Path> {
        self.set_aside.as_deref()
    }

    pub fn save(&mut self, session: &Session) -> Result<()> {
        if &self.session == session {
            return Ok(());
        }
        write(&self.path, session)?;
        self.session = session.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use flock_core::history::Location;
    use flock_core::tabs::{SavedPane, SavedWindow};
    use flock_core::{ClusterId, Page};

    use super::*;

    fn now() -> Timestamp {
        match "2026-10-13T06:36:00Z".parse() {
            Ok(now) => now,
            Err(error) => panic!("timestamp: {error}"),
        }
    }

    fn session() -> Session {
        let window = SavedWindow {
            panes: vec![SavedPane {
                tabs: vec![
                    Location::default(),
                    Location::page_on(ClusterId::new("prod-us-east"), Page::Pods),
                ],
                active: 1,
            }],
            focused: 0,
        };
        Session {
            windows: vec![window.clone(), window],
        }
    }

    #[test]
    fn saved_tabs_survive_a_reopen() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let Ok(mut store) = TabStore::open_at(dir.path(), now()) else {
            panic!("open");
        };
        assert_eq!(store.session(), &Session::default());
        assert!(store.save(&session()).is_ok());
        let Ok(reopened) = TabStore::open_at(dir.path(), now()) else {
            panic!("reopen");
        };
        assert_eq!(reopened.session(), &session());
        assert_eq!(reopened.set_aside(), None);
    }

    #[test]
    fn a_corrupt_file_is_set_aside_and_starts_empty() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        assert!(fs::write(dir.path().join(TABS_FILE), "{\"windows\": 3").is_ok());
        let Ok(store) = TabStore::open_at(dir.path(), now()) else {
            panic!("open");
        };
        let kept = dir.path().join("tabs.json.corrupt-20261013T063600Z");
        assert_eq!(store.set_aside(), Some(kept.as_path()));
        assert_eq!(store.session(), &Session::default());
    }
}
