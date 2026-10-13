use k8s_openapi::api::apps::v1::{
    ControllerRevision, DaemonSet, Deployment, ReplicaSet, StatefulSet,
};
use k8s_openapi::api::core::v1::PodTemplateSpec;
use kube::Api;
use kube::api::{ListParams, Patch, PatchParams};
use serde_json::json;

use fleet_core::Uid;

use crate::error::{Error, Result};
use crate::manager::ClusterHandle;
use crate::ops::{FIELD_MANAGER, with_uid, with_uid_test};

pub const DEPLOYMENT: &str = "Deployment";
pub const STATEFUL_SET: &str = "StatefulSet";
pub const DAEMON_SET: &str = "DaemonSet";
pub const REVISION_ANNOTATION: &str = "deployment.kubernetes.io/revision";
const POD_TEMPLATE_HASH: &str = "pod-template-hash";

#[derive(Debug, Clone, PartialEq)]
pub enum Rollback {
    Apply(Box<PodTemplateSpec>),
    AlreadyCurrent,
}

pub fn revision_of(set: &ReplicaSet) -> Option<u64> {
    set.metadata
        .annotations
        .as_ref()?
        .get(REVISION_ANNOTATION)?
        .parse()
        .ok()
}

fn owned_by(set: &ReplicaSet, uid: &str) -> bool {
    set.metadata
        .owner_references
        .iter()
        .flatten()
        .any(|owner| owner.controller == Some(true) && owner.uid == uid)
}

fn without_hash(template: &PodTemplateSpec) -> PodTemplateSpec {
    let mut template = template.clone();
    if let Some(labels) = template
        .metadata
        .as_mut()
        .and_then(|meta| meta.labels.as_mut())
    {
        labels.remove(POD_TEMPLATE_HASH);
    }
    template
}

pub fn rollback_template(
    deployment: &Deployment,
    sets: &[ReplicaSet],
    revision: u64,
) -> Result<Rollback> {
    let name = deployment.metadata.name.clone().unwrap_or_default();
    let paused = deployment
        .spec
        .as_ref()
        .and_then(|spec| spec.paused)
        .unwrap_or(false);
    if paused {
        return Err(Error::Paused(name));
    }
    let uid = deployment
        .metadata
        .uid
        .as_deref()
        .ok_or(Error::MissingField("metadata.uid"))?;
    let owned: Vec<(u64, &ReplicaSet)> = sets
        .iter()
        .filter(|set| owned_by(set, uid))
        .filter_map(|set| Some((revision_of(set)?, set)))
        .collect();
    let current = owned.iter().map(|(number, _)| *number).max();
    let wanted = if revision == 0 {
        owned
            .iter()
            .map(|(number, _)| *number)
            .filter(|number| Some(*number) != current)
            .max()
            .ok_or(Error::RevisionNotFound(revision))?
    } else {
        revision
    };
    let set = owned
        .iter()
        .find(|(number, _)| *number == wanted)
        .map(|(_, set)| *set)
        .ok_or(Error::RevisionNotFound(wanted))?;
    let template = set
        .spec
        .as_ref()
        .and_then(|spec| spec.template.as_ref())
        .ok_or(Error::MissingField("spec.template"))?;
    let target = without_hash(template);
    let live = deployment
        .spec
        .as_ref()
        .map(|spec| without_hash(&spec.template));
    if live.as_ref() == Some(&target) {
        return Ok(Rollback::AlreadyCurrent);
    }
    Ok(Rollback::Apply(Box::new(target)))
}

fn same(kind: &str, name: &str, found: Option<&str>, expected: &Uid) -> Result<()> {
    if found == Some(expected.as_str()) {
        return Ok(());
    }
    Err(Error::Replaced {
        kind: kind.to_owned(),
        name: name.to_owned(),
    })
}

pub async fn rollback(
    handle: &ClusterHandle,
    namespace: &str,
    name: &str,
    uid: &Uid,
    revision: u64,
) -> Result<()> {
    let client = handle.client()?;
    let deployments: Api<Deployment> = Api::namespaced(client.clone(), namespace);
    let sets: Api<ReplicaSet> = Api::namespaced(client, namespace);
    let deployment = deployments.get(name).await?;
    same(DEPLOYMENT, name, deployment.metadata.uid.as_deref(), uid)?;
    let listed = sets.list(&ListParams::default()).await?;
    let Rollback::Apply(template) = rollback_template(&deployment, &listed.items, revision)? else {
        return Ok(());
    };
    let replace = json!([{ "op": "replace", "path": "/spec/template", "value": template }]);
    let operations: json_patch::Patch = serde_json::from_value(with_uid_test(&replace, uid))?;
    let params = PatchParams {
        field_manager: Some(FIELD_MANAGER.to_owned()),
        ..PatchParams::default()
    };
    deployments
        .patch(name, &params, &Patch::Json::<()>(operations))
        .await?;
    Ok(())
}

