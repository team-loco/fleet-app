use std::time::Duration;

use crate::filter::TextFilter;
use crate::overview::{Commitment, join_clauses, named, plural, verb};
use crate::{Status, Uid};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub used: f32,
    pub allocatable: f32,
}

impl Usage {
    pub fn fraction(&self) -> f32 {
        if self.allocatable > 0. {
            self.used / self.allocatable
        } else {
            0.
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeCondition {
    pub kind: String,
    pub value: String,
    pub healthy: bool,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub name: String,
    pub uid: Uid,
    pub status: Status,
    pub state: String,
    pub schedulable: bool,
    pub instance_type: String,
    pub zone: String,
    pub version: String,
    pub internal_ip: String,
    pub os_image: String,
    pub runtime: String,
    pub cpu: Usage,
    pub memory: Usage,
    pub cpu_commitment: Commitment,
    pub memory_commitment: Commitment,
    pub pods: u32,
    pub pod_capacity: u32,
    pub age: Duration,
    pub conditions: Vec<NodeCondition>,
    pub taints: Vec<String>,
    pub labels: Vec<String>,
}

impl Node {
    pub fn matches(&self, query: &TextFilter) -> bool {
        let mut fields = vec![
            self.name.as_str(),
            self.state.as_str(),
            self.instance_type.as_str(),
            self.zone.as_str(),
            self.version.as_str(),
            self.internal_ip.as_str(),
        ];
        fields.extend(self.labels.iter().map(String::as_str));
        query.matches(None, &fields)
    }

    pub fn is_ready(&self) -> bool {
        self.conditions
            .iter()
            .any(|c| c.kind == READY_CONDITION && c.value == CONDITION_TRUE)
    }
}

pub const READY_CONDITION: &str = "Ready";
pub const CONDITION_TRUE: &str = "True";
const INSTANCE_SIZE_SEPARATOR: char = '.';

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeSummary {
    pub total: usize,
    pub ready: usize,
    pub cordoned: usize,
    pub not_ready: usize,
    pub instance_types: Vec<(String, usize)>,
    pub pod_capacity: u32,
}

impl NodeSummary {
    pub fn of(nodes: &[Node]) -> Self {
        let mut instance_types: Vec<(String, usize)> = Vec::new();
        for node in nodes {
            match instance_types
                .iter_mut()
                .find(|(name, _)| *name == node.instance_type)
            {
                Some((_, count)) => *count += 1,
                None => instance_types.push((node.instance_type.clone(), 1)),
            }
        }
        let cordoned = nodes.iter().filter(|n| !n.schedulable).count();
        let not_ready = nodes.iter().filter(|n| !n.is_ready()).count();
        let ready = nodes
            .iter()
            .filter(|n| n.schedulable && n.is_ready())
            .count();
        Self {
            total: nodes.len(),
            ready,
            cordoned,
            not_ready,
            instance_types,
            pod_capacity: nodes.iter().map(|n| n.pod_capacity).sum(),
        }
    }

    pub fn instance_label(&self) -> String {
        let parts: Vec<String> = self
            .instance_types
            .iter()
            .map(|(name, count)| format!("{name} ×{count}"))
            .collect();
        parts.join(" · ")
    }

    pub fn compact_instance_label(&self) -> String {
        let mut types = self.instance_types.clone();
        types.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        let parts: Vec<String> = types
            .iter()
            .enumerate()
            .map(|(ix, (name, count))| {
                let shown = if ix == 0 {
                    name.as_str()
                } else {
                    name.split(INSTANCE_SIZE_SEPARATOR).next().unwrap_or(name)
                };
                format!("{shown} ×{count}")
            })
            .collect();
        parts.join(" · ")
    }
}

pub fn sentence(nodes: &[Node]) -> String {
    if nodes.is_empty() {
        return "No nodes have registered with this cluster.".into();
    }
    let not_ready: Vec<&str> = nodes
        .iter()
        .filter(|n| !n.is_ready())
        .map(|n| n.name.as_str())
        .collect();
    let cordoned: Vec<&str> = nodes
        .iter()
        .filter(|n| !n.schedulable && n.is_ready())
        .map(|n| n.name.as_str())
        .collect();
    let pressured: Vec<String> = nodes
        .iter()
        .filter(|n| n.is_ready())
        .filter_map(|n| {
            let kinds: Vec<String> = n
                .conditions
                .iter()
                .filter(|c| !c.healthy && c.kind != READY_CONDITION)
                .map(|c| c.kind.clone())
                .collect();
            (!kinds.is_empty()).then(|| format!("{} reports {}", n.name, join_clauses(&kinds)))
        })
        .collect();
    let mut clauses = Vec::new();
    if !not_ready.is_empty() {
        clauses.push(format!(
            "{} {} not ready",
            named(&not_ready),
            verb(not_ready.len(), "is", "are")
        ));
    }
    clauses.extend(pressured);
    if !cordoned.is_empty() {
        let only = clauses.is_empty();
        let mut clause = format!(
            "{} {} cordoned",
            named(&cordoned),
            verb(cordoned.len(), "is", "are")
        );
        if only {
            clause.push_str(", so new pods are not scheduled on ");
            clause.push_str(verb(cordoned.len(), "it", "them"));
        }
        clauses.push(clause);
    }
    if clauses.is_empty() {
        return format!(
            "All {} are ready and schedulable.",
            plural(nodes.len(), "node", "nodes")
        );
    }
    format!("{}.", join_clauses(&clauses))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn node(name: &str, instance: &str, schedulable: bool, ready: bool) -> Node {
        let value = if ready { CONDITION_TRUE } else { "False" };
        Node {
            uid: Uid::default(),
            name: name.into(),
            status: Status::Ok,
            state: String::new(),
            schedulable,
            instance_type: instance.into(),
            zone: String::new(),
            version: String::new(),
            internal_ip: String::new(),
            os_image: String::new(),
            runtime: String::new(),
            cpu: Usage::default(),
            memory: Usage::default(),
            cpu_commitment: Commitment::default(),
            memory_commitment: Commitment::default(),
            pods: 0,
            pod_capacity: 110,
            age: Duration::ZERO,
            conditions: vec![NodeCondition {
                kind: READY_CONDITION.into(),
                value: value.into(),
                healthy: ready,
                message: String::new(),
            }],
            taints: Vec::new(),
            labels: Vec::new(),
        }
    }

    #[test]
    fn node_filter_matches_name_instance_and_zone() {
        let n = node("ip-10-4-31-12", "m6i.xlarge", true, true);
        assert!(n.matches(&TextFilter::parse("31-12")));
        assert!(n.matches(&TextFilter::parse("M6I")));
        assert!(!n.matches(&TextFilter::parse("c7g")));
        assert!(!n.matches(&TextFilter::parse("ns:default")));
    }

    #[test]
    fn summary_counts_ready_cordoned_and_groups_instance_types() {
        let nodes = [
            node("a", "m6i.4xlarge", true, true),
            node("b", "m6i.4xlarge", false, true),
            node("c", "g5.2xlarge", true, true),
            node("d", "m6i.4xlarge", true, false),
        ];
        let summary = NodeSummary::of(&nodes);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.ready, 2);
        assert_eq!(summary.cordoned, 1);
        assert_eq!(summary.not_ready, 1);
        assert_eq!(summary.pod_capacity, 440);
        assert_eq!(summary.instance_label(), "m6i.4xlarge ×3 · g5.2xlarge ×1");
        assert_eq!(summary.compact_instance_label(), "m6i.4xlarge ×3 · g5 ×1");
    }

    #[test]
    fn sentence_names_the_cordoned_node() {
        let nodes = [
            node("ip-10-4-31-12", "m6i", false, true),
            node("ip-10-4-22-63", "m6i", true, true),
        ];
        assert_eq!(
            sentence(&nodes),
            "ip-10-4-31-12 is cordoned, so new pods are not scheduled on it."
        );
    }

    #[test]
    fn sentence_puts_not_ready_and_pressure_before_cordoned() {
        let mut pressured = node("c", "m6i", true, true);
        pressured.conditions.push(NodeCondition {
            kind: "DiskPressure".into(),
            value: CONDITION_TRUE.into(),
            healthy: false,
            message: String::new(),
        });
        let nodes = [
            node("a", "m6i", true, false),
            node("b", "m6i", false, false),
            pressured,
            node("d", "m6i", false, true),
        ];
        assert_eq!(
            sentence(&nodes),
            "a and b are not ready, c reports DiskPressure and d is cordoned."
        );
    }

    #[test]
    fn sentence_for_healthy_and_empty_clusters() {
        let nodes = [node("a", "m6i", true, true), node("b", "m6i", true, true)];
        assert_eq!(sentence(&nodes), "All 2 nodes are ready and schedulable.");
        assert_eq!(sentence(&[]), "No nodes have registered with this cluster.");
    }

    #[test]
    fn usage_fraction_guards_zero_allocatable() {
        assert!(
            (Usage {
                used: 2.,
                allocatable: 8.
            }
            .fraction()
                - 0.25)
                .abs()
                < f32::EPSILON
        );
        assert!(Usage::default().fraction().abs() < f32::EPSILON);
    }
}
