use std::time::Duration;

use chrono::{DateTime, Utc};
use fleet_core::ResourceRef;
use fleet_core::flux::{FluxAction, FluxKey};
use fleet_flux::{GitOpsObject, ObjectKind, actions};
use kube::api::PropagationPolicy;
use serde_json::{Value, json};

use super::failure::{ActionFailureKind, action_failure};
use crate::error::{Error, Result};
use crate::manager::ClusterHandle;
use crate::ops::{ObjectRef, PatchBody};

const NODE: &str = "Node";
const DEPLOYMENT: &str = "Deployment";
const STATEFUL_SET: &str = "StatefulSet";
const DAEMON_SET: &str = "DaemonSet";
const REPLICA_SET: &str = "ReplicaSet";
const DELETE_POLL: Duration = Duration::from_millis(500);
const DELETE_WAIT: Duration = Duration::from_secs(60);
const BUILTIN: [(&str, &str); 16] = [
    ("Pod", "v1"),
    ("Service", "v1"),
    ("ConfigMap", "v1"),
    ("Secret", "v1"),
    ("PersistentVolumeClaim", "v1"),
    ("Node", "v1"),
    ("Namespace", "v1"),
    ("ServiceAccount", "v1"),
    ("Deployment", "apps/v1"),
    ("StatefulSet", "apps/v1"),
    ("DaemonSet", "apps/v1"),
    ("ReplicaSet", "apps/v1"),
    ("Job", "batch/v1"),
    ("CronJob", "batch/v1"),
    ("Ingress", "networking.k8s.io/v1"),
    ("HorizontalPodAutoscaler", "autoscaling/v2"),
];
const FLUX_KINDS: [ObjectKind; 6] = [
    ObjectKind::Kustomization,
    ObjectKind::HelmRelease,
    ObjectKind::GitRepository,
    ObjectKind::HelmRepository,
    ObjectKind::HelmChart,
    ObjectKind::OCIRepository,
];
const FOREGROUND_KINDS: [&str; 6] = [
    DEPLOYMENT,
    STATEFUL_SET,
    DAEMON_SET,
    REPLICA_SET,
    "Job",
    "CronJob",
];

fn flux_kind(kind: &str) -> Option<ObjectKind> {
    FLUX_KINDS
        .into_iter()
        .find(|candidate| candidate.as_str() == kind)
}

pub fn api_version(kind: &str) -> Option<&'static str> {
    let builtin = BUILTIN
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, version)| *version);
    builtin.or_else(|| flux_kind(kind).map(ObjectKind::api_version))
}

pub fn propagation(kind: &str) -> PropagationPolicy {
    if FOREGROUND_KINDS.contains(&kind) {
        PropagationPolicy::Foreground
    } else {
        PropagationPolicy::Background
    }
}

pub fn object_ref(target: &ResourceRef) -> Result<ObjectRef> {
    let api_version = api_version(&target.kind).ok_or_else(|| Error::UnknownKind {
        api_version: String::new(),
        kind: target.kind.clone(),
    })?;
    Ok(ObjectRef {
        api_version: api_version.to_owned(),
        kind: target.kind.clone(),
        namespace: target.namespace.clone(),
        name: target.name.clone(),
    })
}

fn require(action: &'static str, target: &ResourceRef, kinds: &[&str]) -> Result<()> {
    if kinds.contains(&target.kind.as_str()) {
        return Ok(());
    }
    Err(Error::Unsupported {
        action,
        kind: target.kind.clone(),
    })
}

pub fn cordon_patch(cordoned: bool) -> Value {
    let unschedulable = if cordoned { json!(true) } else { Value::Null };
    json!({ "spec": { "unschedulable": unschedulable } })
}

pub async fn cordon(handle: &ClusterHandle, target: &ResourceRef, cordoned: bool) -> Result<()> {
    require("Cordon", target, &[NODE])?;
    let node = object_ref(target)?;
    handle
        .patch(&node, PatchBody::Merge(cordon_patch(cordoned)))
        .await?;
    Ok(())
}

pub async fn restart(handle: &ClusterHandle, target: &ResourceRef) -> Result<()> {
    require("Restart", target, &[DEPLOYMENT, STATEFUL_SET, DAEMON_SET])?;
    handle.rollout_restart(&object_ref(target)?).await?;
    Ok(())
}

pub async fn scale(handle: &ClusterHandle, target: &ResourceRef, replicas: u32) -> Result<()> {
    require("Scale", target, &[DEPLOYMENT, STATEFUL_SET, REPLICA_SET])?;
    let replicas = i32::try_from(replicas).unwrap_or(i32::MAX);
    handle.scale(&object_ref(target)?, replicas).await
}

