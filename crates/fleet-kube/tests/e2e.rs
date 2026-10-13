use fleet_core::{Backend, ClusterId, Command, ContextRef, NewCluster, Page, Update};
use fleet_kube::LiveBackend;
use fleet_kube::checks::{CheckKind, CheckStatus, Verdict, run_checks};
use fleet_kube::discovery::{DiscoverySources, KubeContext, discover};
use fleet_kube::manager::{ClusterEvent, ClusterHandle, ClusterManager, ConnectionStatus};
use fleet_kube::ops::{ExecExit, ExecRequest, LogRequest, PatchBody, object_ref};
use fleet_kube::provider::Provider;
use fleet_kube::resources::WatchKind;
use fleet_kube::settings::KubeSettings;
use fleet_kube::shell_env::ShellEnv;
use fleet_kube::store::ClusterStore;
use k8s_openapi::api::core::v1::{ConfigMap, Pod};
use kube::api::{DeleteParams, PartialObjectMeta, PostParams, PropagationPolicy};
use kube::{Api, ResourceExt};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod common;

use common::{WAIT, eventually};

const E2E_VAR: &str = "FLEET_E2E";
const CONTEXT: &str = "kind-fleet-a";
const NAMESPACE: &str = "default";
const POD: &str = "fleet-kube-e2e";
const CONFIGMAP: &str = "fleet-kube-e2e";
const IMAGE: &str = "busybox:1.37";
const HTTP_PORT: u16 = 8080;
const GREETING: &str = "hello from fleet";
const SUPPORT_DIR: &str = "support";

fn enabled() -> bool {
    std::env::var(E2E_VAR).is_ok_and(|value| value == "1")
}

fn kind_context() -> Option<KubeContext> {
    let discovery = discover(&DiscoverySources::from_env());
    discovery
        .contexts
        .into_iter()
        .find(|context| context.id.context == CONTEXT)
}

async fn connected(manager: &ClusterManager, context: KubeContext) -> ClusterHandle {
    let handle = manager.connect(context);
    eventually(
        "cluster connected",
        || async { handle.status() },
        |status| *status == ConnectionStatus::Connected,
    )
    .await;
    handle
}

fn test_pod() -> Pod {
    let script = format!(
        "echo {GREETING}; while true; do printf 'HTTP/1.1 200 OK\\r\\nContent-Length: 2\\r\\n\\r\\nok' | nc -l -p {HTTP_PORT}; done"
    );
    let Ok(pod) = serde_json::from_value(json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": { "name": POD, "namespace": NAMESPACE },
        "spec": {
            "terminationGracePeriodSeconds": 0,
            "containers": [{
                "name": "main",
                "image": IMAGE,
                "command": ["sh", "-c", script],
                "ports": [{ "containerPort": HTTP_PORT }]
            }]
        }
    })) else {
        panic!("pod fixture");
    };
    pod
}

