use std::time::Duration;

use fleet_core::home::{Problem, Revision};
use fleet_core::{ClusterId, Counts, ResourceRef, Status};
use fleet_flux::{GitOpsObject, ObjectKind, summarize};
use jiff::Timestamp;
use kube::api::DynamicObject;
use serde_json::Value;

use crate::clock::between;
use crate::resources::CustomKind;

const API_VERSION: &str = "apiVersion";
const KIND: &str = "kind";
const READY_TRANSITION: &str = "/status/conditions";
const READY_CONDITION: &str = "Ready";
const ROOT_NAME: &str = "flux-system";
const SHORT_SHA: usize = 7;
pub const WATCHED: [ObjectKind; 2] = [ObjectKind::Kustomization, ObjectKind::HelmRelease];

pub fn custom_kind(kind: ObjectKind) -> CustomKind {
    let (group, version) = kind.api_version().split_once('/').unwrap_or_default();
    CustomKind::new(group, version, kind.as_str())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FluxObject {
    pub kind: ObjectKind,
    pub namespace: String,
    pub name: String,
    pub status: Status,
    pub sentence: String,
    pub suspended: bool,
    pub revision: Option<String>,
    pub since: Duration,
}

fn core_status(status: fleet_flux::Status) -> Status {
    match status {
        fleet_flux::Status::Ok => Status::Ok,
        fleet_flux::Status::Warn => Status::Warn,
        fleet_flux::Status::Err => Status::Err,
        fleet_flux::Status::Progress => Status::Progress,
        fleet_flux::Status::Suspended => Status::Suspended,
        fleet_flux::Status::Offline => Status::Offline,
    }
}

fn ready_since(value: &Value, now: Timestamp) -> Duration {
    value
        .pointer(READY_TRANSITION)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|condition| condition.get("type").and_then(Value::as_str) == Some(READY_CONDITION))
        .and_then(|condition| condition.get("lastTransitionTime"))
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<Timestamp>().ok())
        .map(|then| between(then, now))
        .unwrap_or_default()
}

pub fn flux_object(object: &DynamicObject, kind: ObjectKind, now: Timestamp) -> Option<FluxObject> {
    let mut value = serde_json::to_value(object).ok()?;
    let fields = value.as_object_mut()?;
    fields
        .entry(API_VERSION)
        .or_insert_with(|| Value::from(kind.api_version()));
    fields
        .entry(KIND)
        .or_insert_with(|| Value::from(kind.as_str()));
    let parsed = GitOpsObject::from_json(&value);
    let gitops = match parsed {
        Ok(gitops) => gitops,
        Err(error) => {
            tracing::debug!(%error, "skipping unreadable Flux object");
            return None;
        }
    };
    let summary = summarize(&gitops, None);
    Some(FluxObject {
        kind,
        revision: gitops.revision().map(str::to_owned),
        suspended: gitops.suspended,
        namespace: gitops.identity.namespace,
        name: gitops.identity.name,
        status: core_status(summary.status),
        sentence: summary.sentence,
        since: ready_since(&value, now),
    })
}

pub fn counts(objects: &[FluxObject]) -> Counts {
    let total = u32::try_from(objects.len()).unwrap_or(u32::MAX);
    let tally = |status: Status| {
        let matching = objects.iter().filter(|o| o.status == status).count();
        u32::try_from(matching).unwrap_or(u32::MAX)
    };
    Counts::total(total)
        .with_err(tally(Status::Err))
        .with_warn(tally(Status::Warn))
        .with_sus(tally(Status::Suspended))
}

pub fn problems(cluster: &ClusterId, objects: &[FluxObject]) -> Vec<Problem> {
    objects
        .iter()
        .filter(|object| {
            matches!(
                object.status,
                Status::Err | Status::Warn | Status::Suspended
            )
        })
        .map(|object| Problem {
            status: object.status,
            target: ResourceRef {
                cluster: cluster.clone(),
                kind: object.kind.as_str().to_owned(),
                namespace: Some(object.namespace.clone()),
                name: object.name.clone(),
            },
            message: object.sentence.clone(),
            age: object.since,
        })
        .collect()
}

pub fn short_revision(revision: &str) -> String {
    match revision.rsplit_once(':') {
        Some((_, hash)) => hash.chars().take(SHORT_SHA).collect(),
        None => revision
            .rsplit_once('@')
            .map_or(revision, |(_, tail)| tail)
            .to_owned(),
    }
}

