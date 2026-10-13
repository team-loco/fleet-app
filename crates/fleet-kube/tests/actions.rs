use std::time::Instant;

use fleet_core::flux::{FluxAction, FluxKey, FluxKind};
use fleet_core::{
    Backend, ClusterId, Command, ContextRef, ExecEvent, ExecRequest, ExecTarget, LogLine,
    LogRequest, NewCluster, Receiver, ResourceRef, Sender, Sink, Snapshot, Source, Update,
};
use fleet_kube::LiveBackend;
use fleet_kube::discovery::{DiscoverySources, KubeContext, discover};
use fleet_kube::settings::KubeSettings;
use fleet_kube::store::ClusterStore;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{ConfigMap, Node, Pod, PodTemplateSpec};
use kube::api::{ApiResource, DynamicObject, GroupVersionKind};
use kube::api::{DeleteParams, ListParams, PostParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config, ResourceExt};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod common;

use common::{WAIT, eventually};

const E2E_VAR: &str = "FLEET_E2E";
const SUPPORT_DIR: &str = "support";
const CONTEXT: &str = "kind-fleet-a";
const CRASHING_NAMESPACE: &str = "payments";
const CRASHING_SELECTOR: &str = "app.kubernetes.io/name=checkout-api";
const CRASHING_CONTAINER: &str = "podinfo";
const PODINFO_NAMESPACE: &str = "apps";
const PODINFO: &str = "podinfo";
const PODINFO_SELECTOR: &str = "app.kubernetes.io/name=podinfo";
const PODINFO_CONTAINER: &str = "podinfo";
const PODINFO_PORT: u16 = 9898;
const DEBUG_NAMESPACE: &str = "default";
const DEBUG_POD: &str = "fleet-actions-e2e";
const DEBUG_IMAGE: &str = "busybox:1.37";
const ECHO_MARKER: &str = "fleet-e2e-exec";

fn enabled() -> bool {
    std::env::var(E2E_VAR).is_ok_and(|value| value == "1")
}

fn kind_context() -> Option<KubeContext> {
    discover(&DiscoverySources::from_env())
        .contexts
        .into_iter()
        .find(|context| context.id.context == CONTEXT)
}

