use std::sync::Arc;

use fleet_core::Status;
use fleet_flux::InventoryEntry;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use kube::Resource;

use crate::manager::ClusterHandle;

const DEPLOYMENT: &str = "Deployment";
const STATEFUL_SET: &str = "StatefulSet";
const DAEMON_SET: &str = "DaemonSet";
const DEFAULT_REPLICAS: i32 = 1;

pub struct Workloads {
    deployments: Vec<Arc<Deployment>>,
    stateful_sets: Vec<Arc<StatefulSet>>,
    daemon_sets: Vec<Arc<DaemonSet>>,
}

fn named<'a, K: Resource>(objects: &'a [Arc<K>], entry: &InventoryEntry) -> Option<&'a K> {
    objects
        .iter()
        .find(|object| {
            let meta = object.meta();
            meta.name.as_deref() == Some(entry.name.as_str())
                && meta.namespace.as_deref().unwrap_or_default() == entry.namespace
        })
        .map(AsRef::as_ref)
}

pub fn readiness(ready: i32, wanted: i32) -> (Status, String) {
    let status = if ready >= wanted {
        Status::Ok
    } else if ready == 0 {
        Status::Err
    } else {
        Status::Warn
    };
    (status, format!("{ready}/{wanted} ready"))
}

impl Workloads {
    pub fn of(handle: &ClusterHandle) -> Self {
        Self {
            deployments: handle.objects::<Deployment>(),
            stateful_sets: handle.objects::<StatefulSet>(),
            daemon_sets: handle.objects::<DaemonSet>(),
        }
    }

    pub fn health(&self, entry: &InventoryEntry) -> Option<(Status, String)> {
        match entry.kind.as_str() {
            DEPLOYMENT => {
                let found = named(&self.deployments, entry)?;
                let wanted = found
                    .spec
                    .as_ref()
                    .and_then(|spec| spec.replicas)
                    .unwrap_or(DEFAULT_REPLICAS);
                let ready = found.status.as_ref().and_then(|s| s.ready_replicas);
                Some(readiness(ready.unwrap_or_default(), wanted))
            }
            STATEFUL_SET => {
                let found = named(&self.stateful_sets, entry)?;
                let wanted = found
                    .spec
                    .as_ref()
                    .and_then(|spec| spec.replicas)
                    .unwrap_or(DEFAULT_REPLICAS);
                let ready = found.status.as_ref().and_then(|s| s.ready_replicas);
                Some(readiness(ready.unwrap_or_default(), wanted))
            }
            DAEMON_SET => {
                let found = named(&self.daemon_sets, entry)?;
                let status = found.status.as_ref()?;
                Some(readiness(
                    status.number_ready,
                    status.desired_number_scheduled,
                ))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_grades_partial_and_empty_rollouts() {
        assert_eq!(readiness(3, 3), (Status::Ok, "3/3 ready".to_owned()));
        assert_eq!(readiness(2, 3), (Status::Warn, "2/3 ready".to_owned()));
        assert_eq!(readiness(0, 1), (Status::Err, "0/1 ready".to_owned()));
        assert_eq!(readiness(0, 0), (Status::Ok, "0/0 ready".to_owned()));
    }
}
