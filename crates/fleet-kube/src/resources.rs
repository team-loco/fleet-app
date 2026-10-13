use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

use futures::StreamExt;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Event, Namespace, Node, PersistentVolumeClaim, Pod, Secret, Service, ServiceAccount,
};
use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
use kube::api::{DynamicObject, PartialObjectMeta};
use kube::core::{ApiResource, GroupVersionKind};
use kube::runtime::reflector::{self, Store};
use kube::runtime::{WatchStreamExt, watcher};
use kube::{Api, Client, Resource};
use serde::de::DeserializeOwned;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::Error;
use crate::task::TaskGuard;

const LAST_APPLIED: &str = "kubectl.kubernetes.io/last-applied-configuration";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomKind {
    pub group: String,
    pub version: String,
    pub kind: String,
}

impl CustomKind {
    pub fn new(group: &str, version: &str, kind: &str) -> Self {
        Self {
            group: group.to_owned(),
            version: version.to_owned(),
            kind: kind.to_owned(),
        }
    }

    pub fn gvk(&self) -> GroupVersionKind {
        GroupVersionKind::gvk(&self.group, &self.version, &self.kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WatchKind {
    Namespaces,
    Nodes,
    Pods,
    Deployments,
    StatefulSets,
    DaemonSets,
    ReplicaSets,
    Jobs,
    CronJobs,
    Services,
    Ingresses,
    ConfigMaps,
    Secrets,
    ServiceAccounts,
    NetworkPolicies,
    Events,
    PersistentVolumeClaims,
    Custom(CustomKind),
}

pub trait Watched: Resource + Clone + Debug + Send + Sync + 'static {
    const KIND: WatchKind;
}

macro_rules! watched {
    ($($ty:ty => $kind:ident),* $(,)?) => {
        $(impl Watched for $ty {
            const KIND: WatchKind = WatchKind::$kind;
        })*
    };
}

watched! {
    Namespace => Namespaces,
    Node => Nodes,
    Pod => Pods,
    Deployment => Deployments,
    StatefulSet => StatefulSets,
    DaemonSet => DaemonSets,
    ReplicaSet => ReplicaSets,
    Job => Jobs,
    CronJob => CronJobs,
    Service => Services,
    Ingress => Ingresses,
    PartialObjectMeta<ConfigMap> => ConfigMaps,
    PartialObjectMeta<Secret> => Secrets,
    PartialObjectMeta<ServiceAccount> => ServiceAccounts,
    PartialObjectMeta<NetworkPolicy> => NetworkPolicies,
    Event => Events,
    PersistentVolumeClaim => PersistentVolumeClaims,
}

#[derive(Debug)]
pub enum SignalEvent {
    Changed,
    Failed(Error),
}

#[derive(Debug)]
pub struct Signal {
    pub kind: WatchKind,
    pub event: SignalEvent,
}

pub struct RunningWatch {
    store: Arc<dyn Any + Send + Sync>,
    _task: TaskGuard,
}

fn into_error(err: watcher::Error) -> Error {
    match err {
        watcher::Error::InitialListFailed(inner) => Error::Kube(inner),
        watcher::Error::WatchStartFailed(inner) => Error::Kube(inner),
        watcher::Error::WatchFailed(inner) => Error::Kube(inner),
        watcher::Error::WatchError(status) => Error::Kube(kube::Error::Api(status)),
        watcher::Error::NoResourceVersion => Error::MissingField("resourceVersion"),
    }
}

async fn forward<K, S>(stream: S, kind: WatchKind, signals: UnboundedSender<Signal>)
where
    S: futures::Stream<Item = Result<watcher::Event<K>, watcher::Error>> + Send,
{
    let mut stream = std::pin::pin!(stream);
    while let Some(item) = stream.next().await {
        let event = match item {
            Ok(watcher::Event::Init | watcher::Event::InitApply(_)) => continue,
            Ok(_) => SignalEvent::Changed,
            Err(err) => SignalEvent::Failed(into_error(err)),
        };
        let signal = Signal {
            kind: kind.clone(),
            event,
        };
        if signals.send(signal).is_err() {
            return;
        }
    }
}

fn spawn_full<K>(runtime: &Handle, client: Client, signals: UnboundedSender<Signal>) -> RunningWatch
where
    K: Watched + DeserializeOwned + Resource<DynamicType = ()>,
    K::Scope: Send,
{
    let writer = reflector::store::Writer::<K>::default();
    let store = writer.as_reader();
    let api: Api<K> = Api::all(client);
    let stream = reflector::reflector(
        writer,
        watcher(api, watcher::Config::default())
            .default_backoff()
            .modify(trim_metadata),
    );
    let _guard = runtime.enter();
    RunningWatch {
        store: Arc::new(store),
        _task: TaskGuard::spawn(forward(stream, K::KIND, signals)),
    }
}

pub fn spawn_custom(
    runtime: &Handle,
    client: Client,
    kind: &CustomKind,
    resource: &ApiResource,
    signals: UnboundedSender<Signal>,
) -> RunningWatch {
    let writer = reflector::store::Writer::<DynamicObject>::new(resource.clone());
    let store = writer.as_reader();
    let api: Api<DynamicObject> = Api::all_with(client, resource);
    let stream = reflector::reflector(
        writer,
        watcher(api, watcher::Config::default())
            .default_backoff()
            .modify(trim_metadata),
    );
    let _guard = runtime.enter();
    RunningWatch {
        store: Arc::new(store),
        _task: TaskGuard::spawn(forward(stream, WatchKind::Custom(kind.clone()), signals)),
    }
}

pub fn spawn_builtin(
    kind: &WatchKind,
    runtime: &Handle,
    client: Client,
    signals: UnboundedSender<Signal>,
) -> Option<RunningWatch> {
    let running = match kind {
        WatchKind::Namespaces => spawn_full::<Namespace>(runtime, client, signals),
        WatchKind::Nodes => spawn_full::<Node>(runtime, client, signals),
        WatchKind::Pods => spawn_full::<Pod>(runtime, client, signals),
        WatchKind::Deployments => spawn_full::<Deployment>(runtime, client, signals),
        WatchKind::StatefulSets => spawn_full::<StatefulSet>(runtime, client, signals),
        WatchKind::DaemonSets => spawn_full::<DaemonSet>(runtime, client, signals),
        WatchKind::ReplicaSets => spawn_full::<ReplicaSet>(runtime, client, signals),
        WatchKind::Jobs => spawn_full::<Job>(runtime, client, signals),
        WatchKind::CronJobs => spawn_full::<CronJob>(runtime, client, signals),
        WatchKind::Services => spawn_full::<Service>(runtime, client, signals),
        WatchKind::Ingresses => spawn_full::<Ingress>(runtime, client, signals),
        WatchKind::ConfigMaps => {
            spawn_full::<PartialObjectMeta<ConfigMap>>(runtime, client, signals)
        }
        WatchKind::Secrets => spawn_full::<PartialObjectMeta<Secret>>(runtime, client, signals),
        WatchKind::ServiceAccounts => {
            spawn_full::<PartialObjectMeta<ServiceAccount>>(runtime, client, signals)
        }
        WatchKind::NetworkPolicies => {
            spawn_full::<PartialObjectMeta<NetworkPolicy>>(runtime, client, signals)
        }
        WatchKind::Events => spawn_full::<Event>(runtime, client, signals),
        WatchKind::PersistentVolumeClaims => {
            spawn_full::<PersistentVolumeClaim>(runtime, client, signals)
        }
        WatchKind::Custom(_) => return None,
    };
    Some(running)
}

pub fn snapshot<K: Watched>(running: &RunningWatch) -> Vec<Arc<K>>
where
    K::DynamicType: Eq + std::hash::Hash + Clone,
{
    running
        .store
        .downcast_ref::<Store<K>>()
        .map(Store::state)
        .unwrap_or_default()
}

pub fn custom_snapshot(running: &RunningWatch) -> Vec<Arc<DynamicObject>> {
    running
        .store
        .downcast_ref::<Store<DynamicObject>>()
        .map(Store::state)
        .unwrap_or_default()
}

fn trim_metadata<K: Resource>(object: &mut K) {
    let metadata = object.meta_mut();
    metadata.managed_fields = None;
    if let Some(annotations) = &mut metadata.annotations {
        annotations.remove(LAST_APPLIED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cached_metadata_drops_apply_payload_but_keeps_identity_and_flux_annotations() {
        let parsed = serde_json::from_value::<PartialObjectMeta<ConfigMap>>(json!({
            "metadata": {
                "name": "config", "namespace": "scale", "uid": "uid", "resourceVersion": "42",
                "labels": {"app": "scale"},
                "annotations": {
                    "kubectl.kubernetes.io/last-applied-configuration": "large payload",
                    "reconcile.fluxcd.io/requestedAt": "now"
                },
                "managedFields": [{"manager": "kubectl", "operation": "Update", "apiVersion": "v1"}]
            }
        }));
        let Ok(mut object) = parsed else {
            panic!("fixture failed")
        };
        trim_metadata(&mut object);
        assert!(object.metadata.managed_fields.is_none());
        let Some(annotations) = object.metadata.annotations else {
            panic!("annotations lost")
        };
        assert!(!annotations.contains_key("kubectl.kubernetes.io/last-applied-configuration"));
        assert_eq!(
            annotations
                .get("reconcile.fluxcd.io/requestedAt")
                .map(String::as_str),
            Some("now")
        );
        assert_eq!(object.metadata.name.as_deref(), Some("config"));
        assert_eq!(object.metadata.namespace.as_deref(), Some("scale"));
        assert_eq!(object.metadata.uid.as_deref(), Some("uid"));
        assert_eq!(object.metadata.resource_version.as_deref(), Some("42"));
        let Some(labels) = object.metadata.labels else {
            panic!("labels lost")
        };
        assert_eq!(labels.get("app").map(String::as_str), Some("scale"));
    }
}