fn owned_revisions<'a>(
    uid: &str,
    revisions: &'a [ControllerRevision],
) -> Vec<(u64, &'a ControllerRevision)> {
    revisions
        .iter()
        .filter(|revision| {
            revision
                .metadata
                .owner_references
                .iter()
                .flatten()
                .any(|owner| owner.controller == Some(true) && owner.uid == uid)
        })
        .filter_map(|revision| Some((u64::try_from(revision.revision).ok()?, revision)))
        .collect()
}

pub fn controller_rollback(
    uid: &str,
    revisions: &[ControllerRevision],
    revision: u64,
) -> Result<Option<serde_json::Value>> {
    let owned = owned_revisions(uid, revisions);
    let current = owned.iter().map(|(number, _)| *number).max();
    let wanted = if revision == 0 {
        owned
            .iter()
            .map(|(number, _)| *number)
            .filter(|number| Some(*number) != current)
            .max()
            .ok_or(Error::RevisionNotFound(revision))?
    } else {
        revision
    };
    let chosen = owned
        .iter()
        .find(|(number, _)| *number == wanted)
        .map(|(_, revision)| *revision)
        .ok_or(Error::RevisionNotFound(wanted))?;
    if Some(wanted) == current {
        return Ok(None);
    }
    let data = chosen.data.as_ref().ok_or(Error::MissingField("data"))?;
    Ok(Some(data.0.clone()))
}

async fn controller_undo<K>(api: Api<K>, name: &str, expected: &Uid, revision: u64) -> Result<()>
where
    K: kube::Resource<DynamicType = ()>
        + Clone
        + serde::de::DeserializeOwned
        + serde::Serialize
        + std::fmt::Debug,
{
    let object = api.get(name).await?;
    let uid = object
        .meta()
        .uid
        .clone()
        .ok_or(Error::MissingField("metadata.uid"))?;
    same(&K::kind(&()), name, Some(&uid), expected)?;
    let namespace = object.meta().namespace.clone().unwrap_or_default();
    let client = api.clone().into_client();
    let history: Api<ControllerRevision> = Api::namespaced(client, &namespace);
    let listed = history.list(&ListParams::default()).await?;
    let Some(patch) = controller_rollback(&uid, &listed.items, revision)? else {
        return Ok(());
    };
    let params = PatchParams {
        field_manager: Some(FIELD_MANAGER.to_owned()),
        ..PatchParams::default()
    };
    let patch = with_uid(patch, expected);
    api.patch(name, &params, &Patch::Strategic(patch)).await?;
    Ok(())
}

