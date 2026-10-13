use std::collections::BTreeMap;
use std::sync::Arc;

use fleet_core::home::ClusterMetrics;
use fleet_core::{ClusterId, ClusterSummary, Counts, Page, Status};
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Event, Namespace, Node, PersistentVolumeClaim, Pod, Secret, Service,
};
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::PartialObjectMeta;

use crate::manager::{ClusterHandle, ConnectionStatus};
use crate::provider::region;
use crate::resources::WatchKind;

const POD_FAILED: &str = "Failed";
const POD_SUCCEEDED: &str = "Succeeded";
const POD_PENDING: &str = "Pending";
const READY_CONDITION: &str = "Ready";
const CONDITION_TRUE: &str = "True";
pub const FAILING_WAIT_REASONS: [&str; 7] = [
    "CrashLoopBackOff",
    "ImagePullBackOff",
    "ErrImagePull",
    "CreateContainerConfigError",
    "CreateContainerError",
    "InvalidImageName",
    "RunContainerError",
];
const STATE_CONNECTING: &str = "Connecting";
const STATE_HEALTHY: &str = "Healthy";
const STATE_WARNINGS: &str = "Warnings";
const STATE_ERRORS: &str = "Errors";
const STATE_DEGRADED: &str = "Degraded";
const STATE_AUTH_EXPIRED: &str = "Auth expired";
const STATE_OFFLINE: &str = "Offline";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodHealth {
    Healthy,
    Warning,
    Failing,
}

pub fn pod_health(pod: &Pod) -> PodHealth {
    let Some(status) = pod.status.as_ref() else {
        return PodHealth::Warning;
    };
    let phase = status.phase.as_deref().unwrap_or_default();
    if phase == POD_SUCCEEDED {
        return PodHealth::Healthy;
    }
    if phase == POD_FAILED {
        return PodHealth::Failing;
    }
    let waiting_failure = status
        .container_statuses
        .iter()
        .flatten()
        .chain(status.init_container_statuses.iter().flatten())
        .filter_map(|container| {
            container
                .state
                .as_ref()?
                .waiting
                .as_ref()?
                .reason
                .as_deref()
        })
        .any(|reason| FAILING_WAIT_REASONS.contains(&reason));
    if waiting_failure {
        return PodHealth::Failing;
    }
    let ready =
        status.conditions.iter().flatten().any(|condition| {
            condition.type_ == READY_CONDITION && condition.status == CONDITION_TRUE
        });
    if phase == POD_PENDING || !ready {
        return PodHealth::Warning;
    }
    PodHealth::Healthy
}

fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

pub fn pod_counts(pods: &[Arc<Pod>]) -> Counts {
    let healths: Vec<PodHealth> = pods.iter().map(|pod| pod_health(pod)).collect();
    let failing = healths.iter().filter(|h| **h == PodHealth::Failing).count();
    let warning = healths.iter().filter(|h| **h == PodHealth::Warning).count();
    Counts::total(count(pods.len()))
        .with_err(count(failing))
        .with_warn(count(warning))
}

pub fn node_ready(node: &Node) -> bool {
    node.status
        .as_ref()
        .and_then(|status| status.conditions.as_ref())
        .into_iter()
        .flatten()
        .any(|condition| condition.type_ == READY_CONDITION && condition.status == CONDITION_TRUE)
}

pub fn schedulable(node: &Node) -> bool {
    !node
        .spec
        .as_ref()
        .and_then(|spec| spec.unschedulable)
        .unwrap_or_default()
}

pub fn node_counts(nodes: &[Arc<Node>]) -> Counts {
    let not_ready = nodes.iter().filter(|node| !node_ready(node)).count();
    Counts::total(count(nodes.len())).with_warn(count(not_ready))
}

pub fn cluster_state(status: &ConnectionStatus, problems: Counts) -> (Status, &'static str) {
    match status {
        ConnectionStatus::Connecting => (Status::Progress, STATE_CONNECTING),
        ConnectionStatus::Connected => match problems.status() {
            Status::Err => (Status::Err, STATE_ERRORS),
            Status::Warn => (Status::Warn, STATE_WARNINGS),
            _ => (Status::Ok, STATE_HEALTHY),
        },
        ConnectionStatus::Degraded(_) => (Status::Warn, STATE_DEGRADED),
        ConnectionStatus::AuthExpired(_) => (Status::Err, STATE_AUTH_EXPIRED),
        ConnectionStatus::Offline(_) => (Status::Offline, STATE_OFFLINE),
    }
}