pub async fn delete(handle: &ClusterHandle, target: &ResourceRef) -> Result<()> {
    let object = object_ref(target)?;
    handle.delete(&object, propagation(&target.kind)).await?;
    let deadline = tokio::time::Instant::now() + DELETE_WAIT;
    while tokio::time::Instant::now() < deadline {
        match handle.get(&object).await {
            Ok(_) => tokio::time::sleep(DELETE_POLL).await,
            Err(error) if action_failure(&error).kind == ActionFailureKind::NotFound => {
                return Ok(());
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn flux_patch(action: FluxAction, at: DateTime<Utc>) -> Value {
    match action {
        FluxAction::Reconcile => actions::reconcile(at),
        FluxAction::ReconcileWithSource => actions::reconcile(at),
        FluxAction::Force => actions::force(at),
        FluxAction::Suspend => actions::suspend(),
        FluxAction::Resume => actions::resume(),
    }
}

fn flux_ref(kind: ObjectKind, namespace: &str, name: &str) -> ObjectRef {
    ObjectRef {
        api_version: kind.api_version().to_owned(),
        kind: kind.as_str().to_owned(),
        namespace: Some(namespace.to_owned()),
        name: name.to_owned(),
    }
}

fn now() -> DateTime<Utc> {
    let now = jiff::Timestamp::now();
    let nanos = u32::try_from(now.subsec_nanosecond()).unwrap_or_default();
    DateTime::from_timestamp(now.as_second(), nanos).unwrap_or_default()
}

pub async fn flux(handle: &ClusterHandle, key: &FluxKey, action: FluxAction) -> Result<()> {
    let kind = flux_kind(key.kind.label()).ok_or_else(|| Error::Unsupported {
        action: "Reconcile",
        kind: key.kind.label().to_owned(),
    })?;
    let target = flux_ref(kind, &key.namespace, &key.name);
    let at = now();
    if action == FluxAction::ReconcileWithSource {
        let object = handle.get(&target).await?;
        let parsed = GitOpsObject::from_json(&serde_json::to_value(&object)?)?;
        let source = parsed
            .source_ref
            .and_then(|source| Some((flux_kind(&source.kind)?, source)));
        if let Some((source_kind, source)) = source {
            let source = flux_ref(source_kind, &source.namespace, &source.name);
            handle
                .patch(&source, PatchBody::Merge(actions::reconcile(at)))
                .await?;
        }
    }
    handle
        .patch(&target, PatchBody::Merge(flux_patch(action, at)))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use fleet_core::ClusterId;

    use super::*;

    fn target(kind: &str) -> ResourceRef {
        ResourceRef {
            cluster: ClusterId::new("kind-fleet-a"),
            kind: kind.into(),
            namespace: Some("apps".into()),
            name: "podinfo".into(),
        }
    }

    #[test]
    fn kinds_map_to_api_versions() {
        assert_eq!(api_version("Deployment"), Some("apps/v1"));
        assert_eq!(api_version("ConfigMap"), Some("v1"));
        assert_eq!(
            api_version("HelmRelease"),
            Some("helm.toolkit.fluxcd.io/v2")
        );
        assert_eq!(api_version("Widget"), None);
        assert!(object_ref(&target("Widget")).is_err());
    }

    #[test]
    fn workloads_delete_in_the_foreground() {
        assert_eq!(
            format!("{:?}", propagation("Deployment")),
            format!("{:?}", PropagationPolicy::Foreground)
        );
        assert_eq!(
            format!("{:?}", propagation("ConfigMap")),
            format!("{:?}", PropagationPolicy::Background)
        );
    }

    #[test]
    fn cordon_sets_and_clears_unschedulable() {
        assert_eq!(cordon_patch(true), json!({"spec": {"unschedulable": true}}));
        assert_eq!(
            cordon_patch(false),
            json!({"spec": {"unschedulable": null}})
        );
    }

    #[test]
    fn actions_reject_kinds_they_do_not_apply_to() {
        assert!(matches!(
            require("Scale", &target("DaemonSet"), &[DEPLOYMENT, STATEFUL_SET]),
            Err(Error::Unsupported {
                action: "Scale",
                ..
            })
        ));
        assert!(require("Scale", &target("Deployment"), &[DEPLOYMENT]).is_ok());
    }

    #[test]
    fn flux_actions_patch_annotations_or_suspend() {
        let at = DateTime::from_timestamp(0, 0).unwrap_or_default();
        assert_eq!(
            flux_patch(FluxAction::Suspend, at),
            json!({"spec": {"suspend": true}})
        );
        let reconcile = flux_patch(FluxAction::Reconcile, at);
        assert!(reconcile["metadata"]["annotations"][actions::REQUESTED_AT].is_string());
    }
}