pub async fn rollback_controller(
    handle: &ClusterHandle,
    kind: &str,
    namespace: &str,
    name: &str,
    uid: &Uid,
    revision: u64,
) -> Result<()> {
    let client = handle.client()?;
    match kind {
        STATEFUL_SET => {
            let api: Api<StatefulSet> = Api::namespaced(client, namespace);
            controller_undo(api, name, uid, revision).await
        }
        DAEMON_SET => {
            let api: Api<DaemonSet> = Api::namespaced(client, namespace);
            controller_undo(api, name, uid, revision).await
        }
        other => Err(Error::Unsupported {
            action: "Roll back",
            kind: other.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use k8s_openapi::api::apps::v1::{DeploymentSpec, ReplicaSetSpec};
    use k8s_openapi::api::core::v1::{Container, PodSpec};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};

    use super::*;

    const UID: &str = "deploy-uid";

    fn template(image: &str, hash: Option<&str>) -> PodTemplateSpec {
        let mut labels = BTreeMap::from([("app".to_owned(), "checkout".to_owned())]);
        if let Some(hash) = hash {
            labels.insert(POD_TEMPLATE_HASH.into(), hash.into());
        }
        PodTemplateSpec {
            metadata: Some(ObjectMeta {
                labels: Some(labels),
                ..ObjectMeta::default()
            }),
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "api".into(),
                    image: Some(image.into()),
                    ..Container::default()
                }],
                ..PodSpec::default()
            }),
        }
    }

    fn set(revision: u64, image: &str, owner: &str) -> ReplicaSet {
        ReplicaSet {
            metadata: ObjectMeta {
                name: Some(format!("checkout-{revision}")),
                annotations: Some(BTreeMap::from([(
                    REVISION_ANNOTATION.to_owned(),
                    revision.to_string(),
                )])),
                owner_references: Some(vec![OwnerReference {
                    controller: Some(true),
                    uid: owner.into(),
                    ..OwnerReference::default()
                }]),
                ..ObjectMeta::default()
            },
            spec: Some(ReplicaSetSpec {
                template: Some(template(image, Some(&format!("h{revision}")))),
                ..ReplicaSetSpec::default()
            }),
            status: None,
        }
    }

    fn deployment(image: &str, paused: bool) -> Deployment {
        Deployment {
            metadata: ObjectMeta {
                name: Some("checkout".into()),
                uid: Some(UID.into()),
                ..ObjectMeta::default()
            },
            spec: Some(DeploymentSpec {
                template: template(image, None),
                paused: Some(paused),
                ..DeploymentSpec::default()
            }),
            status: None,
        }
    }

    fn sets() -> Vec<ReplicaSet> {
        vec![
            set(35, "api:2.12", UID),
            set(36, "api:2.13", UID),
            set(37, "api:2.14", UID),
            set(36, "other:1", "someone-else"),
        ]
    }

    #[test]
    fn rolls_back_to_the_requested_revision_without_its_hash() {
        let Ok(Rollback::Apply(chosen)) =
            rollback_template(&deployment("api:2.14", false), &sets(), 35)
        else {
            panic!("revision 35 applies");
        };
        assert_eq!(*chosen, template("api:2.12", None));
    }

    #[test]
    fn revision_zero_means_the_previous_one() {
        let chosen = rollback_template(&deployment("api:2.14", false), &sets(), 0);
        assert_eq!(
            chosen.ok(),
            Some(Rollback::Apply(Box::new(template("api:2.13", None))))
        );
    }

    #[test]
    fn ignores_replica_sets_of_other_owners_and_unknown_revisions() {
        let foreign = vec![set(36, "other:1", "someone-else")];
        assert!(matches!(
            rollback_template(&deployment("api:2.14", false), &foreign, 36),
            Err(Error::RevisionNotFound(36))
        ));
        assert!(matches!(
            rollback_template(&deployment("api:2.14", false), &sets(), 99),
            Err(Error::RevisionNotFound(99))
        ));
    }

    #[test]
    fn current_template_is_a_no_op_and_paused_is_refused() {
        assert_eq!(
            rollback_template(&deployment("api:2.14", false), &sets(), 37).ok(),
            Some(Rollback::AlreadyCurrent)
        );
        assert!(matches!(
            rollback_template(&deployment("api:2.14", true), &sets(), 36),
            Err(Error::Paused(name)) if name == "checkout"
        ));
    }

    fn controller_revision(revision: i64, image: &str, owner: &str) -> ControllerRevision {
        ControllerRevision {
            metadata: ObjectMeta {
                name: Some(format!("db-{revision}")),
                owner_references: Some(vec![OwnerReference {
                    controller: Some(true),
                    uid: owner.into(),
                    ..OwnerReference::default()
                }]),
                ..ObjectMeta::default()
            },
            revision,
            data: Some(k8s_openapi::apimachinery::pkg::runtime::RawExtension(
                json!({
                    "spec": { "template": {
                        "$patch": "replace",
                        "spec": { "containers": [{ "name": "db", "image": image }] }
                    } }
                }),
            )),
        }
    }

    fn history() -> Vec<ControllerRevision> {
        vec![
            controller_revision(1, "db:15", UID),
            controller_revision(2, "db:16", UID),
            controller_revision(3, "db:17", UID),
            controller_revision(2, "other:1", "someone-else"),
        ]
    }

    fn image(patch: &serde_json::Value) -> Option<&str> {
        patch
            .pointer("/spec/template/spec/containers/0/image")
            .and_then(serde_json::Value::as_str)
    }

    #[test]
    fn controller_rollback_patches_the_chosen_revision() {
        let chosen = controller_rollback(UID, &history(), 1).ok().flatten();
        assert_eq!(chosen.as_ref().and_then(image), Some("db:15"));
        let previous = controller_rollback(UID, &history(), 0).ok().flatten();
        assert_eq!(previous.as_ref().and_then(image), Some("db:16"));
    }

    #[test]
    fn controller_rollback_skips_the_current_and_unknown_revisions() {
        assert!(matches!(controller_rollback(UID, &history(), 3), Ok(None)));
        assert!(matches!(
            controller_rollback(UID, &history(), 9),
            Err(Error::RevisionNotFound(9))
        ));
        let foreign = vec![controller_revision(2, "other:1", "someone-else")];
        assert!(matches!(
            controller_rollback(UID, &foreign, 2),
            Err(Error::RevisionNotFound(2))
        ));
    }
}
