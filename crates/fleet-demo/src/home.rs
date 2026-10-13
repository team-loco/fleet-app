use std::time::Duration;

use fleet_core::home::{ClusterMetrics, Problem, Revision};
use fleet_core::{ClusterId, Counts, FleetHome, ResourceRef, Status};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const HEAD: &str = "a41e07c";
const HELD_AT: &str = "7c2d9e0";
const ADDED_CPU: u8 = 27;
const ADDED_MEMORY: u8 = 38;
const ADDED_KUSTOMIZATIONS: u32 = 8;

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

pub fn added_metrics(pods: u32) -> ClusterMetrics {
    ClusterMetrics {
        pods: Counts::total(pods),
        cpu_percent: Some(ADDED_CPU),
        memory_percent: Some(ADDED_MEMORY),
        flux: Counts::total(ADDED_KUSTOMIZATIONS),
        revision: Revision {
            sha: HEAD.into(),
            held: false,
        },
    }
}

pub fn fleet_home() -> FleetHome {
    FleetHome {
        problems: problems(),
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
