use std::time::{Duration, Instant};

use fleet_core::{
    Backend, ClusterId, Command, ContextRef, ExecEvent, ExecRequest, ExecTarget, LogLine,
    LogRequest, NewCluster, Receiver, ResourceRef, Sender, Sink, Snapshot, Source, Update,
};
use fleet_kube::LiveBackend;
use fleet_kube::discovery::{DiscoverySources, KubeContext, discover};
use fleet_kube::settings::KubeSettings;
use fleet_kube::store::ClusterStore;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{DeleteParams, ListParams, PostParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config, ResourceExt};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const E2E_VAR: &str = "FLEET_E2E";
const SUPPORT_DIR: &str = "support";
const CONTEXT: &str = "kind-fleet-a";
const WAIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(500);
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
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let update = tokio::time::timeout(left, self.updates.recv()).await;
            let Ok(Ok(update)) = update else {
                panic!("snapshot condition never met");
            };
            if let Update::Snapshot(snapshot) = update
                && done(&snapshot)
            {
                return *snapshot;
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
    let deadline = Instant::now() + WAIT;
    while pods.get_opt(DEBUG_POD).await.ok().flatten().is_some() {
        assert!(Instant::now() < deadline, "old pod lingers");
        tokio::time::sleep(POLL).await;
    }
    let created = pods.create(&PostParams::default(), &debug_pod()).await;
    assert!(created.is_ok(), "{created:?}");
    loop {
        let phase = pods
            .get(DEBUG_POD)
            .await
            .ok()
            .and_then(|pod| pod.status)
            .and_then(|status| status.phase);
        if phase.as_deref() == Some("Running") {
            return;
        }
        assert!(Instant::now() < deadline, "debug pod never ran");
        tokio::time::sleep(POLL).await;
    }
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
