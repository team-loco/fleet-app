use std::time::Duration;

use crate::Status;

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
    pub pods: u32,
    pub pod_capacity: u32,
    pub age: Duration,
    pub conditions: Vec<NodeCondition>,
    pub taints: Vec<String>,
    pub labels: Vec<String>,
}

impl Node {
    pub fn is_ready(&self) -> bool {
        self.conditions
            .iter()
            .any(|c| c.kind == READY_CONDITION && c.value == CONDITION_TRUE)
    }
}

pub const READY_CONDITION: &str = "Ready";
pub const CONDITION_TRUE: &str = "True";

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
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn node(name: &str, instance: &str, schedulable: bool, ready: bool) -> Node {
        let value = if ready { CONDITION_TRUE } else { "False" };
        Node {
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
