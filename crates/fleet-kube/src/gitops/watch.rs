use std::sync::{Arc, Mutex};
use std::time::Duration;

use fleet_core::Status;
use fleet_core::gitops::Controller;
use fleet_flux::ObjectKind;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Namespace;
use kube::api::{DynamicObject, ListParams};
use kube::{Api, Client};

use crate::error::Error;
use crate::manager::{ClusterHandle, Subscription, lock};
use crate::resources::CustomKind;
use crate::task::TaskGuard;

pub const KINDS: [ObjectKind; 6] = [
    ObjectKind::Kustomization,
    ObjectKind::HelmRelease,
    ObjectKind::GitRepository,
    ObjectKind::HelmRepository,
    ObjectKind::HelmChart,
    ObjectKind::OCIRepository,
];
pub const FLUX_NAMESPACE: &str = "flux-system";
const VERSION_LABEL: &str = "app.kubernetes.io/version";
const PART_OF_FLUX: &str = "app.kubernetes.io/part-of=flux";
const CONTROLLER_SUFFIX: &str = "-controller";
const CONTROLLERS: [&str; 5] = [
    "source",
    "kustomize",
    "helm",
    "notification",
    "image-automation",
];
const CONNECT_RETRY: Duration = Duration::from_secs(3);
const REFRESH: Duration = Duration::from_mins(5);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FluxInfo {
    pub kinds: Vec<ObjectKind>,
    pub version: Option<String>,
    pub controllers: Vec<Controller>,
}

impl FluxInfo {
    pub fn detected(&self) -> bool {
        !self.kinds.is_empty()
    }
}

pub struct ClusterFlux {
    pub handle: ClusterHandle,
    pub info: Arc<Mutex<FluxInfo>>,
    _task: TaskGuard,
}

impl ClusterFlux {
    pub fn start(handle: ClusterHandle, changed: async_channel::Sender<()>) -> Self {
        let info = Arc::new(Mutex::new(FluxInfo::default()));
        let task = TaskGuard::spawn(detect(handle.clone(), info.clone(), changed));
        Self {
            handle,
            info,
            _task: task,
        }
    }

    pub fn info(&self) -> FluxInfo {
        lock(&self.info).clone()
    }

    pub fn objects(&self, kind: ObjectKind) -> Vec<Arc<DynamicObject>> {
        self.handle.custom_objects(&custom_kind(kind))
    }
}

pub fn custom_kind(kind: ObjectKind) -> CustomKind {
    let (group, version) = kind.api_version().split_once('/').unwrap_or_default();
    CustomKind::new(group, version, kind.as_str())
}

async fn detect(
    handle: ClusterHandle,
    info: Arc<Mutex<FluxInfo>>,
    changed: async_channel::Sender<()>,
) {
    let mut subscriptions: Vec<Subscription> = Vec::new();
    loop {
        let mut waiting = false;
        for kind in KINDS {
            if lock(&info).kinds.contains(&kind) {
                continue;
            }
            match handle.subscribe_custom(custom_kind(kind)).await {
                Ok(subscription) => {
                    subscriptions.push(subscription);
                    lock(&info).kinds.push(kind);
                }
                Err(Error::NotConnected) => waiting = true,
                Err(error) => tracing::debug!(%error, kind = kind.as_str(), "Flux kind not served"),
            }
        }
        if let Ok(client) = handle.client() {
            let (version, controllers) = install(client).await;
            let mut info = lock(&info);
            info.version = version;
            info.controllers = controllers;
        }
        if changed.try_send(()).is_err() {
            tracing::trace!("GitOps change already pending");
        }
        let pause = if waiting { CONNECT_RETRY } else { REFRESH };
        tracing::debug!(cluster = %handle.context().id.context, kinds = lock(&info).kinds.len(), waiting, "Flux detection pass");
        tokio::time::sleep(pause).await;
    }
}

async fn install(client: Client) -> (Option<String>, Vec<Controller>) {
    let namespaces: Api<Namespace> = Api::all(client.clone());
    let version = match namespaces.get_opt(FLUX_NAMESPACE).await {
        Ok(namespace) => namespace
            .and_then(|namespace| namespace.metadata.labels)
            .and_then(|labels| labels.get(VERSION_LABEL).cloned()),
        Err(error) => {
            tracing::debug!(%error, "Flux namespace unreadable");
            None
        }
    };
    let deployments: Api<Deployment> = Api::namespaced(client, FLUX_NAMESPACE);
    let listed = deployments
        .list(&ListParams::default().labels(PART_OF_FLUX))
        .await;
    let deployed = match listed {
        Ok(list) => list.items,
        Err(error) => {
            tracing::debug!(%error, "Flux controllers unreadable");
            Vec::new()
        }
    };
    (version, controllers(&deployed))
}

pub fn controllers(deployments: &[Deployment]) -> Vec<Controller> {
    CONTROLLERS
        .iter()
        .map(|name| {
            let deployment = deployments.iter().find(|deployment| {
                deployment
                    .metadata
                    .name
                    .as_deref()
                    .and_then(|full| full.strip_suffix(CONTROLLER_SUFFIX))
                    == Some(name)
            });
            Controller {
                name: (*name).to_owned(),
                status: deployment.map(controller_status),
            }
        })
        .collect()
}

fn controller_status(deployment: &Deployment) -> Status {
    let wanted = deployment
        .spec
        .as_ref()
        .and_then(|spec| spec.replicas)
        .unwrap_or(1);
    let ready = deployment
        .status
        .as_ref()
        .and_then(|status| status.ready_replicas)
        .unwrap_or_default();
    if ready >= wanted {
        Status::Ok
    } else {
        Status::Err
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn deployment(name: &str, replicas: i32, ready: i32) -> Deployment {
        let parsed = serde_json::from_value(json!({
            "metadata": { "name": name },
            "spec": { "replicas": replicas, "selector": {}, "template": {} },
            "status": { "readyReplicas": ready }
        }));
        let Ok(deployment) = parsed else {
            panic!("deployment fixture");
        };
        deployment
    }

    #[test]
    fn controllers_list_known_names_with_readiness() {
        let deployed = [
            deployment("source-controller", 1, 1),
            deployment("helm-controller", 1, 0),
            deployment("kustomize-controller", 1, 1),
        ];
        let found: Vec<(String, Option<Status>)> = controllers(&deployed)
            .into_iter()
            .map(|c| (c.name, c.status))
            .collect();
        assert_eq!(
            found,
            [
                ("source".to_owned(), Some(Status::Ok)),
                ("kustomize".to_owned(), Some(Status::Ok)),
                ("helm".to_owned(), Some(Status::Err)),
                ("notification".to_owned(), None),
                ("image-automation".to_owned(), None),
            ]
        );
    }

    #[test]
    fn custom_kinds_split_the_api_version() {
        let kind = custom_kind(ObjectKind::HelmRelease);
        assert_eq!(kind.group, "helm.toolkit.fluxcd.io");
        assert_eq!(kind.version, "v2");
        assert_eq!(kind.kind, "HelmRelease");
    }
}