pub fn page_kind(page: Page) -> Option<WatchKind> {
    match page {
        Page::Nodes => Some(WatchKind::Nodes),
        Page::Namespaces => Some(WatchKind::Namespaces),
        Page::Events => Some(WatchKind::Events),
        Page::Pods => Some(WatchKind::Pods),
        Page::Deployments => Some(WatchKind::Deployments),
        Page::StatefulSets => Some(WatchKind::StatefulSets),
        Page::DaemonSets => Some(WatchKind::DaemonSets),
        Page::Jobs => Some(WatchKind::Jobs),
        Page::CronJobs => Some(WatchKind::CronJobs),
        Page::Services => Some(WatchKind::Services),
        Page::Ingresses => Some(WatchKind::Ingresses),
        Page::ConfigMaps => Some(WatchKind::ConfigMaps),
        Page::Secrets => Some(WatchKind::Secrets),
        Page::Pvcs => Some(WatchKind::PersistentVolumeClaims),
        _ => None,
    }
}

pub const OVERVIEW_KINDS: [WatchKind; 7] = [
    WatchKind::Namespaces,
    WatchKind::Events,
    WatchKind::Deployments,
    WatchKind::StatefulSets,
    WatchKind::DaemonSets,
    WatchKind::Jobs,
    WatchKind::CronJobs,
];

pub fn page_kinds(page: Page) -> Vec<WatchKind> {
    match page {
        Page::ClusterOverview => OVERVIEW_KINDS.to_vec(),
        Page::Pods => vec![WatchKind::Pods, WatchKind::Events],
        other => page_kind(other).into_iter().collect(),
    }
}

fn kind_total(handle: &ClusterHandle, kind: &WatchKind) -> Option<u32> {
    let len = match kind {
        WatchKind::Namespaces => handle.objects::<Namespace>().len(),
        WatchKind::Events => handle.objects::<Event>().len(),
        WatchKind::Deployments => handle.objects::<Deployment>().len(),
        WatchKind::StatefulSets => handle.objects::<StatefulSet>().len(),
        WatchKind::DaemonSets => handle.objects::<DaemonSet>().len(),
        WatchKind::ReplicaSets => handle.objects::<ReplicaSet>().len(),
        WatchKind::Jobs => handle.objects::<Job>().len(),
        WatchKind::CronJobs => handle.objects::<CronJob>().len(),
        WatchKind::Services => handle.objects::<Service>().len(),
        WatchKind::Ingresses => handle.objects::<Ingress>().len(),
        WatchKind::ConfigMaps => handle.objects::<PartialObjectMeta<ConfigMap>>().len(),
        WatchKind::Secrets => handle.objects::<PartialObjectMeta<Secret>>().len(),
        WatchKind::PersistentVolumeClaims => handle.objects::<PersistentVolumeClaim>().len(),
        WatchKind::Nodes => return None,
        WatchKind::Pods => return None,
        WatchKind::Custom(_) => return None,
    };
    Some(count(len))
}

pub fn summarize(
    id: ClusterId,
    name: String,
    handle: &ClusterHandle,
    problems: Counts,
    metrics: Option<ClusterMetrics>,
) -> ClusterSummary {
    let context = handle.context();
    let info = handle.info();
    let connection = handle.status();
    let pods = handle.objects::<Pod>();
    let nodes = handle.objects::<Node>();
    let (status, state) = cluster_state(&connection, problems);
    let mut nav = BTreeMap::from([
        (Page::Pods, pod_counts(&pods)),
        (Page::Nodes, node_counts(&nodes)),
    ]);
    for kind in handle.watched_kinds() {
        let page = kind_page(&kind);
        let total = kind_total(handle, &kind);
        if let (Some(page), Some(total)) = (page, total) {
            nav.insert(page, Counts::total(total));
        }
    }
    let synced_nodes = !nodes.is_empty();
    ClusterSummary {
        id,
        name,
        status,
        state: state.to_owned(),
        distro: info.provider.label().to_owned(),
        region: region(
            info.provider,
            &context.id.context,
            context.server.as_deref(),
        ),
        version: info.version,
        nodes: synced_nodes.then(|| count(nodes.len())),
        problems,
        nav,
        metrics,
    }
}