async fn wait_running(handle: &ClusterHandle) {
    eventually(
        "pod running",
        || async {
            let pods = handle.objects::<Pod>();
            pods.iter().find(|pod| pod.name_any() == POD).cloned()
        },
        |pod| {
            let phase = pod
                .as_ref()
                .and_then(|pod| pod.status.as_ref())
                .and_then(|status| status.phase.as_deref());
            phase == Some("Running")
        },
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn checks_report_ready_for_kind() {
    if !enabled() {
        return;
    }
    let Some(context) = kind_context() else {
        return;
    };
    let env = ShellEnv::from_process(String::new());
    let report = run_checks(&context, &env, &KubeSettings::default()).await;
    assert_eq!(report.verdict, Verdict::Ready, "{report:#?}");
    assert_eq!(report.facts.provider, Provider::Kind);
    let flux = report
        .checks
        .iter()
        .find(|check| check.kind == CheckKind::Flux);
    assert_eq!(flux.map(|c| c.status), Some(CheckStatus::Ok), "{report:#?}");
    assert!(report.facts.flux.is_some());
    assert!(report.facts.version.is_some());
}

async fn recreate_pod(handle: &ClusterHandle) {
    let Ok(client) = handle.client() else {
        panic!("client after connect");
    };
    let pod_api: Api<Pod> = Api::namespaced(client, NAMESPACE);
    let _ = pod_api.delete(POD, &DeleteParams::default()).await;
    eventually(
        "old pod deleted",
        || async { pod_api.get_opt(POD).await.ok().flatten() },
        Option::is_none,
    )
    .await;
    let created = pod_api.create(&PostParams::default(), &test_pod()).await;
    assert!(created.is_ok(), "{created:?}");
    wait_running(handle).await;
}

async fn check_logs(handle: &ClusterHandle) {
    let request = LogRequest {
        namespace: NAMESPACE.to_owned(),
        pod: POD.to_owned(),
        tail_lines: Some(10),
        ..LogRequest::default()
    };
    let Ok(logs) = handle.logs(&request).await else {
        panic!("logs");
    };
    let first = tokio::time::timeout(WAIT, logs.lines.recv()).await;
    assert!(
        matches!(first, Ok(Ok(Ok(ref line))) if line == GREETING),
        "{first:?}"
    );
}

async fn check_exec(handle: &ClusterHandle) {
    let exec = ExecRequest {
        namespace: NAMESPACE.to_owned(),
        pod: POD.to_owned(),
        container: None,
        command: vec!["echo".to_owned(), "fleet-exec".to_owned()],
        tty: false,
    };
    let Ok(session) = handle.exec(&exec).await else {
        panic!("exec");
    };
    let mut output = Vec::new();
    while let Ok(Ok(chunk)) = tokio::time::timeout(WAIT, session.output.recv()).await {
        output.extend(chunk);
    }
    assert_eq!(String::from_utf8_lossy(&output).trim(), "fleet-exec");
    let exit = tokio::time::timeout(WAIT, session.exit.recv()).await;
    assert!(matches!(exit, Ok(Ok(ExecExit::Success))), "{exit:?}");
}

async fn check_port_forward(handle: &ClusterHandle) {
    let Ok(forward) = handle.port_forward(NAMESPACE, POD, HTTP_PORT, 0).await else {
        panic!("port-forward");
    };
    assert_eq!(handle.port_forwards(), vec![forward.clone()]);
    let Ok(mut socket) = tokio::net::TcpStream::connect(forward.local_addr).await else {
        panic!("connect to forward");
    };
    assert!(socket.write_all(b"GET / HTTP/1.0\r\n\r\n").await.is_ok());
    let mut response = String::new();
    let read = tokio::time::timeout(WAIT, socket.read_to_string(&mut response)).await;
    assert!(read.is_ok(), "forward read timed out");
    assert!(response.ends_with("ok"), "{response}");
    assert!(handle.stop_port_forward(forward.id));
    assert_eq!(handle.port_forwards(), Vec::new());
}

async fn check_patch_yaml_apply(handle: &ClusterHandle) {
    let target = object_ref::<Pod>(Some(NAMESPACE), POD);
    let patch = PatchBody::Merge(json!({ "metadata": { "labels": { "fleet-e2e": "patched" } } }));
    let Ok(patched) = handle.patch(&target, patch).await else {
        panic!("patch");
    };
    assert_eq!(
        patched.labels().get("fleet-e2e").map(String::as_str),
        Some("patched")
    );
    let Ok(yaml) = handle.get_yaml(&target).await else {
        panic!("yaml");
    };
    assert!(yaml.contains("fleet-e2e: patched"));
    assert!(!yaml.contains("managedFields"));

    let manifest = format!(
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {CONFIGMAP}\n  namespace: {NAMESPACE}\ndata:\n  key: value\n"
    );
    let applied = handle.apply_yaml(&manifest, false).await;
    assert!(applied.is_ok(), "{applied:?}");
    let configmap = object_ref::<ConfigMap>(Some(NAMESPACE), CONFIGMAP);
    assert!(
        handle
            .delete(&configmap, PropagationPolicy::Background, None)
            .await
            .is_ok()
    );
    assert!(
        handle
            .delete(&target, PropagationPolicy::Background, None)
            .await
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_logs_exec_forward_patch() {
    if !enabled() {
        return;
    }
    let Some(context) = kind_context() else {
        return;
    };
    let env = Arc::new(ShellEnv::from_process(String::new()));
    let runtime = tokio::runtime::Handle::current();
    let (manager, events) = ClusterManager::new(runtime, KubeSettings::default(), env);
    let handle = connected(&manager, context).await;
    let pods = handle.subscribe(WatchKind::Pods);
    let configmaps = handle.subscribe(WatchKind::ConfigMaps);
    recreate_pod(&handle).await;

    let mut saw_pods_changed = false;
    while let Ok(event) = events.try_recv() {
        if let ClusterEvent::Changed { kinds, .. } = event {
            saw_pods_changed |= kinds.contains(&WatchKind::Pods);
        }
    }
    assert!(saw_pods_changed);
    assert_ne!(handle.objects::<PartialObjectMeta<ConfigMap>>().len(), 0);

    check_logs(&handle).await;
    check_exec(&handle).await;
    check_port_forward(&handle).await;
    check_patch_yaml_apply(&handle).await;

    drop(pods);
    drop(configmaps);
    assert_eq!(handle.watched_kinds(), Vec::new());
}

fn kind_only_home(context: &KubeContext) -> tempfile::TempDir {
    let Ok(mut config) = kube::config::Kubeconfig::read_from(&context.id.file) else {
        panic!("kubeconfig readable");
    };
    config.contexts.retain(|named| named.name == CONTEXT);
    config
        .clusters
        .retain(|named| named.name == context.cluster);
    config
        .auth_infos
        .retain(|named| Some(&named.name) == context.user.as_ref());
    config.current_context = Some(CONTEXT.to_owned());
    let Ok(home) = tempfile::tempdir() else {
        panic!("tempdir");
    };
    let kube_dir = home.path().join(".kube");
    assert!(std::fs::create_dir_all(&kube_dir).is_ok());
    let Ok(yaml) = serde_norway::to_string(&config) else {
        panic!("serialize kubeconfig");
    };
    assert!(std::fs::write(kube_dir.join("config"), yaml).is_ok());
    home
}

#[test]
fn live_backend_publishes_kind_cluster() {
    if !enabled() {
        return;
    }
    let Some(context) = kind_context() else {
        return;
    };
    let home = kind_only_home(&context);
    let sources = DiscoverySources {
        kubeconfig_env: None,
        home: Some(home.path().to_path_buf()),
        folders: Vec::new(),
    };
    let mut store = ClusterStore::empty(&home.path().join(SUPPORT_DIR));
    let added = NewCluster {
        context: ContextRef {
            file: home.path().join(".kube").join("config"),
            context: CONTEXT.to_owned(),
        },
        name: CONTEXT.to_owned(),
    };
    assert!(store.add(&[added]).is_ok());
    let backend = LiveBackend::new(KubeSettings::default(), sources, store);
    let (updates_tx, updates) = async_channel::unbounded();
    let (commands, commands_rx) = async_channel::unbounded();
    Box::new(backend).start(updates_tx, commands_rx);
    let mut everywhere = fleet_core::Preferences::default();
    everywhere.select_namespace(
        fleet_core::ResourceKind::Deployment.name(),
        fleet_core::settings::NamespaceSelection::All,
    );
    let all_namespaces = Command::SetPreferences(ClusterId::new(CONTEXT), everywhere);
    assert!(commands.send_blocking(all_namespaces).is_ok());
    let select = Command::SelectCluster(Some(ClusterId::new(CONTEXT)));
    assert!(commands.send_blocking(select).is_ok());
    let page = Command::SelectPage(Page::Deployments);
    assert!(commands.send_blocking(page).is_ok());
    let deadline = std::time::Instant::now() + WAIT;
    let mut last = None;
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "no healthy snapshot; last cluster summary: {last:#?}"
        );
        let Ok(Update::Snapshot(snapshot)) = updates.recv_blocking() else {
            panic!("backend stopped");
        };
        let Some(cluster) = snapshot.cluster(&ClusterId::new(CONTEXT)) else {
            continue;
        };
        last = Some(cluster.clone());
        let deployments = cluster.nav.get(&Page::Deployments).and_then(|c| c.total);
        if cluster.nodes == Some(1) && deployments.is_some_and(|n| n > 0) {
            assert_eq!(cluster.distro, "kind");
            assert_eq!(cluster.region, "local");
            assert!(cluster.version.is_some());
            assert_eq!(snapshot.clusters.len(), 1);
            return;
        }
    }
}

const SCOPED_NAMESPACE: &str = "kube-system";
const OTHER_NAMESPACE: &str = "flux-system";

fn deployment_namespaces(snapshot: &fleet_core::Snapshot) -> Vec<String> {
    let mut namespaces: Vec<String> = snapshot
        .detail(&ClusterId::new(CONTEXT))
        .map(|detail| detail.resources(fleet_core::ResourceKind::Deployment))
        .unwrap_or_default()
        .iter()
        .map(|row| row.namespace.clone())
        .collect();
    namespaces.sort();
    namespaces.dedup();
    namespaces
}

fn wait_for(
    updates: &async_channel::Receiver<Update>,
    what: &str,
    done: impl Fn(&[String]) -> bool,
) {
    let deadline = std::time::Instant::now() + WAIT;
    let mut last = Vec::new();
    while std::time::Instant::now() < deadline {
        let Ok(Update::Snapshot(snapshot)) = updates.recv_blocking() else {
            panic!("backend stopped");
        };
        last = deployment_namespaces(&snapshot);
        if done(&last) {
            return;
        }
    }
    panic!("{what}; last deployment namespaces: {last:?}");
}

#[test]
fn live_backend_watches_only_the_chosen_namespace() {
    if !enabled() {
        return;
    }
    let Some(context) = kind_context() else {
        return;
    };
    let home = kind_only_home(&context);
    let sources = DiscoverySources {
        kubeconfig_env: None,
        home: Some(home.path().to_path_buf()),
        folders: Vec::new(),
    };
    let mut store = ClusterStore::empty(&home.path().join(SUPPORT_DIR));
    let added = NewCluster {
        context: ContextRef {
            file: home.path().join(".kube").join("config"),
            context: CONTEXT.to_owned(),
        },
        name: CONTEXT.to_owned(),
    };
    assert!(store.add(&[added]).is_ok());
    let backend = LiveBackend::new(KubeSettings::default(), sources, store);
    let (updates_tx, updates) = async_channel::unbounded();
    let (commands, commands_rx) = async_channel::unbounded();
    Box::new(backend).start(updates_tx, commands_rx);
    let cluster = ClusterId::new(CONTEXT);
    let choose = |namespace: Option<&str>| {
        let mut preferences = fleet_core::Preferences::default();
        preferences.select_namespace(
            fleet_core::ResourceKind::Deployment.name(),
            fleet_core::settings::NamespaceSelection::of(namespace),
        );
        Command::SetPreferences(cluster.clone(), preferences)
    };
    for command in [
        choose(Some(SCOPED_NAMESPACE)),
        Command::SelectCluster(Some(cluster.clone())),
        Command::SelectPage(Page::Deployments),
    ] {
        assert!(commands.send_blocking(command).is_ok());
    }
    wait_for(&updates, "only kube-system deployments", |namespaces| {
        namespaces == [SCOPED_NAMESPACE]
    });
    assert!(commands.send_blocking(choose(None)).is_ok());
    wait_for(&updates, "deployments from every namespace", |namespaces| {
        namespaces.iter().any(|ns| ns == OTHER_NAMESPACE)
            && namespaces.iter().any(|ns| ns == SCOPED_NAMESPACE)
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn namespaced_watches_list_one_namespace_beside_cluster_wide_ones() {
    if !enabled() {
        return;
    }
    let Some(context) = kind_context() else {
        return;
    };
    let env = Arc::new(ShellEnv::from_process(String::new()));
    let runtime = tokio::runtime::Handle::current();
    let (manager, _events) = ClusterManager::new(runtime, KubeSettings::default(), env);
    let handle = connected(&manager, context).await;
    let scoped = handle.subscribe_in(WatchKind::Deployments, Some(SCOPED_NAMESPACE.to_owned()));
    assert_eq!(scoped.namespace(), Some(SCOPED_NAMESPACE));
    let namespaces = || {
        let mut found: Vec<String> = handle
            .objects::<k8s_openapi::api::apps::v1::Deployment>()
            .iter()
            .filter_map(|d| d.metadata.namespace.clone())
            .collect();
        found.sort();
        found.dedup();
        found
    };
    eventually(
        "scoped deployments",
        || async { namespaces() },
        |found| found == &[SCOPED_NAMESPACE],
    )
    .await;
    assert_eq!(handle.cluster_wide_kinds(), Vec::<WatchKind>::new());
    let everywhere = handle.subscribe(WatchKind::Deployments);
    eventually(
        "every deployment",
        || async { namespaces() },
        |found| found.iter().any(|ns| ns == OTHER_NAMESPACE),
    )
    .await;
    drop(everywhere);
    eventually(
        "back to one namespace",
        || async { namespaces() },
        |found| found == &[SCOPED_NAMESPACE],
    )
    .await;
    drop(scoped);
    assert_eq!(handle.watched_kinds(), Vec::<WatchKind>::new());
}
