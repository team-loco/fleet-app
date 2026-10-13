use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use fleet_core::{
    CheckLine, CheckStatus, ClusterId, ClusterSummary, Command, ContextChoice, ContextRef, Counts,
    KubeconfigFile, NewCluster, Page, Sender, Snapshot, Status, Update, Verdict,
};

const CHECK_STEP: Duration = Duration::from_millis(160);
const DEFAULT_FILE: &str = "~/.kube/config";
const SANDBOX_FILE: &str = "~/.kube/eks/sandbox.yaml";
const ANALYTICS: &str = "gke_acme-data_us-central1_analytics";
const MINIKUBE: &str = "minikube";
const SANDBOX: &str = "arn:aws:eks:us-west-2:[ACCOUNT]:cluster/sandbox";
const ANALYTICS_NODES: u32 = 9;
const ANALYTICS_PODS: u32 = 212;

struct Seed {
    file: &'static str,
    context: &'static str,
    server: &'static str,
    auth: &'static str,
    selected: bool,
}

const SEEDS: [Seed; 3] = [
    Seed {
        file: DEFAULT_FILE,
        context: ANALYTICS,
        server: "https://34.118.[IP]",
        auth: "gke-gcloud-auth-plugin",
        selected: true,
    },
    Seed {
        file: DEFAULT_FILE,
        context: MINIKUBE,
        server: "https://127.0.0.1:52011",
        auth: "client certificate",
        selected: false,
    },
    Seed {
        file: SANDBOX_FILE,
        context: SANDBOX,
        server: "https://[ID].eks.amazonaws.com",
        auth: "aws eks get-token",
        selected: true,
    },
];

#[derive(Default)]
pub struct AddClusters {
    added: BTreeMap<ClusterId, ContextRef>,
}

impl AddClusters {
    pub fn handle(
        &mut self,
        command: &Command,
        fleet: &mut Snapshot,
        updates: &Sender<Update>,
    ) -> bool {
        match command {
            Command::OpenAddCluster(_) => {
                let files = self.files();
                send(updates, Update::Contexts(files));
            }
            Command::CheckContexts(ids) => simulate(updates, ids.clone()),
            Command::AddClusters(new) => {
                for cluster in new {
                    self.added
                        .insert(ClusterId::new(&cluster.name), cluster.context.clone());
                    fleet.clusters.push(summary(cluster));
                }
                send(updates, Update::Snapshot(Box::new(fleet.clone())));
            }
            Command::RemoveCluster(id) => {
                self.added.remove(id);
                fleet.clusters.retain(|cluster| &cluster.id != id);
                send(updates, Update::Snapshot(Box::new(fleet.clone())));
            }
            _ => return false,
        }
        true
    }

    pub fn files(&self) -> Vec<KubeconfigFile> {
        let mut files: Vec<KubeconfigFile> = Vec::new();
        let available = SEEDS
            .iter()
            .filter(|seed| !self.added.values().any(|id| id == &context_ref(seed)));
        for seed in available {
            let choice = ContextChoice {
                id: context_ref(seed),
                server: Some(seed.server.to_owned()),
                auth: seed.auth.to_owned(),
                selected: seed.selected,
            };
            match files.iter_mut().find(|file| file.label == seed.file) {
                Some(file) => file.contexts.push(choice),
                None => files.push(KubeconfigFile {
                    label: seed.file.to_owned(),
                    contexts: vec![choice],
                }),
            }
        }
        files
    }
}

fn send(updates: &Sender<Update>, update: Update) {
    if updates.send_blocking(update).is_err() {
        tracing::debug!("demo update dropped: UI closed");
    }
}

fn context_ref(seed: &Seed) -> ContextRef {
    ContextRef {
        file: PathBuf::from(seed.file),
        context: seed.context.to_owned(),
    }
}

fn line(status: CheckStatus, detail: &str, millis: u64) -> CheckLine {
    CheckLine {
        status,
        detail: detail.to_owned(),
        remedy: None,
        duration: Duration::from_millis(millis),
    }
}

pub fn script(context: &ContextRef) -> (Vec<CheckLine>, Verdict) {
    match context.context.as_str() {
        ANALYTICS => (
            vec![
                line(CheckStatus::Ok, "Reachable in 142 ms", 142),
                line(
                    CheckStatus::Ok,
                    "Kubernetes v1.30.4 · 9 nodes · 212 pods",
                    310,
                ),
                line(
                    CheckStatus::Ok,
                    "Flux v2.4.0 · 14 Kustomizations, 6 HelmReleases",
                    420,
                ),
                line(
                    CheckStatus::Ok,
                    "Metrics from monitoring/prometheus-k8s",
                    260,
                ),
                line(
                    CheckStatus::Warn,
                    "Can’t read Secrets, so env var values won’t resolve. Everything else works.",
                    180,
                ),
            ],
            Verdict::Ready,
        ),
        SANDBOX => {
            let mut expired = line(
                CheckStatus::Err,
                "aws eks get-token: ExpiredToken. Sign in again, then retry.",
                640,
            );
            expired.remedy = Some("aws sso login --profile sandbox".to_owned());
            (
                vec![line(CheckStatus::Ok, "Reachable in 88 ms", 88), expired],
                Verdict::CannotAuthenticate,
            )
        }
        _ => {
            let mut refused = line(
                CheckStatus::Err,
                "Can’t reach https://127.0.0.1:52011: connection refused.",
                3,
            );
            refused.remedy = Some("minikube start".to_owned());
            (vec![refused], Verdict::CannotReach)
        }
    }
}

