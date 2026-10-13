#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

use fleet_core::{Backend, Command, NewCluster, Page, ResourceKind};
use fleet_kube::assemble::{self, Extras, Selection};
use fleet_kube::discovery::{DiscoverySources, KubeContext, discover};
use fleet_kube::manager::{ClusterHandle, ClusterManager};
use fleet_kube::resources::WatchKind;
use fleet_kube::settings::KubeSettings;
use fleet_kube::shell_env::ShellEnv;
use fleet_kube::store::ClusterStore;
use k8s_openapi::api::core::v1::Service;
use kube::api::{Patch, PatchParams};

use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fleet_core::ClusterId;
use fleet_kube::{detail::PodIndex, lists, pods, snapshot};
use jiff::Timestamp;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{ConfigMap, Event, Pod};
use kube::api::PartialObjectMeta;
use serde_json::json;

const POD_COUNT: usize = 10_000;
const CONFIGMAP_COUNT: usize = 5_000;
const SAMPLES: usize = 40;
const REBUILD_BUDGET: Duration = Duration::from_millis(20);
const MAX_INDEX_ALLOCATIONS: u64 = 16;
const MAX_POD_ALLOCATIONS: u64 = 150_016;

fn fixture<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    let Ok(object) = serde_json::from_value(value) else {
        panic!("invalid synthetic fixture");
    };
    object
}

fn synthetic_pods() -> Vec<Arc<Pod>> {
    (0..POD_COUNT)
        .map(|index| {
            let object = fixture(json!({
                "metadata": {"name": format!("pod-{index:05}"), "namespace": "scale",
                    "labels": {"app": "scale"}},
                "spec": {"nodeName": "node", "containers": [{"name": "pause", "image": "pause",
                    "resources": {"requests": {"cpu": "1m", "memory": "1Mi"}}}]},
                "status": {"phase": "Running", "conditions": [{"type": "Ready", "status": "True"}],
                    "containerStatuses": [{"name": "pause", "image": "pause", "imageID": "pause",
                        "ready": true, "restartCount": 0, "state": {"running": {}}}]}
            }));
            Arc::new(object)
        })
        .collect()
}

fn measure<T>(name: &str, mut build: impl FnMut() -> T) -> Duration {
    drop(black_box(build()));
    let mut timings = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let result = black_box(build());
        timings.push(start.elapsed());
        drop(result);
    }
    timings.sort_unstable();
    let median = timings[SAMPLES / 2];
    println!(
        "{name}: median={median:?}, max={:?}, samples={SAMPLES}",
        timings.last()
    );
    median
}

#[test]
#[ignore = "run in release mode with --ignored --nocapture --test-threads=1"]
fn synthetic_snapshot_builders() {
    let raw = synthetic_pods();
    let cluster = ClusterId::new("scale");
    let now = Timestamp::now();
    let pods = measure("10k pod rows", || pods::pods(&cluster, &raw, &[], &[], now));
    let overview = measure("10k pod overview", || {
        (PodIndex::build(&raw, now), snapshot::pod_counts(&raw))
    });
    let combined = measure("10k selected cluster pods + overview", || {
        let rows = pods::pods(&cluster, &raw, &[], &[], now);
        let index = PodIndex::build(&raw, now);
        let counts = snapshot::pod_counts(&raw);
        (rows, index, counts)
    });
    let maps: Vec<PartialObjectMeta<ConfigMap>> = (0..CONFIGMAP_COUNT)
        .map(|index| {
            fixture(json!({"metadata": {"name": format!("map-{index:05}"), "namespace": "scale"}}))
        })
        .collect();
    measure("5k configmap rows", || {
        let mut rows: Vec<_> = maps.iter().map(|map| lists::config_map(map, now)).collect();
        rows.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
        rows
    });
    assert!(pods < REBUILD_BUDGET, "pod rows: {pods:?}");
    assert!(overview < REBUILD_BUDGET, "overview: {overview:?}");
    assert!(combined < REBUILD_BUDGET, "selected rebuild: {combined:?}");
}

const CONTEXT: &str = "kind-fleet-b";
const SCALE_NAMESPACE: &str = "fleet-scale";
const LIVE_CONFIGMAPS: usize = 3000;
const LIVE_SERVICES: usize = 1500;
const LIVE_PODS: usize = 200;
const LIVE_DEPLOYMENTS: usize = 400;
const LIVE_EVENTS: usize = 2000;
const WAIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(100);
const UPDATE_BUDGET: Duration = Duration::from_millis(500);
const LIVE_SAMPLES: usize = 12;
const EVENT_LIMIT: usize = 200;
const USAGE_SAMPLES: usize = 60;

