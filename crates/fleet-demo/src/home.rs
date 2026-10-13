use std::collections::BTreeMap;
use std::time::Duration;

use fleet_core::home::{ClusterMetrics, Commit, GitOpsChanges, Lane, Outcome, Problem, Revision};
use fleet_core::{ClusterId, Counts, FleetHome, ResourceRef, Status};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const HEAD: &str = "a41e07c";
const HELD_AT: &str = "7c2d9e0";

struct MetricsSeed {
    pods: Counts,
    cpu: u8,
    memory: u8,
    flux: Counts,
    revision: &'static str,
    held: bool,
}

pub fn metrics(cluster: &str) -> Option<ClusterMetrics> {
    let seed = match cluster {
        "prod-us-east" => MetricsSeed {
            pods: Counts::total(412).with_err(5),
            cpu: 61,
            memory: 61,
            flux: Counts::total(21).with_err(1),
            revision: HEAD,
            held: false,
        },
        "prod-eu-west" => MetricsSeed {
            pods: Counts::total(356),
            cpu: 48,
            memory: 55,
            flux: Counts::total(21).with_sus(1),
            revision: HELD_AT,
            held: true,
        },
        "staging" => MetricsSeed {
            pods: Counts::total(188).with_warn(2),
            cpu: 35,
            memory: 40,
            flux: Counts::total(21).with_warn(1),
            revision: HEAD,
            held: false,
        },
        "dev-kind" => MetricsSeed {
            pods: Counts::total(64),
            cpu: 12,
            memory: 22,
            flux: Counts::total(12),
            revision: HEAD,
            held: false,
        },
        _ => return None,
    };
    Some(ClusterMetrics {
        pods: seed.pods,
        cpu_percent: Some(seed.cpu),
        memory_percent: Some(seed.memory),
        flux: seed.flux,
        revision: Revision {
            sha: seed.revision.into(),
            held: seed.held,
        },
    })
}

pub fn fleet_home() -> FleetHome {
    FleetHome {
        problems: problems(),
        changes: changes(),
    }
}

fn target(cluster: &str, kind: &str, object: &str) -> ResourceRef {
    let (namespace, name) = match object.split_once('/') {
        Some((namespace, name)) => (Some(namespace.into()), name.into()),
        None => (None, object.into()),
    };
    ResourceRef {
        cluster: ClusterId::new(cluster),
        kind: kind.into(),
        namespace,
        name,
    }
}

fn problem(
    cluster: &str,
    status: Status,
    kind: &str,
    object: &str,
    message: &str,
    age_secs: u64,
) -> Problem {
    Problem {
        status,
        target: target(cluster, kind, object),
        message: message.into(),
        age: Duration::from_secs(age_secs),
    }
}

fn problems() -> Vec<Problem> {
    vec![
        problem(
            "prod-us-east",
            Status::Err,
            "Kustomization",
            "flux-system/apps-payments",
            "Health check failed after 5m: Deployment/payments/checkout-api not ready",
            4 * MINUTE,
        ),
        problem(
            "prod-us-east",
            Status::Err,
            "Deployment",
            "payments/checkout-api",
            "2/3 ready · pod …x2lqp CrashLoopBackOff, 14 restarts, exit 1",
            3 * HOUR,
        ),
        problem(
            "prod-us-east",
            Status::Warn,
            "Pod",
            "ml/ml-batch-5c7f-9kq2d",
            "FailedScheduling: 0/14 nodes available, insufficient nvidia.com/gpu",
            6 * MINUTE,
        ),
        problem(
            "prod-us-east",
            Status::Warn,
            "Pod",
            "monitoring/grafana-6b8d9-tw4mn",
            "FailedMount: configmap \"dashboards\" not found",
            18 * MINUTE,
        ),
        problem(
            "prod-us-east",
            Status::Warn,
            "Node",
            "ip-10-4-31-12",
            "Cordoned (SchedulingDisabled)",
            42 * MINUTE,
        ),
        problem(
            "prod-eu-west",
            Status::Suspended,
            "Kustomization",
            "flux-system/apps-payments",
            "Suspended · 2 commits behind fleet-infra/main",
            2 * DAY,
        ),
        problem(
            "prod-eu-west",
            Status::Warn,
            "HPA",
            "web/web-frontend",
            "At max replicas (12/12) for 25m",
            25 * MINUTE,
        ),
        problem(
            "staging",
            Status::Warn,
            "HelmRelease",
            "monitoring/kube-prometheus-stack",
            "Upgrade to 62.7.0 retrying (2/3): context deadline exceeded",
            9 * MINUTE,
        ),
        problem(
            "staging",
            Status::Warn,
            "Pod",
            "search/search-indexer-0",
            "Readiness probe failed: HTTP 503",
            11 * MINUTE,
        ),
        problem(
            "qa-gke",
            Status::Err,
            "Cluster",
            "qa-gke",
            "Credentials expired (gke-gcloud-auth-plugin) · run gcloud auth login",
            DAY,
        ),
    ]
}

fn lane(cluster: &str, label: &str) -> Lane {
    Lane {
        cluster: ClusterId::new(cluster),
        label: label.into(),
    }
}

fn commit(
    sha: &str,
    message: &str,
    author: &str,
    age_secs: u64,
    outcomes: &[(&str, Outcome)],
) -> Commit {
    let outcomes = outcomes
        .iter()
        .map(|(cluster, outcome)| (ClusterId::new(*cluster), *outcome))
        .collect::<BTreeMap<_, _>>();
    Commit {
        sha: sha.into(),
        message: message.into(),
        author: author.into(),
        age: Duration::from_secs(age_secs),
        outcomes,
    }
}

fn changes() -> GitOpsChanges {
    GitOpsChanges {
        source: "fleet-infra/main".into(),
        lanes: vec![
            lane("staging", "staging"),
            lane("prod-us-east", "us-east"),
            lane("prod-eu-west", "eu-west"),
            lane("dev-kind", "dev"),
        ],
        commits: vec![
            commit(
                HEAD,
                "checkout-api: bump to 2.14.0, read webhook secret from env",
                "@mkhan",
                3 * HOUR,
                &[
                    ("staging", Outcome::Applied),
                    ("prod-us-east", Outcome::Failing),
                    ("prod-eu-west", Outcome::Held),
                    ("dev-kind", Outcome::Applied),
                ],
            ),
            commit(
                "3be91f2",
                "monitoring: kube-prometheus-stack 62.7.0 on staging",
                "@renovate[bot]",
                5 * HOUR,
                &[("staging", Outcome::Retrying)],
            ),
            commit(
                HELD_AT,
                "web-frontend: raise min replicas to 6",
                "@jlee",
                DAY,
                &[
                    ("staging", Outcome::Applied),
                    ("prod-us-east", Outcome::Applied),
                    ("prod-eu-west", Outcome::Applied),
                    ("dev-kind", Outcome::Applied),
                ],
            ),
        ],
    }
}
