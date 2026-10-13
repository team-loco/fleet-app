use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

const FILE: &str = "view.json";
const TEMP_SUFFIX: &str = "json.tmp";
pub const FLEET_SCOPE: &str = "fleet";

#[derive(Debug, thiserror::Error)]
pub enum PrefsError {
    #[error("view preferences: {0}")]
    Io(#[from] std::io::Error),
    #[error("view preferences are not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub type ColumnWidths = BTreeMap<String, f32>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopePrefs {
    pub columns: BTreeMap<String, ColumnWidths>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrefsFile {
    pub scopes: BTreeMap<String, ScopePrefs>,
}

#[derive(Debug, Default)]
pub struct ViewPrefs {
    path: Option<PathBuf>,
    file: PrefsFile,
}

impl Global for ViewPrefs {}

impl ViewPrefs {
    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn empty_at(dir: &Path) -> Self {
        Self {
            path: Some(dir.join(FILE)),
            file: PrefsFile::default(),
        }
    }

    pub fn open(dir: &Path) -> Result<Self, PrefsError> {
        let path = dir.join(FILE);
        let file = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == ErrorKind::NotFound => PrefsFile::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path: Some(path),
            file,
        })
    }

    pub fn column_widths(&self, scope: &str, table: &str) -> ColumnWidths {
        self.file
            .scopes
            .get(scope)
            .and_then(|prefs| prefs.columns.get(table))
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_column_widths(
        &mut self,
        scope: &str,
        table: &str,
        widths: ColumnWidths,
    ) -> Result<(), PrefsError> {
        let prefs = self.file.scopes.entry(scope.to_owned()).or_default();
        if prefs.columns.get(table) == Some(&widths) {
            return Ok(());
        }
        prefs.columns.insert(table.to_owned(), widths);
        self.save()
    }

    fn save(&self) -> Result<(), PrefsError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(&self.file)?;
        let temp = path.with_extension(TEMP_SUFFIX);
        fs::write(&temp, json)?;
        fs::rename(&temp, path)?;
        Ok(())
    }
}

pub fn init(dir: Option<&Path>, cx: &mut App) {
    let prefs = match dir {
        Some(dir) => ViewPrefs::open(dir).unwrap_or_else(|error| {
            tracing::warn!(%error, "starting with default view preferences");
            ViewPrefs::empty_at(dir)
        }),
        None => ViewPrefs::in_memory(),
    };
    cx.set_global(prefs);
}

pub fn column_widths(scope: &str, table: &str, cx: &App) -> ColumnWidths {
    cx.try_global::<ViewPrefs>()
        .map(|prefs| prefs.column_widths(scope, table))
        .unwrap_or_default()
}

pub fn set_column_widths(scope: &str, table: &str, widths: ColumnWidths, cx: &mut App) {
    let result = cx
        .default_global::<ViewPrefs>()
        .set_column_widths(scope, table, widths);
    if let Err(error) = result {
        tracing::warn!(%error, "could not save column widths");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_widths_survive_a_reopen() -> Result<(), PrefsError> {
        let dir = tempfile::tempdir()?;
        let mut prefs = ViewPrefs::open(dir.path())?;
        let widths = ColumnWidths::from([("Name".to_owned(), 320.)]);
        prefs.set_column_widths("prod", "pods", widths.clone())?;
        let reopened = ViewPrefs::open(dir.path())?;
        assert_eq!(reopened.column_widths("prod", "pods"), widths);
        assert!(reopened.column_widths("staging", "pods").is_empty());
        assert!(reopened.column_widths("prod", "nodes").is_empty());
        Ok(())
    }

    #[test]
    fn a_missing_file_starts_empty_and_a_broken_one_is_an_error() -> Result<(), PrefsError> {
        let dir = tempfile::tempdir()?;
        assert_eq!(ViewPrefs::open(dir.path())?.file, PrefsFile::default());
        fs::write(dir.path().join(FILE), b"{ not json")?;
        assert!(matches!(
            ViewPrefs::open(dir.path()),
            Err(PrefsError::Json(_))
        ));
        Ok(())
    }

    #[test]
    fn in_memory_prefs_never_touch_the_disk() -> Result<(), PrefsError> {
        let mut prefs = ViewPrefs::in_memory();
        let widths = ColumnWidths::from([("Age".to_owned(), 60.)]);
        prefs.set_column_widths(FLEET_SCOPE, "fleet-clusters", widths.clone())?;
        assert_eq!(prefs.column_widths(FLEET_SCOPE, "fleet-clusters"), widths);
        Ok(())
    }
}
