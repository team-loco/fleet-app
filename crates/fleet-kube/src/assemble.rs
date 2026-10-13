use fleet_core::home::{ClusterMetrics, Problem, problem_counts};
use fleet_core::overview::{Overview, Utilization};
use fleet_core::{ClusterDetail, ClusterId, ClusterSummary, Counts, Page, ResourceRef};
use fleet_flux::ObjectKind;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::{Event, Namespace, Node, Pod};

use crate::detail::{self, PodIndex, capacity};
use crate::flux::{self, FluxObject, custom_kind, flux_object};
use crate::lists::{nav_counts, watched_lists};
use crate::manager::{ClusterHandle, ConnectionStatus};
use crate::metrics::{NodeUsage, PodUsage};
use crate::probe::PrometheusTarget;
use crate::problems::{connection_problem, node_problems, pod_problems};
use crate::snapshot::{pod_counts, summarize};
use crate::usage::{UsageRing, sample};

const PERCENT: f32 = 100.;
const METRICS_SERVER: &str = "metrics-server";
const NO_METRICS: &str = "unavailable";

#[derive(Clone, Debug, PartialEq)]
pub struct Prometheus {
    pub target: PrometheusTarget,
    pub utilization: Vec<Utilization>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extras {
    pub flux: Vec<ObjectKind>,
    pub node_usage: Option<Vec<NodeUsage>>,
    pub pod_usage: Vec<PodUsage>,
    pub ring: UsageRing,
    pub prometheus: Option<Prometheus>,
}

impl Extras {
    pub fn new(samples: usize) -> Self {
        Self {
            flux: Vec::new(),
            node_usage: None,
            pod_usage: Vec::new(),
            ring: UsageRing::new(samples),
            prometheus: None,
        }
    }

    pub fn record(&mut self, nodes: &[std::sync::Arc<Node>], usage: Option<Vec<NodeUsage>>) {
        if let Some(sample) = usage.as_deref().and_then(|usage| sample(nodes, usage)) {
            self.ring.push(sample);
        }
        self.node_usage = usage;
    }
}

pub struct Selection<'a> {
    pub event_limit: usize,
    pub yaml: Option<&'a (ResourceRef, String)>,
}

pub struct Built {
    pub summary: ClusterSummary,
    pub pods: Vec<fleet_core::pods::Pod>,
    pub problems: Vec<Problem>,
    pub flux: Counts,
    pub detail: Option<ClusterDetail>,
}

fn flux_objects(handle: &ClusterHandle, kinds: &[ObjectKind], now: Timestamp) -> Vec<FluxObject> {
    kinds
        .iter()
        .flat_map(|kind| {
            handle
                .custom_objects(&custom_kind(*kind))
                .into_iter()
                .filter_map(move |object| flux_object(&object, *kind, now))
        })
        .collect()
}

fn reachable(status: &ConnectionStatus) -> bool {
    matches!(
        status,
        ConnectionStatus::Connected | ConnectionStatus::Degraded(_)
    )
}

pub fn cluster(
    id: &ClusterId,
    handle: &ClusterHandle,
    extras: &Extras,
    selection: Option<&Selection<'_>>,
    now: Timestamp,
) -> Built {
    let connection = handle.status();
    let pods = handle.objects::<Pod>();
    let nodes = handle.objects::<Node>();
    let objects = flux_objects(handle, &extras.flux, now);
    let mut problems = pod_problems(id, &pods, now);
    problems.extend(node_problems(id, &nodes, now));
    problems.extend(flux::problems(id, &objects));
    problems.extend(connection_problem(id, &connection));
    let flux_counts = if extras.flux.is_empty() {
        Counts::default()
    } else {
        flux::counts(&objects)
    };
    let percents = extras.ring.latest().map(|s| s.percents());
    let metrics = reachable(&connection).then(|| ClusterMetrics {
        pods: pod_counts(&pods),
        cpu_percent: percents.map(|(cpu, _)| cpu),
        memory_percent: percents.map(|(_, memory)| memory),
        flux: flux_counts,
        revision: flux::revision(&objects),
    });
    let counts = problem_counts(&problems);
    let mut summary = summarize(id.clone(), id.to_string(), handle, counts, metrics);
    let pod_rows = if selection.is_some() {
        crate::pods::pods(
            id,
            &pods,
            &handle.objects::<Event>(),
            &extras.pod_usage,
            now,
        )
    } else {
        Vec::new()
    };
    if selection.is_some() {
        summary
            .nav
            .insert(Page::Pods, crate::pods::counts(&pod_rows));
    }
    let detail = selection.map(|selection| {
        let detail = cluster_detail(handle, extras, &pods, &nodes, selection, now);
        for (kind, rows) in &detail.resources {
            summary.nav.insert(kind.page(), nav_counts(rows));
        }
        let warnings = u32::try_from(detail.warnings()).unwrap_or(u32::MAX);
        summary
            .nav
            .insert(Page::Events, Counts::default().with_warn(warnings));
        detail
    });
    Built {
        summary,
        pods: pod_rows,
        problems,
        flux: Counts {
            total: None,
            ..flux_counts
        },
        detail,
    }
}

fn cluster_detail(
    handle: &ClusterHandle,
    extras: &Extras,
    pods: &[std::sync::Arc<Pod>],
    nodes: &[std::sync::Arc<Node>],
    selection: &Selection<'_>,
    now: Timestamp,
) -> ClusterDetail {
    let index = PodIndex::build(pods, now);
    let (cpu, memory) = index.commitments(capacity(nodes));
    let usage = extras.node_usage.as_deref().unwrap_or_default();
    let (metrics_source, utilization) = match &extras.prometheus {
        Some(prometheus) if !prometheus.utilization.is_empty() => {
            let ranges = prometheus
                .utilization
                .iter()
                .map(|range| Utilization {
                    cpu_requests: cpu.requests * PERCENT,
                    ..range.clone()
                })
                .collect();
            (prometheus.target.label(), ranges)
        }
        _ => {
            let hour = extras.ring.utilization(cpu.requests * PERCENT);
            let source = if extras.node_usage.is_some() {
                METRICS_SERVER
            } else {
                NO_METRICS
            };
            (source.to_owned(), hour.into_iter().collect())
        }
    };
    let mut resources = watched_lists(handle, &index, now);
    if let Some((target, yaml)) = selection.yaml {
        let row = resources
            .iter_mut()
            .filter(|(kind, _)| kind.name() == target.kind)
            .flat_map(|(_, rows)| rows.iter_mut())
            .find(|row| {
                Some(&row.namespace) == target.namespace.as_ref() && row.name == target.name
            });
        if let Some(row) = row {
            row.yaml.clone_from(yaml);
        }
    }
    ClusterDetail {
        overview: Overview {
            metrics_source,
            cpu,
            memory,
            pods: index.phases,
            utilization,
        },
        nodes: detail::nodes(nodes, &index, usage, now),
        namespaces: detail::namespaces(
            &handle.objects::<Namespace>(),
            &index,
            &extras.pod_usage,
            now,
        ),
        events: detail::events(&handle.objects::<Event>(), selection.event_limit, now),
        resources,
    }
}
