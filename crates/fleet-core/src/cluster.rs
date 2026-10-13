use std::collections::BTreeMap;
use std::time::Duration;

use crate::events::{self, Event, recent_warnings};
use crate::nodes::{Node, NodeSummary};
use crate::overview::{Capacity, Overview, WorkloadHealth, join_clauses, named, plural, verb};
use crate::resources::{Resource, ResourceKind};
use crate::{Counts, Page, Status};

pub const CPU_UNIT: &str = "cores";
pub const MEMORY_UNIT: &str = "GiB";
pub const ACTIVE_PHASE: &str = "Active";
const GROUP_SEPARATOR: &str = "; ";
const NO_NAMESPACE_PROBLEMS: &str = "No namespace has problems.";

#[derive(Clone, Debug, PartialEq)]
pub struct Namespace {
    pub name: String,
    pub status: Status,
    pub phase: String,
    pub pods: u32,
    pub problems: Counts,
    pub cpu: f32,
    pub memory: f32,
    pub age: Duration,
}

impl Namespace {
    pub fn phase_note(&self) -> Option<&str> {
        (!self.phase.is_empty() && self.phase != ACTIVE_PHASE).then_some(self.phase.as_str())
    }
}

pub fn namespace_sentence(namespaces: &[Namespace]) -> String {
    let with = |pick: fn(&Counts) -> bool| -> Vec<&str> {
        namespaces
            .iter()
            .filter(|n| pick(&n.problems))
            .map(|n| n.name.as_str())
            .collect()
    };
    let errors = with(|c| c.err > 0);
    let warnings = with(|c| c.err == 0 && c.warn > 0);
    let suspended = with(|c| c.err == 0 && c.warn == 0 && c.sus > 0);
    let mut clauses = Vec::new();
    for (names, what) in [
        (&errors, "errors"),
        (&warnings, "warnings"),
        (&suspended, "suspended objects"),
    ] {
        if !names.is_empty() {
            clauses.push(format!(
                "{} {} {what}",
                named(names),
                verb(names.len(), "has", "have")
            ));
        }
    }
    if clauses.is_empty() {
        return NO_NAMESPACE_PROBLEMS.into();
    }
    format!("{}.", clauses.join(GROUP_SEPARATOR))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClusterDetail {
    pub overview: Overview,
    pub nodes: Vec<Node>,
    pub namespaces: Vec<Namespace>,
    pub events: Vec<Event>,
    pub resources: BTreeMap<ResourceKind, Vec<Resource>>,
}

impl ClusterDetail {
    pub fn node_summary(&self) -> NodeSummary {
        NodeSummary::of(&self.nodes)
    }

    pub fn cpu(&self) -> Capacity {
        Capacity {
            used: self.nodes.iter().map(|n| n.cpu.used).sum(),
            total: self.nodes.iter().map(|n| n.cpu.allocatable).sum(),
            unit: CPU_UNIT.into(),
            requests: self.overview.cpu.requests,
            limits: self.overview.cpu.limits,
        }
    }

    pub fn memory(&self) -> Capacity {
        Capacity {
            used: self.nodes.iter().map(|n| n.memory.used).sum(),
            total: self.nodes.iter().map(|n| n.memory.allocatable).sum(),
            unit: MEMORY_UNIT.into(),
            requests: self.overview.memory.requests,
            limits: self.overview.memory.limits,
        }
    }

    pub fn resources(&self, kind: ResourceKind) -> &[Resource] {
        self.resources.get(&kind).map_or(&[], Vec::as_slice)
    }

    pub fn workload_health(&self) -> Vec<WorkloadHealth> {
        ResourceKind::ALL
            .into_iter()
            .filter(|kind| kind.is_workload())
            .map(|kind| WorkloadHealth::of(kind, self.resources(kind)))
            .collect()
    }

    pub fn nav(&self) -> Vec<(Page, Counts)> {
        let resources = self
            .resources
            .iter()
            .map(|(kind, rows)| (kind.page(), Counts::of(rows.iter().map(|r| r.status))));
        std::iter::once((Page::Events, events::counts(&self.events)))
            .chain(resources)
            .collect()
    }

    pub fn warnings(&self) -> usize {
        self.events.iter().filter(|e| e.is_warning()).count()
    }

    pub fn recent_warnings(&self, limit: usize) -> Vec<Event> {
        recent_warnings(&self.events, limit)
    }

    pub fn sentence(&self) -> String {
        let pods = self.overview.pods;
        let nodes = self.node_summary();
        let mut clauses = Vec::new();
        if pods.failing > 0 {
            clauses.push(format!(
                "{} failing",
                plural(pods.failing as usize, "pod is", "pods are")
            ));
        }
        if pods.pending > 0 {
            clauses.push(format!(
                "{} pending",
                plural(pods.pending as usize, "is", "are")
            ));
        }
        if nodes.not_ready > 0 {
            clauses.push(format!(
                "{} not ready",
                plural(nodes.not_ready, "node is", "nodes are")
            ));
        }
        if nodes.cordoned > 0 {
            clauses.push(format!(
                "{} cordoned",
                plural(nodes.cordoned, "node is", "nodes are")
            ));
        }
        if clauses.is_empty() {
            return format!(
                "All {} are ready and every pod is running.",
                plural(nodes.total, "node", "nodes")
            );
        }
        format!("{}.", join_clauses(&clauses))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::tests::node;
    use crate::overview::PodPhases;
    use crate::resources::tests::resource;

    fn detail(pods: PodPhases, nodes: Vec<Node>) -> ClusterDetail {
        ClusterDetail {
            overview: Overview {
                pods,
                ..Overview::default()
            },
            nodes,
            ..ClusterDetail::default()
        }
    }

    #[test]
    fn sentence_lists_what_is_wrong_first() {
        let pods = PodPhases {
            running: 398,
            pending: 9,
            failing: 5,
        };
        let nodes = vec![node("a", "m6i", true, true), node("b", "m6i", false, true)];
        assert_eq!(
            detail(pods, nodes).sentence(),
            "5 pods are failing, 9 are pending and 1 node is cordoned."
        );
    }

    #[test]
    fn sentence_for_a_healthy_cluster() {
        let pods = PodPhases {
            running: 3,
            pending: 0,
            failing: 0,
        };
        let nodes = vec![node("a", "m6i", true, true), node("b", "m6i", true, true)];
        assert_eq!(
            detail(pods, nodes).sentence(),
            "All 2 nodes are ready and every pod is running."
        );
    }

    #[test]
    fn sentence_with_one_failing_pod_and_unready_node() {
        let pods = PodPhases {
            running: 3,
            pending: 0,
            failing: 1,
        };
        let nodes = vec![node("a", "m6i", true, false)];
        assert_eq!(
            detail(pods, nodes).sentence(),
            "1 pod is failing and 1 node is not ready."
        );
    }

    fn namespace(name: &str, problems: Counts) -> Namespace {
        Namespace {
            name: name.into(),
            status: problems.status(),
            phase: ACTIVE_PHASE.into(),
            pods: 0,
            problems,
            cpu: 0.,
            memory: 0.,
            age: Duration::ZERO,
        }
    }

    #[test]
    fn namespace_sentence_groups_by_worst_problem() {
        let namespaces = [
            namespace("payments", Counts::default().with_err(2).with_warn(1)),
            namespace("ml", Counts::default().with_warn(1)),
            namespace("search", Counts::default().with_warn(1)),
            namespace("web", Counts::default()),
            namespace("batch", Counts::default().with_sus(1)),
        ];
        assert_eq!(
            namespace_sentence(&namespaces),
            "payments has errors; ml and search have warnings; batch has suspended objects."
        );
        assert_eq!(
            namespace_sentence(&namespaces[3..4]),
            "No namespace has problems."
        );
    }

    #[test]
    fn phase_note_hides_active() {
        let mut ns = namespace("old", Counts::default());
        assert_eq!(ns.phase_note(), None);
        ns.phase = "Terminating".into();
        assert_eq!(ns.phase_note(), Some("Terminating"));
    }

    #[test]
    fn nav_counts_events_and_every_resource_kind() {
        let mut detail = ClusterDetail::default();
        detail.resources.insert(
            ResourceKind::Deployment,
            vec![
                resource("a", "x", Status::Err),
                resource("a", "y", Status::Warn),
                resource("a", "z", Status::Ok),
            ],
        );
        let nav = detail.nav();
        assert_eq!(
            nav,
            [
                (Page::Events, Counts::default()),
                (Page::Deployments, Counts::total(3).with_err(1).with_warn(1)),
            ]
        );
    }

    #[test]
    fn capacity_sums_node_usage() {
        let mut a = node("a", "m6i", true, true);
        a.cpu = crate::nodes::Usage {
            used: 1.5,
            allocatable: 8.,
        };
        let mut b = node("b", "m6i", true, true);
        b.cpu = crate::nodes::Usage {
            used: 2.5,
            allocatable: 8.,
        };
        let detail = detail(PodPhases::default(), vec![a, b]);
        assert_eq!(detail.cpu().amount(), "4 / 16 cores");
        assert_eq!(detail.memory().amount(), "0 / 0 GiB");
    }

    #[test]
    fn workload_health_covers_workload_kinds_in_nav_order() {
        let mut detail = ClusterDetail::default();
        detail.resources.insert(
            ResourceKind::Deployment,
            vec![resource("a", "x", Status::Ok)],
        );
        detail
            .resources
            .insert(ResourceKind::Service, vec![resource("a", "s", Status::Ok)]);
        let health = detail.workload_health();
        let pages: Vec<_> = health.iter().map(|h| h.page.label()).collect();
        assert_eq!(
            pages,
            [
                "Deployments",
                "StatefulSets",
                "DaemonSets",
                "Jobs",
                "CronJobs"
            ]
        );
        assert_eq!(health[0].summary, "1 / 1 ready");
        assert_eq!(health[1].summary, "0 / 0 ready");
    }
}
