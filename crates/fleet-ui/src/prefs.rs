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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NamespacePick {
    All,
    Only(String),
}

impl NamespacePick {
    pub fn of(choice: Option<&str>) -> Self {
        choice.map_or(Self::All, |namespace| Self::Only(namespace.to_owned()))
    }

    pub fn choice(&self) -> Option<String> {
        match self {
            Self::All => None,
            Self::Only(namespace) => Some(namespace.clone()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopePrefs {
    pub columns: BTreeMap<String, ColumnWidths>,
    pub namespaces: BTreeMap<String, NamespacePick>,
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

    pub fn namespace_pick(&self, scope: &str, list: &str) -> Option<NamespacePick> {
        self.file
            .scopes
            .get(scope)
            .and_then(|prefs| prefs.namespaces.get(list))
            .cloned()
    }

    pub fn namespace(&self, scope: &str, list: &str, default: &str) -> Option<String> {
        self.namespace_pick(scope, list)
            .map_or_else(|| Some(default.to_owned()), |pick| pick.choice())
    }

    pub fn set_namespace(
        &mut self,
        scope: &str,
        list: &str,
        choice: Option<&str>,
    ) -> Result<(), PrefsError> {
        let pick = NamespacePick::of(choice);
        let prefs = self.file.scopes.entry(scope.to_owned()).or_default();
        if prefs.namespaces.get(list) == Some(&pick) {
            return Ok(());
        }
        prefs.namespaces.insert(list.to_owned(), pick);
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

pub fn namespace(scope: &str, list: &str, default: &str, cx: &App) -> Option<String> {
    cx.try_global::<ViewPrefs>().map_or_else(
        || Some(default.to_owned()),
        |prefs| prefs.namespace(scope, list, default),
    )
}

pub fn namespace_pick(scope: &str, list: &str, cx: &App) -> Option<NamespacePick> {
    cx.try_global::<ViewPrefs>()
        .and_then(|prefs| prefs.namespace_pick(scope, list))
}

pub fn set_namespace(scope: &str, list: &str, choice: Option<&str>, cx: &mut App) {
    let result = cx
        .default_global::<ViewPrefs>()
        .set_namespace(scope, list, choice);
    if let Err(error) = result {
        tracing::warn!(%error, "could not save the namespace choice");
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
    fn namespace_picks_default_until_chosen_and_survive_a_reopen() -> Result<(), PrefsError> {
        let dir = tempfile::tempdir()?;
        let mut prefs = ViewPrefs::open(dir.path())?;
        assert_eq!(
            prefs.namespace("prod", "Pod", "default").as_deref(),
            Some("default")
        );
        prefs.set_namespace("prod", "Pod", Some("loco-system"))?;
        prefs.set_namespace("prod", "Deployment", None)?;
        let reopened = ViewPrefs::open(dir.path())?;
        assert_eq!(
            reopened.namespace("prod", "Pod", "default").as_deref(),
            Some("loco-system")
        );
        assert_eq!(reopened.namespace("prod", "Deployment", "default"), None);
        assert_eq!(
            reopened.namespace("staging", "Pod", "apps").as_deref(),
            Some("apps")
        );
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
