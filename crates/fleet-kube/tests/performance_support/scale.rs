use std::sync::Arc;
use std::time::{Duration, Instant};

use fleet_kube::discovery::{DiscoverySources, KubeContext, discover};
use fleet_kube::manager::{ClusterHandle, ClusterManager};
use fleet_kube::resources::WatchKind;
use fleet_kube::settings::KubeSettings;
use fleet_kube::shell_env::ShellEnv;
use fleet_kube::snapshot;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{ConfigMap, Event, Pod, Service};
use kube::api::PartialObjectMeta;

const CONTEXT: &str = "kind-fleet-b";
pub const SCALE_NAMESPACE: &str = "fleet-scale";
pub const LIVE_CONFIGMAPS: usize = 3000;
const LIVE_SERVICES: usize = 1500;
pub const LIVE_PODS: usize = 200;
const LIVE_DEPLOYMENTS: usize = 400;
const LIVE_EVENTS: usize = 2000;
pub const WAIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(100);

pub fn context() -> (tempfile::TempDir, DiscoverySources, KubeContext) {
    assert_eq!(std::env::var("FLEET_E2E").as_deref(), Ok("1"));
    let Ok(directory) = tempfile::tempdir() else {
        panic!("tempdir failed")
    };
    let output = std::process::Command::new("kind")
        .args(["get", "kubeconfig", "--name", "fleet-b"])
        .output();
    let Ok(output) = output else {
        panic!("kind failed")
    };
    assert!(output.status.success(), "kind kubeconfig failed");
    let kubeconfig = directory.path().join("kind.kubeconfig");
    assert!(std::fs::write(&kubeconfig, output.stdout).is_ok());
    let sources = DiscoverySources {
        kubeconfig_env: Some(kubeconfig.into_os_string()),
        home: None,
        folders: Vec::new(),
    };
    let discovery = discover(&sources);
    assert_eq!(discovery.contexts.len(), 1);
    let Some(context) = discovery
        .contexts
        .into_iter()
        .find(|c| c.id.context == CONTEXT)
    else {
        panic!("kind-fleet-b context missing");
    };
    (directory, sources, context)
}

async fn wait_for_stores(handle: &ClusterHandle) {
    let deadline = Instant::now() + WAIT;
    loop {
        let maps = handle.objects::<PartialObjectMeta<ConfigMap>>();
        let services = handle.objects::<Service>();
        let pods = handle.objects::<Pod>();
        let deployments = handle.objects::<Deployment>();
        let events = handle.objects::<Event>();
        let count = |namespace: Option<&str>| namespace == Some(SCALE_NAMESPACE);
        if maps
            .iter()
            .filter(|m| {
                count(m.metadata.namespace.as_deref())
                    && m.metadata
                        .name
                        .as_deref()
                        .is_some_and(|name| name.starts_with("scale-configmap-"))
            })
            .count()
            == LIVE_CONFIGMAPS
            && services
                .iter()
                .filter(|s| count(s.metadata.namespace.as_deref()))
                .count()
                == LIVE_SERVICES
            && pods
                .iter()
                .filter(|p| count(p.metadata.namespace.as_deref()))
                .count()
                == LIVE_PODS
            && deployments
                .iter()
                .filter(|d| count(d.metadata.namespace.as_deref()))
                .count()
                == LIVE_DEPLOYMENTS
            && events
                .iter()
                .filter(|e| {
                    count(e.metadata.namespace.as_deref())
                        && e.metadata
                            .name
                            .as_deref()
                            .is_some_and(|name| name.starts_with("scale-event-"))
                })
                .count()
                == LIVE_EVENTS
        {
            break;
        }
        assert!(Instant::now() < deadline, "scale stores did not initialize");
        tokio::time::sleep(POLL).await;
    }
}

pub async fn populated_stores(
    context: &KubeContext,
    settings: &KubeSettings,
) -> (
    ClusterManager,
    ClusterHandle,
    Vec<fleet_kube::manager::Subscription>,
) {
    let env = Arc::new(ShellEnv::from_process(String::new()));
    let (manager, _events) =
        ClusterManager::new(tokio::runtime::Handle::current(), settings.clone(), env);
    let handle = manager.connect(context.clone());
    let kinds = [
        WatchKind::Pods,
        WatchKind::Nodes,
        WatchKind::ConfigMaps,
        WatchKind::Services,
    ];
    let mut subscriptions: Vec<_> = kinds
        .into_iter()
        .map(|kind| handle.subscribe(kind))
        .collect();
    subscriptions.extend(
        snapshot::OVERVIEW_KINDS
            .into_iter()
            .map(|kind| handle.subscribe(kind)),
    );
    wait_for_stores(&handle).await;
    (manager, handle, subscriptions)
}