fn kind_only_home(context: &KubeContext) -> tempfile::TempDir {
    let Ok(mut config) = Kubeconfig::read_from(&context.id.file) else {
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

struct Harness {
    _home: tempfile::TempDir,
    commands: Sender<Command>,
    updates: Receiver<Update>,
    client: Client,
}

fn cluster() -> ClusterId {
    ClusterId::new(CONTEXT)
}

fn resource(kind: &str, namespace: Option<&str>, name: &str) -> ResourceRef {
    ResourceRef {
        cluster: cluster(),
        kind: kind.into(),
        namespace: namespace.map(Into::into),
        name: name.into(),
    }
}

impl Harness {
    async fn start() -> Option<Self> {
        if !enabled() {
            return None;
        }
        let context = kind_context()?;
        let home = kind_only_home(&context);
        let options = KubeConfigOptions {
            context: Some(CONTEXT.to_owned()),
            ..KubeConfigOptions::default()
        };
        let Ok(kubeconfig) = Kubeconfig::read_from(home.path().join(".kube").join("config")) else {
            panic!("kind kubeconfig");
        };
        let Ok(config) = Config::from_custom_kubeconfig(kubeconfig, &options).await else {
            panic!("kind client config");
        };
        let Ok(client) = Client::try_from(config) else {
            panic!("kind client");
        };
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
        let harness = Self {
            _home: home,
            commands,
            updates,
            client,
        };
        harness
            .snapshot_until(|snapshot| {
                snapshot
                    .cluster(&cluster())
                    .is_some_and(|summary| summary.nodes.is_some())
            })
            .await;
        Some(harness)
    }

    fn send(&self, command: Command) {
        assert!(self.commands.try_send(command).is_ok());
    }

    async fn snapshot_until(&self, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
        let deadline = Instant::now() + WAIT;
        let mut last = None;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let update = tokio::time::timeout(left, self.updates.recv()).await;
            let Ok(Ok(update)) = update else {
                panic!("snapshot condition never met; last cluster summary: {last:#?}");
            };
            if let Update::Snapshot(snapshot) = update {
                if done(&snapshot) {
                    return *snapshot;
                }
                last = snapshot.cluster(&cluster()).cloned();
            }
        }
    }

    async fn first_pod(&self, namespace: &str, selector: &str) -> String {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let Ok(listed) = pods.list(&ListParams::default().labels(selector)).await else {
            panic!("list pods");
        };
        let Some(pod) = listed.items.first() else {
            panic!("no pod for {selector}");
        };
        pod.name_any()
    }
}

async fn collect_logs(lines: &Receiver<LogLine>) -> Vec<LogLine> {
    let mut collected = Vec::new();
    while let Ok(Ok(line)) = tokio::time::timeout(WAIT, lines.recv()).await {
        collected.push(line);
    }
    collected
}

#[tokio::test(flavor = "multi_thread")]
async fn previous_logs_of_a_crashing_pod_end_with_its_exit() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let pod = harness
        .first_pod(CRASHING_NAMESPACE, CRASHING_SELECTOR)
        .await;
    let (sink, lines) = Sink::channel();
    harness.send(Command::StreamLogs(LogRequest {
        pod: resource("Pod", Some(CRASHING_NAMESPACE), &pod),
        container: CRASHING_CONTAINER.into(),
        previous: true,
        sink,
    }));
    let collected = collect_logs(&lines).await;
    let Some(last) = collected.last() else {
        panic!("no previous logs");
    };
    assert!(last.time.is_none(), "{collected:#?}");
    assert!(
        last.message.starts_with("── container exited"),
        "{collected:#?}"
    );
    assert!(
        collected.iter().any(|line| line.time.is_some()),
        "{collected:#?}"
    );
}

async fn exec_events(events: &Receiver<ExecEvent>) -> (Vec<ExecEvent>, String) {
    let mut seen = Vec::new();
    let mut output = String::new();
    while let Ok(Ok(event)) = tokio::time::timeout(WAIT, events.recv()).await {
        if let ExecEvent::Output(bytes) = &event {
            output.push_str(&String::from_utf8_lossy(bytes));
        }
        let exited = matches!(event, ExecEvent::Exited { .. });
        seen.push(event);
        if exited {
            break;
        }
    }
    (seen, output)
}

fn run_shell(harness: &Harness, target: ExecTarget) -> (Sender<Vec<u8>>, Receiver<ExecEvent>) {
    let (input, source) = Source::channel();
    let (output, events) = Sink::channel();
    harness.send(Command::Exec(ExecRequest {
        target,
        input: source,
        output,
    }));
    (input, events)
}

#[tokio::test(flavor = "multi_thread")]
async fn exec_runs_a_shell_with_a_tty() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let pod = harness.first_pod(PODINFO_NAMESPACE, PODINFO_SELECTOR).await;
    let target = ExecTarget {
        pod: resource("Pod", Some(PODINFO_NAMESPACE), &pod),
        container: PODINFO_CONTAINER.into(),
        debug_image: None,
    };
    let (input, events) = run_shell(&harness, target);
    let script = format!("echo {ECHO_MARKER}-$((40 + 2))\rexit\r");
    assert!(input.send(script.into_bytes()).await.is_ok());
    let (seen, output) = exec_events(&events).await;
    assert!(
        matches!(seen.first(), Some(ExecEvent::Started { .. })),
        "{seen:?}"
    );
    assert!(output.contains(&format!("{ECHO_MARKER}-42")), "{output}");
    assert_eq!(seen.last(), Some(&ExecEvent::Exited { code: Some(0) }));
}

