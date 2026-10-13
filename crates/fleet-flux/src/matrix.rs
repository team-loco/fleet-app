use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{GitOpsObject, ObjectIdentity, Status, Summary, summarize};

#[derive(Clone, Debug)]
pub struct ClusterObjects {
    pub cluster: String,
    pub online: bool,
    pub objects: Vec<GitOpsObject>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Cell {
    pub cluster: String,
    pub present: bool,
    pub revision: Option<String>,
    pub attempted_revision: Option<String>,
    pub summary: Summary,
    pub differs: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Row {
    pub identity: ObjectIdentity,
    pub cells: Vec<Cell>,
    pub differs: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MatrixError {
    #[error("duplicate cluster {0}")]
    DuplicateCluster(String),
    #[error("duplicate object in cluster {0}")]
    DuplicateObject(String),
}

pub fn rows(clusters: &[ClusterObjects]) -> Result<Vec<Row>, MatrixError> {
    let mut names = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut indexes = Vec::new();
    for cluster in clusters {
        if !names.insert(&cluster.cluster) {
            return Err(MatrixError::DuplicateCluster(cluster.cluster.clone()));
        }
        let mut index = BTreeMap::new();
        for object in &cluster.objects {
            if index.insert(&object.identity, object).is_some() {
                return Err(MatrixError::DuplicateObject(cluster.cluster.clone()));
            }
            identities.insert(object.identity.clone());
        }
        indexes.push(index);
    }
    let mut rows = Vec::new();
    for identity in identities {
        let revisions: BTreeSet<_> = indexes
            .iter()
            .zip(clusters)
            .filter(|(_, cluster)| cluster.online)
            .filter_map(|(index, _)| index.get(&identity).and_then(|object| object.revision()))
            .collect();
        let differs = revisions.len() > 1;
        let cells = clusters
            .iter()
            .zip(&indexes)
            .map(|(cluster, index)| {
                let object = index.get(&identity);
                let revision = object
                    .and_then(|object| object.revision())
                    .map(str::to_owned);
                let attempted_revision =
                    object.and_then(|object| object.last_attempted_revision.clone());
                let summary = if !cluster.online {
                    Summary {
                        status: Status::Offline,
                        sentence: "The cluster is offline.".into(),
                        detail: None,
                    }
                } else if let Some(object) = object {
                    let source = object.source_ref.as_ref().and_then(|reference| {
                        cluster.objects.iter().find(|candidate| {
                            candidate.identity.name == reference.name
                                && candidate.identity.namespace == reference.namespace
                                && candidate.identity.kind.as_str() == reference.kind
                        })
                    });
                    summarize(object, source)
                } else {
                    Summary {
                        status: Status::Offline,
                        sentence: "The object is not deployed on this cluster.".into(),
                        detail: None,
                    }
                };
                let cell_differs = differs && cluster.online && revision.is_some();
                Cell {
                    cluster: cluster.cluster.clone(),
                    present: object.is_some(),
                    revision,
                    attempted_revision,
                    summary,
                    differs: cell_differs,
                }
            })
            .collect();
        rows.push(Row {
            identity,
            cells,
            differs,
        });
    }
    Ok(rows)
}
