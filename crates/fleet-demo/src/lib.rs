mod add_cluster;
mod changes;
mod detail;
mod gitops;
mod home;
mod lists;
mod resources;
mod simulate;

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::thread;
#[cfg(feature = "loco")]
use std::time::Duration;

use fleet_core::{
    Backend, ClusterId, ClusterSummary, Command, Counts, Page, Receiver, Sender, Snapshot, Status,
    Update,
};

const PROD_US_EAST: &str = "prod-us-east";
#[cfg(feature = "loco")]
const AGENT_SYNCED_SECS: u64 = 4;

pub struct DemoBackend;

impl Backend for DemoBackend {
    fn start(self: Box<Self>, updates: Sender<Update>, commands: Receiver<Command>) {
        let spawned = thread::Builder::new()
            .name("fleet-demo".into())
            .spawn(move || run(&updates, &commands));
        if let Err(error) = spawned {
            tracing::error!(%error, "demo backend thread failed to start");
        }
    }
}

enum Input {
    Command(Command),
    Settle(simulate::Settle),
}

fn run(updates: &Sender<Update>, commands: &Receiver<Command>) {
    let mut state = snapshot();
    if updates
        .send_blocking(Update::Snapshot(Box::new(state.clone())))
        .is_err()
    {
        return;
    }
    let mut adding = add_cluster::AddClusters::default();
    let (input_tx, input_rx) = mpsc::channel();
    let forward = input_tx.clone();
    let commands = commands.clone();
    let forwarder = thread::Builder::new()
        .name("fleet-demo-commands".into())
        .spawn(move || {
            while let Ok(command) = commands.recv_blocking() {
                if forward.send(Input::Command(command)).is_err() {
                    break;
                }
            }
        });
    if let Err(error) = forwarder {
        tracing::error!(%error, "demo command forwarder failed to start");
        return;
    }
    while let Ok(input) = input_rx.recv() {
        let changed = match input {
            Input::Command(command) => {
                if adding.handle(&command, &mut state, updates) {
                    continue;
                }
                tracing::debug!(?command, "demo backend received command");
                let pending = simulate::apply(&mut state, &command);
                let changed = pending.is_some();
                if let Some(pending) = pending {
                    schedule(&input_tx, pending);
                }
                changed
            }
            Input::Settle(pending) => {
                simulate::settle(&mut state, &pending);
                true
            }
        };
        if changed
            && updates
                .send_blocking(Update::Snapshot(Box::new(state.clone())))
                .is_err()
        {
            return;
        }
    }
}

fn schedule(input: &mpsc::Sender<Input>, pending: simulate::Settle) {
    let input = input.clone();
    let timer = thread::Builder::new()
        .name("fleet-demo-settle".into())
        .spawn(move || {
            thread::sleep(simulate::SETTLE);
            let _ = input.send(Input::Settle(pending));
        });
    if let Err(error) = timer {
        tracing::error!(%error, "demo settle timer failed to start");
    }
}

pub fn snapshot() -> Snapshot {
    let fleet_nav = BTreeMap::from([
        (Page::Problems, Counts::default().with_err(3).with_warn(6)),
        (Page::GitOps, Counts::default().with_err(1).with_sus(1)),
    ]);
    Snapshot {
        clusters: vec![
            prod_us_east(),
            cluster(ClusterSeed {
                name: "prod-eu-west",
                status: Status::Ok,
                state: "Healthy",
                distro: "EKS",
                region: "eu-west-1",
                version: Some("v1.31.2"),
                nodes: Some(11),
                problems: Counts::default().with_sus(1).with_warn(1),
            }),
            cluster(ClusterSeed {
                name: "staging",
                status: Status::Warn,
                state: "Warnings",
                distro: "EKS",
                region: "us-east-2",
                version: Some("v1.30.5"),
                nodes: Some(6),
                problems: Counts::default().with_warn(2),
            }),
            cluster(ClusterSeed {
                name: "dev-kind",
                status: Status::Ok,
                state: "Healthy",
                distro: "kind",
                region: "local",
                version: Some("v1.31.0"),
                nodes: Some(3),
                problems: Counts::default(),
            }),
            cluster(ClusterSeed {
                name: "qa-gke",
                status: Status::Err,
                state: "Auth expired",
                distro: "GKE",
                region: "us-central1",
                version: None,
                nodes: None,
                problems: Counts::default().with_err(1),
            }),
            cluster(ClusterSeed {
                name: "edge-k3s",
                status: Status::Offline,
                state: "Offline 3h",
                distro: "k3s",
                region: "edge",
                version: Some("v1.29.8"),
                nodes: None,
                problems: Counts::default(),
            }),
        ],
        fleet_nav,
        home: home::fleet_home(),
        resources: resources::resources(),
        changes: changes::log(),
        details: BTreeMap::from([(ClusterId::new(PROD_US_EAST), detail::prod_us_east())]),
        gitops: gitops::gitops(),
    }
}

