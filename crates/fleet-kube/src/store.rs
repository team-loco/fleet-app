use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use fleet_core::{ClusterId, NewCluster};
use serde::{Deserialize, Serialize};

use crate::discovery::ContextRef;
use crate::error::Result;

const APP_SUPPORT: &str = "Library/Application Support";
const APP_ID: &str = "dev.fleet.app";
const CLUSTERS_FILE: &str = "clusters.json";
const TEMP_SUFFIX: &str = "json.tmp";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedCluster {
    pub file: PathBuf,
    pub context: String,
    pub name: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterStore {
    path: PathBuf,
    clusters: Vec<SavedCluster>,
}

impl ClusterStore {
    pub fn default_dir(home: &Path) -> PathBuf {
        home.join(APP_SUPPORT).join(APP_ID)
    }

    pub fn open(dir: &Path) -> Result<Self> {
        let path = dir.join(CLUSTERS_FILE);
        let clusters = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<StoreFile>(&bytes)?.clusters,
            Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { path, clusters })
    }

    pub fn empty(dir: &Path) -> Self {
        Self {
            path: dir.join(CLUSTERS_FILE),
            clusters: Vec::new(),
        }
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

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let body = StoreFile {
            clusters: self.clusters.clone(),
        };
        let json = serde_json::to_vec_pretty(&body)?;
        let temp = self.path.with_extension(TEMP_SUFFIX);
        fs::write(&temp, json)?;
        fs::rename(&temp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let Ok(store) = ClusterStore::open(dir.path()) else {
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
        let Ok(reopened) = ClusterStore::open(&nested) else {
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
        let Ok(reopened) = ClusterStore::open(&nested) else {
            panic!("reopen");
        };
        assert_eq!(reopened.clusters().len(), 1);
        assert!(matches!(store.remove(&ClusterId::new("nope")), Ok(None)));
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        assert!(fs::write(dir.path().join(CLUSTERS_FILE), "{not json").is_ok());
        assert!(ClusterStore::open(dir.path()).is_err());
    }
}