fn resident_kib() -> u64 {
    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output();
    let Ok(output) = output else {
        panic!("ps failed")
    };
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    let Ok(rss) = text.trim().parse() else {
        panic!("invalid ps RSS")
    };
    rss
}

async fn visible(
    updates: &async_channel::Receiver<fleet_core::Update>,
    predicate: impl Fn(&fleet_core::Snapshot) -> bool,
) -> fleet_core::Snapshot {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let received = tokio::time::timeout_at(deadline, updates.recv()).await;
        let Ok(Ok(update)) = received else {
            panic!("backend did not publish the expected snapshot")
        };
        if let fleet_core::Update::Snapshot(snapshot) = update
            && predicate(&snapshot)
        {
            return *snapshot;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FLEET_E2E=1 and mise run e2e:scale; run alone in release mode"]
async fn kind_scale_snapshot_and_update_latency() {
    let (directory, sources, context) = scale_context();
    let settings = KubeSettings::default();
    let cluster = ClusterId::new("scale-b");
    benchmark_stores(&context, &settings, &cluster).await;
    benchmark_updates(&directory, sources, context, settings, cluster).await;
}

fn scale_context() -> (tempfile::TempDir, DiscoverySources, KubeContext) {
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

async fn benchmark_stores(context: &KubeContext, settings: &KubeSettings, cluster: &ClusterId) {
    let rss_before = resident_kib();
    let (manager, handle, subscriptions) = populated_stores(context, settings).await;
    let rss_stores = resident_kib();
    println!(
        "watcher population RSS delta={} KiB (includes client/runtime, not exact heap size)",
        rss_stores.saturating_sub(rss_before)
    );
    let extras = Extras::new(USAGE_SAMPLES);
    for page in [
        Page::ClusterOverview,
        Page::Pods,
        Page::ConfigMaps,
        Page::Services,
    ] {
        let selection = Selection {
            page,
            event_limit: EVENT_LIMIT,
            yaml: None,
        };
        let name = format!("kind scale {page:?} selected rebuild");
        measure(&name, || {
            assemble::cluster(
                cluster,
                &handle,
                &extras,
                None,
                Some(&selection),
                Timestamp::now(),
            )
        });
    }
    drop(subscriptions);
    manager.disconnect(&context.id);
    drop(handle);
    drop(manager);
}

async fn benchmark_updates(
    directory: &tempfile::TempDir,
    sources: DiscoverySources,
    context: KubeContext,
    settings: KubeSettings,
    cluster: ClusterId,
) {
    let support = directory.path().join("support");
    let store = ClusterStore::empty(&support);
    let backend = fleet_kube::LiveBackend::new(settings, sources, store);
    let (updates_tx, updates) = async_channel::unbounded();
    let (commands, commands_rx) = async_channel::unbounded();
    Box::new(backend).start(updates_tx, commands_rx);
    let add = Command::AddClusters(vec![NewCluster {
        context: context.id.clone(),
        name: cluster.to_string(),
    }]);
    assert!(commands.send(add).await.is_ok());
    visible(&updates, |s| s.cluster(&cluster).is_some()).await;
    assert!(
        commands
            .send(Command::SelectCluster(Some(cluster.clone())))
            .await
            .is_ok()
    );
    assert!(
        commands
            .send(Command::SelectPage(Page::ConfigMaps))
            .await
            .is_ok()
    );
    let configmaps = visible(&updates, |s| {
        s.detail(&cluster)
            .is_some_and(|d| d.resources(ResourceKind::ConfigMap).len() >= LIVE_CONFIGMAPS)
    })
    .await;
    assert!(
        configmaps.pods.is_empty(),
        "ConfigMaps should not build hidden pod details"
    );
    let Some(summary) = configmaps.cluster(&cluster) else {
        panic!("cluster missing")
    };
    assert!(summary.nav.get(&Page::Pods).is_some_and(|counts| {
        counts
            .total
            .is_some_and(|total| usize::try_from(total).is_ok_and(|total| total >= LIVE_PODS))
    }));
    let env = ShellEnv::from_process(String::new());
    let connected =
        fleet_kube::client::connect(&context, &env, KubeSettings::default().connect_timeout).await;
    let Ok(client) = connected else {
        panic!("client failed")
    };
    assert!(commands.send(Command::SelectPage(Page::Pods)).await.is_ok());
    visible(&updates, |s| {
        s.pods
            .iter()
            .filter(|p| p.namespace == SCALE_NAMESPACE)
            .count()
            == LIVE_PODS
    })
    .await;
    let api: kube::Api<Pod> = kube::Api::namespaced(client, SCALE_NAMESPACE);
    let name = "scale-pod-0000";
    measure_updates(&api, name, &updates).await;
    drop(commands);
}

async fn measure_updates(
    api: &kube::Api<Pod>,
    name: &str,
    updates: &async_channel::Receiver<fleet_core::Update>,
) {
    let mut timings = Vec::new();
    let mut requests = Vec::new();
    for sample in 0..LIVE_SAMPLES {
        let marker = format!("probe-{sample}");
        let patch = json!({"metadata": {"labels": {"fleet-scale-probe": marker}}});
        let start = Instant::now();
        let patched = api
            .patch(name, &PatchParams::default(), &Patch::Merge(&patch))
            .await;
        requests.push(start.elapsed());
        assert!(patched.is_ok(), "patch failed: {patched:?}");
        let snapshot = visible(updates, |s| {
            s.pods.iter().any(|row| {
                row.name == name
                    && row.namespace == SCALE_NAMESPACE
                    && row.labels.get("fleet-scale-probe") == Some(&marker)
            })
        })
        .await;
        let mut state = fleet_core::AppState::default();
        state.update(fleet_core::Update::Snapshot(Box::new(snapshot)));
        black_box(state);
        timings.push(start.elapsed());
    }
    let patch = json!({"metadata": {"labels": {"fleet-scale-probe": null}}});
    assert!(
        api.patch(name, &PatchParams::default(), &Patch::Merge(&patch))
            .await
            .is_ok()
    );
    requests.sort_unstable();
    println!(
        "API patch acknowledgement: median={:?}, max={:?}",
        requests[LIVE_SAMPLES / 2],
        requests.last()
    );
    timings.sort_unstable();
    let max = timings.last().copied().unwrap_or_default();
    println!(
        "API patch to UI AppState: median={:?}, max={max:?}, samples={LIVE_SAMPLES}",
        timings[LIVE_SAMPLES / 2]
    );
    assert!(
        max < UPDATE_BUDGET,
        "UI update latency exceeded budget: {max:?}"
    );
}

async fn populated_stores(
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

const WATCHER_HEAP_BUDGET: usize = 32 * 1024 * 1024;
const CANCEL_WAIT: Duration = Duration::from_millis(100);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires FLEET_E2E=1 and e2e:scale; run alone with --test-threads=1"]
async fn kind_scale_watcher_heap() {
    let (_directory, _sources, context) = scale_context();
    let profiler = dhat::Profiler::builder().testing().build();
    let settings = KubeSettings::default();
    let (manager, handle, subscriptions) = populated_stores(&context, &settings).await;
    let populated = dhat::HeapStats::get().curr_bytes;
    drop(subscriptions);
    tokio::time::sleep(CANCEL_WAIT).await;
    let emptied = dhat::HeapStats::get().curr_bytes;
    let retained = populated.saturating_sub(emptied);
    println!(
        "watcher stores + watch tasks released heap={retained} bytes, populated process heap={populated} bytes"
    );
    assert!(retained > 0, "watcher memory was not released");
    assert!(
        retained < WATCHER_HEAP_BUDGET,
        "watcher heap exceeds budget: {retained}"
    );
    manager.disconnect(&context.id);
    drop(handle);
    drop(manager);
    drop(profiler);
}

#[test]
#[ignore = "run alone in release mode with --test-threads=1"]
fn synthetic_snapshot_allocations() {
    let raw = synthetic_pods();
    let cluster = ClusterId::new("scale");
    let now = Timestamp::now();
    let profiler = dhat::Profiler::builder().testing().build();
    let rows = pods::pods(&cluster, &raw, &[], &[], now);
    let pod_stats = dhat::HeapStats::get();
    let index = PodIndex::build(&raw, now);
    let overview_stats = dhat::HeapStats::get();
    println!(
        "10k pod rows allocated {} blocks, {} bytes; overview added {} blocks, {} bytes",
        pod_stats.total_blocks,
        pod_stats.total_bytes,
        overview_stats.total_blocks - pod_stats.total_blocks,
        overview_stats.total_bytes - pod_stats.total_bytes
    );
    assert!(
        pod_stats.total_blocks <= MAX_POD_ALLOCATIONS,
        "pod rows allocated {} blocks",
        pod_stats.total_blocks
    );
    let index_allocations = overview_stats.total_blocks - pod_stats.total_blocks;
    assert!(
        index_allocations <= MAX_INDEX_ALLOCATIONS,
        "overview allocated {index_allocations} blocks"
    );
    black_box((&rows, &index));
    drop(index);
    drop(rows);
    drop(profiler);
}
