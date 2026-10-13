use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use fleet_core::settings::{MetricsSource, NamespaceSelection};
use fleet_core::{ClusterId, NewCluster, Preferences};
use jiff::Timestamp;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::discovery::ContextRef;
use crate::error::{Error, Result};

const APP_SUPPORT: &str = "Library/Application Support";
const APP_ID: &str = "dev.fleet.app";
const CLUSTERS_FILE: &str = "clusters.json";
const TEMP_SUFFIX: &str = "json.tmp";
const CORRUPT_SUFFIX: &str = "corrupt";
const CORRUPT_STAMP: &str = "%Y%m%dT%H%M%SZ";
const KUBECONFIGS_DIR: &str = "kubeconfigs";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedCluster {
    pub file: PathBuf,
    pub context: String,
    pub name: String,
    #[serde(default)]
    pub preferences: SavedPreferences,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedPreferences {
    pub namespace_selection: Option<SavedNamespace>,
    pub default_namespace: Option<String>,
    pub metrics: SavedMetrics,
    pub confirm_destructive: bool,
    pub notify: bool,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SavedMetrics {
    #[default]
    Auto,
    MetricsServer,
    Prometheus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SavedNamespace {
    All,
    Named(String),
}

impl Default for SavedPreferences {
    fn default() -> Self {
        Self::from(&Preferences::default())
    }
}

impl From<&Preferences> for SavedPreferences {
    fn from(preferences: &Preferences) -> Self {
        let metrics = match preferences.metrics {
            MetricsSource::Auto => SavedMetrics::Auto,
            MetricsSource::MetricsServer => SavedMetrics::MetricsServer,
            MetricsSource::Prometheus => SavedMetrics::Prometheus,
        };
        Self {
            namespace_selection: preferences.namespace_selection.as_ref().map(
                |scope| match scope {
                    NamespaceSelection::All => SavedNamespace::All,
                    NamespaceSelection::Named(namespace) => {
                        SavedNamespace::Named(namespace.clone())
                    }
                },
            ),
            default_namespace: preferences.default_namespace.clone(),
            metrics,
            confirm_destructive: preferences.confirm_destructive,
            notify: preferences.notify,
            labels: preferences.labels.clone(),
        }
    }
}

impl From<&SavedPreferences> for Preferences {
    fn from(saved: &SavedPreferences) -> Self {
        let metrics = match saved.metrics {
            SavedMetrics::Auto => MetricsSource::Auto,
            SavedMetrics::MetricsServer => MetricsSource::MetricsServer,
            SavedMetrics::Prometheus => MetricsSource::Prometheus,
        };
        Self {
            namespace_selection: saved.namespace_selection.as_ref().map(|scope| match scope {
                SavedNamespace::All => NamespaceSelection::All,
                SavedNamespace::Named(namespace) => NamespaceSelection::Named(namespace.clone()),
            }),
            default_namespace: saved.default_namespace.clone(),
            metrics,
            confirm_destructive: saved.confirm_destructive,
            notify: saved.notify,
            labels: saved.labels.clone(),
        }
    }
}

impl SavedCluster {
    pub fn context_ref(&self) -> ContextRef {
        ContextRef {
            file: self.file.clone(),
            context: self.context.clone(),
        }
    }

    pub fn id(&self) -> ClusterId {
        ClusterId::new(&self.name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct StoreFile {
    clusters: Vec<SavedCluster>,
    #[serde(default)]
    folders: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterStore {
    path: PathBuf,
    clusters: Vec<SavedCluster>,
    folders: Vec<PathBuf>,
    set_aside: Option<PathBuf>,
}

impl ClusterStore {
    pub fn default_dir(home: &Path) -> PathBuf {
        home.join(APP_SUPPORT).join(APP_ID)
    }

    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_at(dir, Timestamp::now())
    }

    pub fn open_at(dir: &Path, now: Timestamp) -> Result<Self> {
        let loaded = load::<StoreFile>(&dir.join(CLUSTERS_FILE), now)?;
        let file = loaded.value.unwrap_or_default();
        Ok(Self {
            clusters: file.clusters,
            folders: file.folders,
            set_aside: loaded.set_aside,
            ..Self::empty(dir)
        })
    }

    pub fn empty(dir: &Path) -> Self {
        Self {
            path: dir.join(CLUSTERS_FILE),
            clusters: Vec::new(),
            folders: Vec::new(),
            set_aside: None,
        }
    }

    pub fn set_aside(&self) -> Option<&Path> {
        self.set_aside.as_deref()
    }

    pub fn kubeconfigs_dir(&self) -> PathBuf {
        let dir = self.path.parent().unwrap_or(Path::new(""));
        dir.join(KUBECONFIGS_DIR)
    }

    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    pub fn add_folder(&mut self, folder: &Path) -> Result<bool> {
        if !folder.is_dir() {
            return Err(Error::NotAFolder(folder.to_path_buf()));
        }
        if self.folders.iter().any(|known| known == folder) {
            return Ok(false);
        }
        self.folders.push(folder.to_path_buf());
        self.save()?;
        Ok(true)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn clusters(&self) -> &[SavedCluster] {
        &self.clusters
    }

    pub fn contains(&self, context: &ContextRef) -> bool {
        self.clusters.iter().any(|c| &c.context_ref() == context)
    }

    pub fn add(&mut self, added: &[NewCluster]) -> Result<()> {
        for cluster in added {
            let taken = self.contains(&cluster.context)
                || self.clusters.iter().any(|c| c.name == cluster.name);
            if taken {
                continue;
            }
            self.clusters.push(SavedCluster {
                file: cluster.context.file.clone(),
                context: cluster.context.context.clone(),
                name: cluster.name.clone(),
                preferences: SavedPreferences::default(),
            });
        }
        self.save()
    }

    pub fn remove(&mut self, id: &ClusterId) -> Result<Option<SavedCluster>> {
        let Some(ix) = self.clusters.iter().position(|c| &c.id() == id) else {
            return Ok(None);
        };
        let removed = self.clusters.remove(ix);
        self.save()?;
        Ok(Some(removed))
    }

    pub fn get(&self, id: &ClusterId) -> Option<&SavedCluster> {
        self.clusters.iter().find(|c| &c.id() == id)
    }

    pub fn set_preferences(&mut self, id: &ClusterId, preferences: &Preferences) -> Result<bool> {
        let Some(saved) = self.clusters.iter_mut().find(|c| &c.id() == id) else {
            return Ok(false);
        };
        saved.preferences = SavedPreferences::from(preferences);
        self.save()?;
        Ok(true)
    }

    fn save(&self) -> Result<()> {
        let body = StoreFile {
            clusters: self.clusters.clone(),
            folders: self.folders.clone(),
        };
        write(&self.path, &body)
    }
}

pub(crate) struct Loaded<T> {
    pub value: Option<T>,
    pub set_aside: Option<PathBuf>,
}

pub(crate) fn load<T: DeserializeOwned>(path: &Path, now: Timestamp) -> Result<Loaded<T>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(Loaded {
                value: None,
                set_aside: None,
            });
        }
        Err(error) => return Err(error.into()),
    };
    match serde_json::from_slice::<T>(&bytes) {
        Ok(value) => Ok(Loaded {
            value: Some(value),
            set_aside: None,
        }),
        Err(error) => {
            let aside = corrupt_path(path, now);
            fs::rename(path, &aside)?;
            tracing::warn!(%error, file = %path.display(), kept = %aside.display(), "unreadable; set aside");
            Ok(Loaded {
                value: None,
                set_aside: Some(aside),
            })
        }
    }
}

pub(crate) fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(value)?;
    let temp = path.with_extension(TEMP_SUFFIX);
    fs::write(&temp, json)?;
    fs::rename(&temp, path)?;
    Ok(())
}

fn corrupt_path(path: &Path, now: Timestamp) -> PathBuf {
    let stamp = now.strftime(CORRUPT_STAMP);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.{CORRUPT_SUFFIX}-{stamp}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        match "2026-10-13T06:36:00Z".parse() {
            Ok(now) => now,
            Err(error) => panic!("timestamp: {error}"),
        }
    }

    fn new_cluster(context: &str, name: &str) -> NewCluster {
        NewCluster {
            context: ContextRef {
                file: PathBuf::from("/h/.kube/config"),
                context: context.to_owned(),
            },
            name: name.to_owned(),
        }
    }

    #[test]
    fn missing_file_is_an_empty_fleet() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let Ok(store) = ClusterStore::open_at(dir.path(), now()) else {
            panic!("open");
        };
        assert_eq!(store.clusters(), &[]);
    }

    #[test]
    fn added_clusters_survive_a_reopen_and_removal_forgets_them() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let nested = dir.path().join("dev.fleet.app");
        let mut store = ClusterStore::empty(&nested);
        let added = [
            new_cluster("kind-fleet-a", "fleet-a"),
            new_cluster("kind-fleet-b", "fleet-b"),
            new_cluster("kind-fleet-a", "again"),
            new_cluster("other", "fleet-b"),
        ];
        assert!(store.add(&added).is_ok());
        let Ok(reopened) = ClusterStore::open_at(&nested, now()) else {
            panic!("reopen");
        };
        let names: Vec<&str> = reopened
            .clusters()
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["fleet-a", "fleet-b"]);
        assert!(reopened.contains(&new_cluster("kind-fleet-b", "x").context));

        let mut store = reopened;
        let removed = store.remove(&ClusterId::new("fleet-a"));
        assert_eq!(
            removed.ok().flatten().map(|c| c.context),
            Some("kind-fleet-a".to_owned())
        );
        let Ok(reopened) = ClusterStore::open_at(&nested, now()) else {
            panic!("reopen");
        };
        assert_eq!(reopened.clusters().len(), 1);
        assert!(matches!(store.remove(&ClusterId::new("nope")), Ok(None)));
    }

    #[test]
    fn preferences_survive_a_reopen() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let mut store = ClusterStore::empty(dir.path());
        assert!(store.add(&[new_cluster("kind-fleet-a", "fleet-a")]).is_ok());
        let preferences = Preferences {
            namespace_selection: Some(NamespaceSelection::All),
            default_namespace: Some("payments".to_owned()),
            metrics: MetricsSource::Prometheus,
            confirm_destructive: false,
            notify: false,
            labels: vec!["env=dev".to_owned()],
        };
        let id = ClusterId::new("fleet-a");
        assert!(matches!(store.set_preferences(&id, &preferences), Ok(true)));
        assert!(matches!(
            store.set_preferences(&ClusterId::new("nope"), &preferences),
            Ok(false)
        ));
        let Ok(reopened) = ClusterStore::open_at(dir.path(), now()) else {
            panic!("reopen");
        };
        let saved = reopened.get(&id).map(|c| Preferences::from(&c.preferences));
        assert_eq!(saved, Some(preferences.clone()));
        let mut reopened = reopened;
        let named = Preferences {
            namespace_selection: Some(NamespaceSelection::Named("web".into())),
            ..preferences
        };
        assert!(reopened.set_preferences(&id, &named).is_ok());
        let reopened = ClusterStore::open_at(dir.path(), now())
            .unwrap_or_else(|error| panic!("reopen: {error}"));
        assert_eq!(
            reopened
                .get(&id)
                .map(|cluster| Preferences::from(&cluster.preferences)),
            Some(named)
        );
    }

    #[test]
    fn files_without_preferences_get_defaults() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let old = r#"{"clusters":[{"file":"/h/.kube/config","context":"a","name":"a"}]}"#;
        assert!(fs::write(dir.path().join(CLUSTERS_FILE), old).is_ok());
        let Ok(store) = ClusterStore::open_at(dir.path(), now()) else {
            panic!("open");
        };
        let saved = store
            .get(&ClusterId::new("a"))
            .map(|c| Preferences::from(&c.preferences));
        assert_eq!(saved, Some(Preferences::default()));
    }

    #[test]
    fn corrupt_file_is_set_aside_before_anything_is_written() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        assert!(fs::write(dir.path().join(CLUSTERS_FILE), "{not json").is_ok());
        let Ok(mut store) = ClusterStore::open_at(dir.path(), now()) else {
            panic!("open");
        };
        let kept = dir.path().join("clusters.json.corrupt-20261013T063600Z");
        assert_eq!(store.set_aside(), Some(kept.as_path()));
        assert_eq!(store.clusters(), &[]);
        assert!(store.add(&[new_cluster("kind-fleet-a", "fleet-a")]).is_ok());
        assert_eq!(fs::read_to_string(&kept).ok().as_deref(), Some("{not json"));
        let Ok(reopened) = ClusterStore::open_at(dir.path(), now()) else {
            panic!("reopen");
        };
        assert_eq!(reopened.clusters().len(), 1);
        assert_eq!(reopened.set_aside(), None);
    }

    #[test]
    fn watched_folders_survive_a_reopen_and_must_exist() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let folder = dir.path().join("team");
        assert!(fs::create_dir_all(&folder).is_ok());
        let support = dir.path().join("support");
        let mut store = ClusterStore::empty(&support);

        assert!(matches!(store.add_folder(&folder), Ok(true)));
        assert!(matches!(store.add_folder(&folder), Ok(false)));
        let missing = store.add_folder(&dir.path().join("nope"));
        assert!(matches!(missing, Err(Error::NotAFolder(_))), "{missing:?}");

        let Ok(reopened) = ClusterStore::open_at(&support, now()) else {
            panic!("reopen");
        };
        assert_eq!(reopened.folders(), [folder]);
        assert_eq!(reopened.kubeconfigs_dir(), support.join(KUBECONFIGS_DIR));
    }
}