pub fn revision(objects: &[FluxObject]) -> Revision {
    let mut kustomizations: Vec<&FluxObject> = objects
        .iter()
        .filter(|o| o.kind == ObjectKind::Kustomization && o.revision.is_some())
        .collect();
    kustomizations.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    let root = kustomizations
        .iter()
        .find(|o| o.name == ROOT_NAME && o.namespace == ROOT_NAME)
        .or_else(|| kustomizations.first());
    match root {
        Some(object) => Revision {
            sha: object
                .revision
                .as_deref()
                .map(short_revision)
                .unwrap_or_default(),
            held: object.suspended,
        },
        None => Revision {
            sha: String::new(),
            held: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::problems::tests::{at, now};

    fn dynamic(value: Value) -> DynamicObject {
        match serde_json::from_value(value) {
            Ok(object) => object,
            Err(error) => panic!("fixture: {error}"),
        }
    }

    fn broken_kustomization() -> DynamicObject {
        dynamic(json!({
            "metadata": { "name": "broken-kustomization", "namespace": "flux-system", "generation": 1 },
            "spec": { "interval": "10m", "prune": true, "path": "./fleet-nonexistent-path", "sourceRef": { "kind": "GitRepository", "name": "podinfo" } },
            "status": {
                "observedGeneration": 1,
                "conditions": [{
                    "type": "Ready", "status": "False", "reason": "ArtifactFailed",
                    "message": "kustomization path not found: stat /tmp/x/fleet-nonexistent-path: no such file or directory",
                    "lastTransitionTime": at(4)
                }]
            }
        }))
    }

    fn suspended_release() -> DynamicObject {
        dynamic(json!({
            "apiVersion": "helm.toolkit.fluxcd.io/v2",
            "kind": "HelmRelease",
            "metadata": { "name": "search-indexer", "namespace": "search" },
            "spec": { "interval": "10m", "suspend": true, "chart": { "spec": { "chart": "podinfo", "sourceRef": { "kind": "HelmRepository", "name": "podinfo" } } } }
        }))
    }

    fn ready_kustomization(name: &str, revision: &str) -> DynamicObject {
        dynamic(json!({
            "metadata": { "name": name, "namespace": "flux-system" },
            "spec": { "interval": "10m", "prune": true, "path": "./kustomize", "sourceRef": { "kind": "GitRepository", "name": "podinfo" } },
            "status": {
                "lastAppliedRevision": revision,
                "conditions": [{ "type": "Ready", "status": "True", "reason": "ReconciliationSucceeded", "message": "Applied", "lastTransitionTime": at(30) }]
            }
        }))
    }

    fn objects() -> Vec<FluxObject> {
        [
            (broken_kustomization(), ObjectKind::Kustomization),
            (suspended_release(), ObjectKind::HelmRelease),
            (
                ready_kustomization(
                    "apps",
                    "master@sha1:b6b680fe507be26c0a5e8c8a1b6c3a0e8d9f0a11",
                ),
                ObjectKind::Kustomization,
            ),
        ]
        .iter()
        .filter_map(|(object, kind)| flux_object(object, *kind, now()))
        .collect()
    }

    #[test]
    fn failing_and_suspended_objects_become_problems() {
        let problems = problems(&ClusterId::new("kind-fleet-a"), &objects());
        let lines: Vec<(Status, String, &str, Duration)> = problems
            .iter()
            .map(|p| (p.status, p.target.to_string(), p.message.as_str(), p.age))
            .collect();
        assert_eq!(
            lines,
            [
                (
                    Status::Err,
                    "flux-system/broken-kustomization".into(),
                    "The build path does not exist in the source artifact.",
                    Duration::from_mins(4)
                ),
                (
                    Status::Suspended,
                    "search/search-indexer".into(),
                    "Reconciliation is suspended.",
                    Duration::ZERO
                ),
            ]
        );
    }

    #[test]
    fn counts_and_revision_summarize_the_cluster() {
        let objects = objects();
        let counts = counts(&objects);
        assert_eq!(counts, Counts::total(3).with_err(1).with_sus(1));
        let revision = revision(&objects);
        assert_eq!(revision.sha, "b6b680f");
        assert!(!revision.held);
    }

    #[test]
    fn short_revisions() {
        assert_eq!(short_revision("main@sha1:a41e07c99"), "a41e07c");
        assert_eq!(short_revision("6.5.4"), "6.5.4");
        assert_eq!(short_revision("v1@6.5.4"), "6.5.4");
        assert_eq!(
            custom_kind(ObjectKind::HelmRelease),
            CustomKind::new("helm.toolkit.fluxcd.io", "v2", "HelmRelease")
        );
    }
}
