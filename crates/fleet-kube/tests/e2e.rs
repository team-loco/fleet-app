use std::sync::Arc;
use std::time::Duration;

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
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const E2E_VAR: &str = "FLEET_E2E";
const CONTEXT: &str = "kind-fleet-a";
const NAMESPACE: &str = "default";
const POD: &str = "fleet-kube-e2e";
const CONFIGMAP: &str = "fleet-kube-e2e";
const IMAGE: &str = "busybox:1.37";
const HTTP_PORT: u16 = 8080;
const WAIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(500);
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
    let deadline = tokio::time::Instant::now() + WAIT;
    while handle.status() != ConnectionStatus::Connected {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{:?}",
            handle.status()
        );
        tokio::time::sleep(POLL).await;
    }
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
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let running = handle.objects::<Pod>().iter().any(|pod| {
            pod.name_any() == POD
                && pod.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Running")
        });
        if running {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "pod never ran");
        tokio::time::sleep(POLL).await;
    }
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
    let deadline = tokio::time::Instant::now() + WAIT;
    while pod_api.get_opt(POD).await.ok().flatten().is_some() {
        assert!(tokio::time::Instant::now() < deadline, "old pod lingers");
        tokio::time::sleep(POLL).await;
    }
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
    let Ok(yaml) = handle.get_yaml(&target, true).await else {
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
            .delete(&configmap, PropagationPolicy::Background)
            .await
            .is_ok()
    );
    assert!(
        handle
            .delete(&target, PropagationPolicy::Background)
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
    let select = Command::SelectCluster(Some(ClusterId::new(CONTEXT)));
    assert!(commands.send_blocking(select).is_ok());
    let page = Command::SelectPage(Page::Deployments);
    assert!(commands.send_blocking(page).is_ok());
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        assert!(std::time::Instant::now() < deadline, "no healthy snapshot");
        let Ok(Update::Snapshot(snapshot)) = updates.recv_blocking() else {
            panic!("backend stopped");
        };
        let Some(cluster) = snapshot.cluster(&ClusterId::new(CONTEXT)) else {
            continue;
        };
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