pub fn kind_page(kind: &WatchKind) -> Option<Page> {
    let pages = [
        Page::Nodes,
        Page::Namespaces,
        Page::Events,
        Page::Pods,
        Page::Deployments,
        Page::StatefulSets,
        Page::DaemonSets,
        Page::Jobs,
        Page::CronJobs,
        Page::Services,
        Page::Ingresses,
        Page::ConfigMaps,
        Page::Secrets,
        Page::Pvcs,
    ];
    pages
        .into_iter()
        .find(|page| page_kind(*page).as_ref() == Some(kind))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::failure::{Failure, FailureKind};

    fn pod(status: &serde_json::Value) -> Arc<Pod> {
        let parsed = serde_json::from_value(json!({
            "metadata": { "name": "p", "namespace": "n" },
            "status": status
        }));
        let Ok(pod) = parsed else {
            panic!("pod fixture");
        };
        Arc::new(pod)
    }

    fn ready() -> serde_json::Value {
        json!({ "phase": "Running", "conditions": [{ "type": "Ready", "status": "True" }] })
    }

    #[test]
    fn classifies_pods() {
        let crash = pod(&json!({
            "phase": "Running",
            "containerStatuses": [{
                "name": "c", "image": "i", "imageID": "", "ready": false, "restartCount": 5,
                "state": { "waiting": { "reason": "CrashLoopBackOff" } }
            }]
        }));
        let pending = pod(&json!({ "phase": "Pending" }));
        let done = pod(&json!({ "phase": "Succeeded" }));
        let failed = pod(&json!({ "phase": "Failed" }));
        assert_eq!(pod_health(&crash), PodHealth::Failing);
        assert_eq!(pod_health(&pending), PodHealth::Warning);
        assert_eq!(pod_health(&done), PodHealth::Healthy);
        assert_eq!(pod_health(&pod(&ready())), PodHealth::Healthy);
        let counts = pod_counts(&[crash, pending, done, failed, pod(&ready())]);
        assert_eq!(counts, Counts::total(5).with_err(2).with_warn(1));
    }

    #[test]
    fn connection_states_match_design_words() {
        let failure = Failure {
            kind: FailureKind::ExpiredCredentials,
            detail: String::new(),
            message: String::new(),
            remedy: None,
        };
        assert_eq!(
            cluster_state(
                &ConnectionStatus::AuthExpired(failure.clone()),
                Counts::default()
            ),
            (Status::Err, "Auth expired")
        );
        assert_eq!(
            cluster_state(&ConnectionStatus::Offline(failure), Counts::default()),
            (Status::Offline, "Offline")
        );
        assert_eq!(
            cluster_state(&ConnectionStatus::Connected, Counts::default()),
            (Status::Ok, "Healthy")
        );
        assert_eq!(
            cluster_state(&ConnectionStatus::Connected, Counts::default().with_warn(2)),
            (Status::Warn, "Warnings")
        );
        assert_eq!(
            cluster_state(&ConnectionStatus::Connecting, Counts::default()),
            (Status::Progress, "Connecting")
        );
    }

    #[test]
    fn pages_round_trip_to_kinds() {
        assert_eq!(
            page_kind(Page::Pvcs),
            Some(WatchKind::PersistentVolumeClaims)
        );
        assert_eq!(kind_page(&WatchKind::Deployments), Some(Page::Deployments));
        assert_eq!(page_kind(Page::GitOps), None);
        assert_eq!(kind_page(&WatchKind::ReplicaSets), None);
        assert_eq!(
            page_kinds(Page::ClusterOverview).len(),
            OVERVIEW_KINDS.len()
        );
        assert_eq!(page_kinds(Page::Secrets), [WatchKind::Secrets]);
        assert_eq!(page_kinds(Page::Changes), []);
    }
}