#[derive(Clone, Copy)]
struct ClusterSeed {
    name: &'static str,
    status: Status,
    state: &'static str,
    distro: &'static str,
    region: &'static str,
    version: Option<&'static str>,
    nodes: Option<u32>,
    problems: Counts,
}

fn cluster(seed: ClusterSeed) -> ClusterSummary {
    let nav = seed
        .nodes
        .map(|n| BTreeMap::from([(Page::Nodes, Counts::total(n))]))
        .unwrap_or_default();
    ClusterSummary {
        id: ClusterId::new(seed.name),
        name: seed.name.into(),
        status: seed.status,
        state: seed.state.into(),
        distro: seed.distro.into(),
        region: seed.region.into(),
        version: seed.version.map(Into::into),
        nodes: seed.nodes,
        problems: seed.problems,
        nav,
        metrics: home::metrics(seed.name),
        #[cfg(feature = "loco")]
        loco_agent_synced: None,
    }
}

fn prod_us_east() -> ClusterSummary {
    let mut summary = cluster(ClusterSeed {
        name: PROD_US_EAST,
        status: Status::Err,
        state: "Degraded",
        distro: "EKS",
        region: "us-east-1",
        version: Some("v1.31.2"),
        nodes: Some(14),
        problems: Counts::default().with_err(2).with_warn(3),
    });
    summary.nav = BTreeMap::from([
        (Page::Nodes, Counts::total(14).with_warn(1)),
        (Page::Namespaces, Counts::total(23)),
        (Page::Events, Counts::default().with_warn(7)),
        #[cfg(feature = "loco")]
        (Page::LocoPlatform, Counts::default().with_warn(1)),
        #[cfg(feature = "loco")]
        (Page::LocoApplications, Counts::total(214).with_err(1)),
        #[cfg(feature = "loco")]
        (Page::LocoBuilds, Counts::default().with_err(1)),
        #[cfg(feature = "loco")]
        (Page::LocoRoutes, Counts::total(44).with_warn(2)),
        (Page::Pods, Counts::total(412).with_err(5)),
        (Page::Deployments, Counts::total(48).with_err(1)),
        (Page::StatefulSets, Counts::total(12)),
        (Page::DaemonSets, Counts::total(9)),
        (Page::Jobs, Counts::total(31)),
        (Page::CronJobs, Counts::total(14)),
        (Page::Kustomizations, Counts::total(12).with_err(1)),
        (Page::HelmReleases, Counts::total(9)),
        (Page::Sources, Counts::total(4)),
        (Page::Services, Counts::total(64)),
        (Page::Ingresses, Counts::total(11)),
        (Page::ConfigMaps, Counts::total(88)),
        (Page::Secrets, Counts::total(71)),
        (Page::Pvcs, Counts::total(19)),
    ]);
    #[cfg(feature = "loco")]
    {
        summary.loco_agent_synced = Some(Duration::from_secs(AGENT_SYNCED_SECS));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_matches_design_rollups() {
        let snapshot = snapshot();
        assert_eq!(snapshot.clusters.len(), 6);
        assert_eq!(snapshot.connected(), 5);
        let problems = snapshot.problems();
        assert_eq!((problems.err, problems.warn, problems.sus), (3, 6, 1));
    }

    #[test]
    fn problem_list_agrees_with_cluster_rollups() {
        let snapshot = snapshot();
        for cluster in &snapshot.clusters {
            let listed = snapshot
                .home
                .problems
                .iter()
                .filter(|p| p.target.cluster == cluster.id);
            let counts = fleet_core::home::problem_counts(listed);
            assert_eq!(
                (counts.err, counts.warn, counts.sus),
                (
                    cluster.problems.err,
                    cluster.problems.warn,
                    cluster.problems.sus
                ),
                "{}",
                cluster.name
            );
        }
    }

    #[test]
    fn status_bar_flux_total_matches_design() {
        let flux = fleet_core::gitops::flux_ready(&snapshot().clusters);
        assert_eq!((flux.ready, flux.total), (72, 75));
    }

    #[test]
    fn unreachable_clusters_have_no_metrics() {
        let snapshot = snapshot();
        let without: Vec<&str> = snapshot
            .clusters
            .iter()
            .filter(|c| c.metrics.is_none())
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(without, ["qa-gke", "edge-k3s"]);
    }

    #[test]
    fn backend_sends_snapshot_then_drains_commands() {
        let (update_tx, update_rx) = async_channel::unbounded();
        let (command_tx, command_rx) = async_channel::unbounded();
        Box::new(DemoBackend).start(update_tx, command_rx);
        let first = update_rx.recv_blocking();
        assert_eq!(first, Ok(Update::Snapshot(Box::new(snapshot()))));
        assert!(
            command_tx
                .send_blocking(Command::ToggleClustersPanel)
                .is_ok()
        );
    }
}