fn debug_pod() -> Pod {
    let Ok(pod) = serde_json::from_value(json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": { "name": DEBUG_POD, "namespace": DEBUG_NAMESPACE },
        "spec": {
            "terminationGracePeriodSeconds": 0,
            "containers": [{
                "name": "main",
                "image": DEBUG_IMAGE,
                "command": ["sleep", "3600"]
            }]
        }
    })) else {
        panic!("pod fixture");
    };
    pod
}

async fn recreate(pods: &Api<Pod>) {
    let _ = pods.delete(DEBUG_POD, &DeleteParams::default()).await;
    eventually(
        "old debug pod deleted",
        || async { pods.get_opt(DEBUG_POD).await.ok().flatten() },
        Option::is_none,
    )
    .await;
    let created = pods.create(&PostParams::default(), &debug_pod()).await;
    assert!(created.is_ok(), "{created:?}");
    eventually(
        "debug pod running",
        || async { pods.get(DEBUG_POD).await.ok().and_then(|pod| pod.status) },
        |status| status.as_ref().and_then(|status| status.phase.as_deref()) == Some("Running"),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn debug_shell_attaches_an_ephemeral_container() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let pods: Api<Pod> = Api::namespaced(harness.client.clone(), DEBUG_NAMESPACE);
    recreate(&pods).await;
    let target = ExecTarget {
        pod: resource("Pod", Some(DEBUG_NAMESPACE), DEBUG_POD),
        container: "main".into(),
        debug_image: Some(DEBUG_IMAGE.into()),
    };
    let (input, events) = run_shell(&harness, target);
    let Ok(Ok(started)) = tokio::time::timeout(WAIT, events.recv()).await else {
        panic!("debug session never started");
    };
    assert!(
        matches!(&started, ExecEvent::Started { note } if note.contains("Debug container: debugger-")),
        "{started:?}"
    );
    let script = format!("echo {ECHO_MARKER}\rexit\r");
    assert!(input.send(script.into_bytes()).await.is_ok());
    let (seen, output) = exec_events(&events).await;
    let _ = pods.delete(DEBUG_POD, &DeleteParams::default()).await;
    assert!(output.contains(ECHO_MARKER), "{output}");
    assert!(
        matches!(seen.last(), Some(ExecEvent::Exited { .. })),
        "{seen:?}"
    );
}

async fn http_get(port: u16) -> String {
    let Ok(mut socket) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await else {
        panic!("connect to forward");
    };
    let request = "GET / HTTP/1.0\r\nHost: localhost\r\n\r\n";
    assert!(socket.write_all(request.as_bytes()).await.is_ok());
    let mut response = String::new();
    let read = tokio::time::timeout(WAIT, socket.read_to_string(&mut response)).await;
    assert!(read.is_ok(), "forward read timed out");
    response
}

#[tokio::test(flavor = "multi_thread")]
async fn port_forward_a_service_then_stop_it() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let service = resource("Service", Some(PODINFO_NAMESPACE), PODINFO);
    harness.send(Command::PortForward {
        service: service.clone(),
        local: 0,
        remote: PODINFO_PORT,
    });
    let snapshot = harness
        .snapshot_until(|snapshot| !snapshot.forwards.is_empty())
        .await;
    let Some(forward) = snapshot.forwards.first().cloned() else {
        panic!("forward listed");
    };
    assert_eq!(forward.target, service);
    assert_ne!(forward.local, 0);
    let response = http_get(forward.local).await;
    assert!(response.starts_with("HTTP/1.0 200"), "{response}");
    assert!(response.contains("\"hostname\""), "{response}");
    harness.send(Command::StopPortForward {
        local: forward.local,
    });
    harness
        .snapshot_until(|snapshot| snapshot.forwards.is_empty())
        .await;
    let refused = tokio::net::TcpStream::connect(("127.0.0.1", forward.local)).await;
    assert!(refused.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn port_forward_failures_become_notices() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    harness.send(Command::PortForward {
        service: resource("Service", Some(PODINFO_NAMESPACE), "no-such-service"),
        local: 0,
        remote: PODINFO_PORT,
    });
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(Ok(update)) = tokio::time::timeout(left, harness.updates.recv()).await else {
            panic!("no notice");
        };
        if let Update::Notice(notice) = update {
            assert_eq!(notice.title, "Port-forward svc/no-such-service failed");
            let detail = notice.detail.unwrap_or_default();
            assert!(detail.starts_with("kind-fleet-a · Not found:"), "{detail}");
            return;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn port_forward_a_pod_port_from_the_pods_page() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let pod = harness.first_pod(PODINFO_NAMESPACE, PODINFO_SELECTOR).await;
    let target = resource("Pod", Some(PODINFO_NAMESPACE), &pod);
    harness.send(Command::PortForward {
        service: target.clone(),
        local: 0,
        remote: PODINFO_PORT,
    });
    let snapshot = harness
        .snapshot_until(|snapshot| !snapshot.forwards.is_empty())
        .await;
    let Some(forward) = snapshot.forwards.first().cloned() else {
        panic!("forward listed");
    };
    assert_eq!(forward.target, target);
    let response = http_get(forward.local).await;
    assert!(response.starts_with("HTTP/1.0 200"), "{response}");
    harness.send(Command::StopPortForward {
        local: forward.local,
    });
    harness
        .snapshot_until(|snapshot| snapshot.forwards.is_empty())
        .await;
}

const WEB_NAMESPACE: &str = "web";
const FLUX_NAMESPACE: &str = "flux-system";
const KUSTOMIZATION: &str = "apps";
const NODE: &str = "fleet-a-control-plane";
const SCRATCH_CONFIGMAP: &str = "fleet-actions-e2e";
const RESTARTED_AT: &str = "kubectl.kubernetes.io/restartedAt";
const REQUESTED_AT: &str = "reconcile.fluxcd.io/requestedAt";

async fn replicas(deployments: &Api<Deployment>) -> Option<i32> {
    let deployment = deployments.get(PODINFO).await.ok()?;
    deployment.spec?.replicas
}

async fn template(deployments: &Api<Deployment>) -> Option<PodTemplateSpec> {
    let deployment = deployments.get(PODINFO).await.ok()?;
    Some(deployment.spec?.template)
}

fn no_notices(harness: &Harness) {
    while let Ok(update) = harness.updates.try_recv() {
        if let Update::Notice(notice) = update {
            panic!("unexpected notice: {notice:?}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn scale_up_then_back_down() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let deployments: Api<Deployment> = Api::namespaced(harness.client.clone(), WEB_NAMESPACE);
    let Some(before) = replicas(&deployments).await else {
        panic!("web/podinfo replicas");
    };
    let target = resource("Deployment", Some(WEB_NAMESPACE), PODINFO);
    let up = u32::try_from(before + 1).unwrap_or_default();
    harness.send(Command::Scale {
        target: target.clone(),
        replicas: up,
    });
    eventually(
        "scale up",
        || replicas(&deployments),
        |observed| *observed == Some(before + 1),
    )
    .await;
    harness.send(Command::Scale {
        target,
        replicas: u32::try_from(before).unwrap_or_default(),
    });
    eventually(
        "scale down",
        || replicas(&deployments),
        |observed| *observed == Some(before),
    )
    .await;
    no_notices(&harness);
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_then_roll_back_to_the_original_template() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let deployments: Api<Deployment> = Api::namespaced(harness.client.clone(), WEB_NAMESPACE);
    let Some(original) = template(&deployments).await else {
        panic!("web/podinfo template");
    };
    let target = resource("Deployment", Some(WEB_NAMESPACE), PODINFO);
    harness.send(Command::RestartRollout(target.clone()));
    eventually(
        "restart annotation",
        || template(&deployments),
        |observed| {
            let annotations = observed
                .as_ref()
                .and_then(|t| t.metadata.as_ref())
                .and_then(|meta| meta.annotations.as_ref());
            annotations.is_some_and(|annotations| annotations.contains_key(RESTARTED_AT))
        },
    )
    .await;
    harness.send(Command::Rollback {
        target,
        revision: 0,
    });
    eventually(
        "rollback",
        || template(&deployments),
        |observed| observed.as_ref() == Some(&original),
    )
    .await;
    no_notices(&harness);
}

#[tokio::test(flavor = "multi_thread")]
async fn cordon_then_uncordon_the_node() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let nodes: Api<Node> = Api::all(harness.client.clone());
    let unschedulable = || async {
        nodes
            .get(NODE)
            .await
            .ok()
            .and_then(|node| node.spec?.unschedulable)
            .unwrap_or(false)
    };
    let target = resource("Node", None, NODE);
    harness.send(Command::Cordon(target.clone()));
    eventually("cordon", unschedulable, |observed| *observed).await;
    harness.send(Command::Uncordon(target));
    eventually("uncordon", unschedulable, |observed| !*observed).await;
    no_notices(&harness);
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_waits_until_the_object_is_gone() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let configmaps: Api<ConfigMap> = Api::namespaced(harness.client.clone(), DEBUG_NAMESPACE);
    let Ok(scratch) = serde_json::from_value::<ConfigMap>(json!({
        "metadata": { "name": SCRATCH_CONFIGMAP },
        "data": { "key": "value" }
    })) else {
        panic!("configmap fixture");
    };
    let _ = configmaps.create(&PostParams::default(), &scratch).await;
    harness.send(Command::Delete(resource(
        "ConfigMap",
        Some(DEBUG_NAMESPACE),
        SCRATCH_CONFIGMAP,
    )));
    eventually(
        "delete",
        || async { configmaps.get_opt(SCRATCH_CONFIGMAP).await.ok().flatten() },
        Option::is_none,
    )
    .await;
    no_notices(&harness);
}

#[tokio::test(flavor = "multi_thread")]
async fn flux_reconcile_requests_a_sync() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    let key = FluxKey::new(
        CONTEXT,
        FluxKind::Kustomization,
        FLUX_NAMESPACE,
        KUSTOMIZATION,
    );
    let resource = ApiResource::from_gvk(&GroupVersionKind::gvk(
        "kustomize.toolkit.fluxcd.io",
        "v1",
        "Kustomization",
    ));
    let kustomizations: Api<DynamicObject> =
        Api::namespaced_with(harness.client.clone(), FLUX_NAMESPACE, &resource);
    let requested = || async {
        kustomizations
            .get(KUSTOMIZATION)
            .await
            .ok()
            .and_then(|object| object.annotations().get(REQUESTED_AT).cloned())
    };
    let before = requested().await;
    harness.send(Command::Flux(key, FluxAction::ReconcileWithSource));
    eventually("reconcile request", requested, |now| {
        now.is_some() && *now != before
    })
    .await;
    no_notices(&harness);
}

#[tokio::test(flavor = "multi_thread")]
async fn unsupported_actions_explain_themselves() {
    let Some(harness) = Harness::start().await else {
        return;
    };
    harness.send(Command::Scale {
        target: resource("DaemonSet", Some("kube-system"), "kube-proxy"),
        replicas: 2,
    });
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(Ok(update)) = tokio::time::timeout(left, harness.updates.recv()).await else {
            panic!("no notice");
        };
        if let Update::Notice(notice) = update {
            assert_eq!(notice.title, "Scale ds/kube-proxy failed");
            assert_eq!(
                notice.detail.as_deref(),
                Some("kind-fleet-a · Scale is not supported for DaemonSet")
            );
            return;
        }
    }
}
