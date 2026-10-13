use std::time::Duration;

use chrono::{DateTime, Utc};
use fleet_core::flux::{FluxAction, FluxKey};
use fleet_core::{ResourceRef, Seen, Uid};
use fleet_flux::{GitOpsObject, ObjectKind, actions};
use kube::api::PropagationPolicy;
use serde_json::{Value, json};

use super::failure::{ActionFailureKind, action_failure};
use crate::error::{Error, Result};
use crate::manager::ClusterHandle;
use crate::ops::{ObjectRef, PatchBody, restart_patch};

const NODE: &str = "Node";
const DEPLOYMENT: &str = "Deployment";
const STATEFUL_SET: &str = "StatefulSet";
const DAEMON_SET: &str = "DaemonSet";
const REPLICA_SET: &str = "ReplicaSet";
const TEMPLATE_PATH: &str = "/spec/template";
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

fn known(uid: &Uid) -> Result<&Uid> {
    if uid.as_str().is_empty() {
        return Err(Error::MissingField("metadata.uid"));
    }
    Ok(uid)
}

fn uid_of(object: &kube::api::DynamicObject) -> Option<&str> {
    object.metadata.uid.as_deref()
}

pub(crate) async fn replaced(handle: &ClusterHandle, object: &ObjectRef, uid: &Uid) -> bool {
    match handle.get(object).await {
        Ok(found) => uid_of(&found) != Some(uid.as_str()),
        Err(_) => false,
    }
}

pub(crate) async fn explain(
    handle: &ClusterHandle,
    object: &ObjectRef,
    uid: &Uid,
    error: Error,
) -> Error {
    if replaced(handle, object, uid).await {
        return Error::Replaced {
            kind: object.kind.clone(),
            name: object.name.clone(),
        };
    }
    error
}

async fn patch(handle: &ClusterHandle, seen: &Seen, body: PatchBody) -> Result<()> {
    let uid = known(&seen.uid)?;
    let object = object_ref(&seen.target)?;
    match handle.patch(&object, body.guarded(uid)).await {
        Ok(_) => Ok(()),
        Err(error) => Err(explain(handle, &object, uid, error).await),
    }
}

pub async fn cordon(handle: &ClusterHandle, seen: &Seen, cordoned: bool) -> Result<()> {
    require("Cordon", &seen.target, &[NODE])?;
    patch(handle, seen, PatchBody::Merge(cordon_patch(cordoned))).await
}

pub async fn restart(handle: &ClusterHandle, seen: &Seen) -> Result<()> {
    require(
        "Restart",
        &seen.target,
        &[DEPLOYMENT, STATEFUL_SET, DAEMON_SET],
    )?;
    let body = restart_patch(jiff::Timestamp::now());
    patch(handle, seen, PatchBody::Merge(body)).await
}

pub async fn scale(handle: &ClusterHandle, seen: &Seen, replicas: u32) -> Result<()> {
    require(
        "Scale",
        &seen.target,
        &[DEPLOYMENT, STATEFUL_SET, REPLICA_SET],
    )?;
    let uid = known(&seen.uid)?;
    let replicas = i32::try_from(replicas).unwrap_or(i32::MAX);
    let object = object_ref(&seen.target)?;
    match handle.scale(&object, replicas, uid).await {
        Ok(()) => Ok(()),
        Err(error) => Err(explain(handle, &object, uid, error).await),
    }
}

pub fn template_patch(template: &Value) -> Value {
    json!([{ "op": "replace", "path": TEMPLATE_PATH, "value": template }])
}

pub async fn replace_template(
    handle: &ClusterHandle,
    target: &ResourceRef,
    template: &Value,
) -> Result<()> {
    require("Apply", target, &[DEPLOYMENT, STATEFUL_SET, DAEMON_SET])?;
    handle
        .patch(
            &object_ref(target)?,
            PatchBody::Json(template_patch(template)),
        )
        .await?;
    Ok(())
}