fn simulate(updates: &Sender<Update>, ids: Vec<ContextRef>) {
    for id in ids {
        let updates = updates.clone();
        let spawned = thread::Builder::new()
            .name("fleet-demo-checks".into())
            .spawn(move || {
                let (lines, verdict) = script(&id);
                for line in lines {
                    thread::sleep(CHECK_STEP);
                    send(&updates, Update::Check(id.clone(), line));
                }
                send(&updates, Update::Checked(id, verdict));
            });
        if let Err(error) = spawned {
            tracing::error!(%error, "demo checks failed to start");
        }
    }
}

fn summary(cluster: &NewCluster) -> ClusterSummary {
    let analytics = cluster.context.context == ANALYTICS;
    let nav = BTreeMap::from([
        (Page::Nodes, Counts::total(ANALYTICS_NODES)),
        (Page::Pods, Counts::total(ANALYTICS_PODS)),
    ]);
    ClusterSummary {
        id: ClusterId::new(&cluster.name),
        name: cluster.name.clone(),
        status: Status::Ok,
        state: "Healthy".into(),
        distro: if analytics { "GKE" } else { "Kubernetes" }.into(),
        region: if analytics { "us-central1" } else { "" }.into(),
        version: analytics.then(|| "v1.30.4".into()),
        nodes: analytics.then_some(ANALYTICS_NODES),
        problems: Counts::default(),
        nav: if analytics { nav } else { BTreeMap::new() },
        metrics: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analytics() -> NewCluster {
        NewCluster {
            context: ContextRef {
                file: PathBuf::from(DEFAULT_FILE),
                context: ANALYTICS.to_owned(),
            },
            name: "analytics".to_owned(),
        }
    }

    #[test]
    fn picker_groups_design_contexts_by_file() {
        let files = AddClusters::default().files();
        let labels: Vec<&str> = files.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, [DEFAULT_FILE, SANDBOX_FILE]);
        let selected: Vec<bool> = files
            .iter()
            .flat_map(|f| &f.contexts)
            .map(|c| c.selected)
            .collect();
        assert_eq!(selected, [true, false, true]);
        let meta = files
            .first()
            .and_then(|f| f.contexts.first())
            .map(ContextChoice::meta);
        assert_eq!(
            meta.as_deref(),
            Some("https://34.118.[IP] · gke-gcloud-auth-plugin")
        );
    }

    #[test]
    fn scripts_match_the_design_results() {
        let (lines, verdict) = script(&analytics().context);
        assert_eq!(verdict, Verdict::Ready);
        assert_eq!(lines.len(), 5);
        assert_eq!(lines.last().map(|l| l.status), Some(CheckStatus::Warn));
        let sandbox = ContextRef {
            file: PathBuf::from(SANDBOX_FILE),
            context: SANDBOX.to_owned(),
        };
        let (lines, verdict) = script(&sandbox);
        assert_eq!(verdict, Verdict::CannotAuthenticate);
        let remedy = lines.last().and_then(|l| l.remedy.clone());
        assert_eq!(remedy.as_deref(), Some("aws sso login --profile sandbox"));
    }

    #[test]
    fn checks_stream_one_line_at_a_time_then_finish() {
        let (tx, rx) = async_channel::unbounded();
        let id = analytics().context;
        simulate(&tx, vec![id.clone()]);
        let received: Vec<Update> = (0..6).filter_map(|_| rx.recv_blocking().ok()).collect();
        assert_eq!(received.len(), 6);
        assert!(matches!(received.first(), Some(Update::Check(ctx, _)) if ctx == &id));
        assert_eq!(received.last(), Some(&Update::Checked(id, Verdict::Ready)));
    }

    #[test]
    fn added_clusters_leave_the_picker_until_removed() {
        let (tx, rx) = async_channel::unbounded();
        let mut adding = AddClusters::default();
        let mut fleet = Snapshot::default();
        let add = Command::AddClusters(vec![analytics()]);
        assert!(adding.handle(&add, &mut fleet, &tx));
        assert_eq!(fleet.clusters.len(), 1);
        assert!(matches!(rx.recv_blocking(), Ok(Update::Snapshot(s)) if s.clusters.len() == 1));
        let remaining: usize = adding.files().iter().map(|f| f.contexts.len()).sum();
        assert_eq!(remaining, 2);
        let remove = Command::RemoveCluster(ClusterId::new("analytics"));
        assert!(adding.handle(&remove, &mut fleet, &tx));
        assert_eq!(fleet.clusters.len(), 0);
        let remaining: usize = adding.files().iter().map(|f| f.contexts.len()).sum();
        assert_eq!(remaining, 3);
        assert!(!adding.handle(&Command::ToggleClustersPanel, &mut fleet, &tx));
    }
}
