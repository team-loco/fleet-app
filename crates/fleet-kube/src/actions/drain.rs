use std::time::Duration;

use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::ResourceExt;
use kube::api::{EvictParams, ListParams};

use super::failure::{ActionFailureKind, action_failure};
use crate::error::{Error, Result};
use crate::manager::ClusterHandle;

pub const MIRROR_ANNOTATION: &str = "kubernetes.io/config.mirror";
const DAEMON_SET: &str = "DaemonSet";
const NODE_FIELD: &str = "spec.nodeName";
const EVICTION_RETRY: Duration = Duration::from_secs(5);
const EVICTION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    pub namespace: String,
    pub name: String,
}

fn daemon_owned(pod: &Pod) -> bool {
    pod.owner_references()
        .iter()
        .any(|owner| owner.controller == Some(true) && owner.kind == DAEMON_SET)
}

fn mirrored(pod: &Pod) -> bool {
    pod.annotations().contains_key(MIRROR_ANNOTATION)
}

pub fn drain_candidates(pods: &[Pod]) -> Vec<&Pod> {
    pods.iter()
        .filter(|pod| pod.metadata.deletion_timestamp.is_none())
        .filter(|pod| !daemon_owned(pod))
        .filter(|pod| !mirrored(pod))
        .collect()
}

async fn evict(pods: &Api<Pod>, name: &str) -> Result<bool> {
    let deadline = tokio::time::Instant::now() + EVICTION_TIMEOUT;
    loop {
        let error = match pods.evict(name, &EvictParams::default()).await {
            Ok(_) => return Ok(true),
            Err(error) => Error::Kube(error),
        };
        match action_failure(&error).kind {
            ActionFailureKind::NotFound => return Ok(true),
            ActionFailureKind::Blocked => {}
            _ => return Err(error),
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(EVICTION_RETRY).await;
    }
}

pub async fn drain(handle: &ClusterHandle, node: &str) -> Result<Vec<Blocked>> {
    let client = handle.client()?;
    let all: Api<Pod> = Api::all(client.clone());
    let selector = format!("{NODE_FIELD}={node}");
    let listed = all.list(&ListParams::default().fields(&selector)).await?;
    let mut blocked = Vec::new();
    for pod in drain_candidates(&listed.items) {
        let namespace = pod.namespace().unwrap_or_default();
        let name = pod.name_any();
        let pods: Api<Pod> = Api::namespaced(client.clone(), &namespace);
        if !evict(&pods, &name).await? {
            blocked.push(Blocked { namespace, name });
        }
    }
    Ok(blocked)
}

pub fn blocked_line(blocked: &[Blocked]) -> String {
    let names: Vec<String> = blocked
        .iter()
        .map(|pod| format!("{}/{}", pod.namespace, pod.name))
        .collect();
    let noun = if blocked.len() == 1 { "pod" } else { "pods" };
    format!(
        "{} {noun} blocked by a disruption budget: {}",
        blocked.len(),
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference, Time};

    use super::*;

    fn pod(name: &str) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.into()),
                namespace: Some("apps".into()),
                ..ObjectMeta::default()
            },
            ..Pod::default()
        }
    }

    fn owned(name: &str, kind: &str) -> Pod {
        let mut pod = pod(name);
        pod.metadata.owner_references = Some(vec![OwnerReference {
            kind: kind.into(),
            controller: Some(true),
            ..OwnerReference::default()
        }]);
        pod
    }

    #[test]
    fn drain_skips_daemon_sets_mirrors_and_terminating_pods() {
        let mut mirror = pod("kube-apiserver-n1");
        mirror.metadata.annotations = Some(BTreeMap::from([(
            MIRROR_ANNOTATION.to_owned(),
            "x".to_owned(),
        )]));
        let mut leaving = pod("leaving");
        leaving.metadata.deletion_timestamp = Some(Time(jiff::Timestamp::UNIX_EPOCH));
        let pods = vec![
            owned("web-1", "ReplicaSet"),
            owned("fluent-bit-x", DAEMON_SET),
            mirror,
            leaving,
            pod("bare"),
            owned("db-0", "StatefulSet"),
        ];
        let names: Vec<String> = drain_candidates(&pods)
            .iter()
            .map(|pod| pod.name_any())
            .collect();
        assert_eq!(names, ["web-1", "bare", "db-0"]);
    }

    #[test]
    fn blocked_evictions_are_listed() {
        let blocked = vec![Blocked {
            namespace: "payments".into(),
            name: "checkout-1".into(),
        }];
        assert_eq!(
            blocked_line(&blocked),
            "1 pod blocked by a disruption budget: payments/checkout-1"
        );
    }
}