pub async fn delete(handle: &ClusterHandle, seen: &Seen) -> Result<()> {
    let uid = known(&seen.uid)?;
    let object = object_ref(&seen.target)?;
    let propagation = propagation(&seen.target.kind);
    if let Err(error) = handle.delete(&object, propagation, Some(uid)).await {
        return Err(explain(handle, &object, uid, error).await);
    }
    let deadline = tokio::time::Instant::now() + DELETE_WAIT;
    while tokio::time::Instant::now() < deadline {
        match handle.get(&object).await {
            Ok(found) if uid_of(&found) != Some(uid.as_str()) => return Ok(()),
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

pub async fn flux(
    handle: &ClusterHandle,
    key: &FluxKey,
    uid: &Uid,
    action: FluxAction,
) -> Result<()> {
    let uid = known(uid)?;
    let kind = flux_kind(key.kind.label()).ok_or_else(|| Error::Unsupported {
        action: "Reconcile",
        kind: key.kind.label().to_owned(),
    })?;
    let target = flux_ref(kind, &key.namespace, &key.name);
    let at = now();
    if action == FluxAction::ReconcileWithSource {
        let object = handle.get(&target).await?;
        if uid_of(&object) != Some(uid.as_str()) {
            return Err(Error::Replaced {
                kind: target.kind,
                name: target.name,
            });
        }
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
    let body = PatchBody::Merge(flux_patch(action, at)).guarded(uid);
    match handle.patch(&target, body).await {
        Ok(_) => Ok(()),
        Err(error) => Err(explain(handle, &target, uid, error).await),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use fleet_core::ClusterId;
    use k8s_openapi::api::apps::v1::Deployment;
    use k8s_openapi::api::core::v1::ConfigMap;
    use kube::core::ApiResource;

    use super::*;
    use crate::manager::lock;

    const SEEN: &str = "seen-uid";
    const RECREATED: &str = "recreated-uid";
    const CONFIGMAP_PATH: &str = "/api/v1/namespaces/apps/configmaps/podinfo";
    const SCALE_PATH: &str = "/apis/apps/v1/namespaces/apps/deployments/podinfo/scale";
    const CONFLICT: &str = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","reason":"Conflict","code":409,"message":"Precondition failed: UID mismatch"}"#;

    fn object(kind: &str, uid: &str) -> String {
        json!({
            "apiVersion": api_version(kind),
            "kind": kind,
            "metadata": { "name": "podinfo", "namespace": "apps", "uid": uid }
        })
        .to_string()
    }

    fn sent_uid(body: &Value, pointer: &str) -> Option<String> {
        body.pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    fn recreated(
        kind: &'static str,
    ) -> (
        crate::manager::ClusterManager,
        ClusterHandle,
        Arc<crate::fake::Traffic>,
        Arc<Mutex<bool>>,
    ) {
        let changed = Arc::new(Mutex::new(false));
        let state = changed.clone();
        let (client, traffic) =
            crate::fake::client_reading_bodies(Duration::ZERO, move |method, _, body| {
                let current = object(kind, RECREATED);
                if method == http::Method::GET {
                    return Some((http::StatusCode::OK, current));
                }
                let wanted = sent_uid(body, "/preconditions/uid")
                    .or_else(|| sent_uid(body, "/metadata/uid"));
                if wanted.as_deref() != Some(RECREATED) {
                    return Some((http::StatusCode::CONFLICT, CONFLICT.to_owned()));
                }
                *lock(&state) = true;
                Some((http::StatusCode::OK, current))
            });
        let (manager, handle) = crate::fake::handle(&client);
        let mut resources = lock(&handle.shared.resources);
        resources.insert(
            ("v1".to_owned(), "ConfigMap".to_owned()),
            ApiResource::erase::<ConfigMap>(&()),
        );
        resources.insert(
            ("apps/v1".to_owned(), "Deployment".to_owned()),
            ApiResource::erase::<Deployment>(&()),
        );
        drop(resources);
        (manager, handle, traffic, changed)
    }

    #[tokio::test]
    async fn a_confirmed_delete_never_removes_a_recreated_object() {
        let (_manager, handle, traffic, changed) = recreated("ConfigMap");
        let seen = Seen::new(target("ConfigMap"), Uid::new(SEEN));
        let outcome = delete(&handle, &seen).await;
        assert!(
            matches!(outcome, Err(Error::Replaced { .. })),
            "{outcome:?}"
        );
        assert!(!*lock(&changed));
        let sent: Vec<Option<String>> = traffic
            .bodies(&http::Method::DELETE, CONFIGMAP_PATH)
            .iter()
            .map(|body| sent_uid(body, "/preconditions/uid"))
            .collect();
        assert_eq!(sent, [Some(SEEN.to_owned())]);
    }

    #[tokio::test]
    async fn a_confirmed_scale_never_resizes_a_recreated_workload() {
        let (_manager, handle, traffic, changed) = recreated("Deployment");
        let seen = Seen::new(target("Deployment"), Uid::new(SEEN));
        let outcome = scale(&handle, &seen, 5).await;
        assert!(
            matches!(outcome, Err(Error::Replaced { .. })),
            "{outcome:?}"
        );
        assert!(!*lock(&changed));
        let sent: Vec<Option<String>> = traffic
            .bodies(&http::Method::PATCH, SCALE_PATH)
            .iter()
            .map(|body| sent_uid(body, "/metadata/uid"))
            .collect();
        assert_eq!(sent, [Some(SEEN.to_owned())]);
    }

    #[tokio::test]
    async fn an_unknown_uid_is_refused_before_any_request() {
        let (_manager, handle, traffic, _) = recreated("ConfigMap");
        let seen = Seen::new(target("ConfigMap"), Uid::default());
        assert!(matches!(
            delete(&handle, &seen).await,
            Err(Error::MissingField("metadata.uid"))
        ));
        assert_eq!(traffic.requests(), Vec::new());
    }

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

    #[test]
    fn template_patch_replaces_the_whole_pod_template() {
        let template = json!({ "spec": { "containers": [{ "name": "api", "image": "api:1" }] } });
        assert_eq!(
            template_patch(&template),
            json!([{ "op": "replace", "path": "/spec/template", "value": template }])
        );
    }
}
