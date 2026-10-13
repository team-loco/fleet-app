use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use fleet_core::cluster::Namespace;
use fleet_core::events::{Event, warnings_first};
use fleet_core::nodes::{self, NodeCondition};
use fleet_core::overview::{Commitment, PodPhases};
use fleet_core::{Counts, Status};
use jiff::Timestamp;
use k8s_openapi::api::core::v1::{self as core, Node, Pod};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

use crate::clock::{age, between};
use crate::metrics::{NodeUsage, PodUsage, Usage, cpu_millicores, parse_quantity};
use crate::problems::{OwnerKey, is_active, owner_of, pod_issue};
use crate::snapshot::{PodHealth, node_ready, pod_health, schedulable};
use crate::usage::{Allocatable, allocatable, cores, gib, narrow};

const CPU: &str = "cpu";
const MEMORY: &str = "memory";
const PODS: &str = "pods";
const PHASE_PENDING: &str = "Pending";
const PHASE_SUCCEEDED: &str = "Succeeded";
const READY_CONDITION: &str = "Ready";
const CONDITION_TRUE: &str = "True";
const CONDITION_FALSE: &str = "False";
const STATE_READY: &str = "Ready";
const STATE_NOT_READY: &str = "NotReady";
const STATE_CORDONED: &str = "SchedulingDisabled";
const INSTANCE_TYPE_LABELS: [&str; 2] = [
    "node.kubernetes.io/instance-type",
    "beta.kubernetes.io/instance-type",
];
const ZONE_LABELS: [&str; 2] = [
    "topology.kubernetes.io/zone",
    "failure-domain.beta.kubernetes.io/zone",
];
const INTERNAL_IP: &str = "InternalIP";
const WARNING: &str = "Warning";
const ERROR_REASONS: [&str; 4] = [
    "BackOff",
    "Failed",
    "ProgressDeadlineExceeded",
    "FailedCreate",
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Totals {
    cpu: f64,
    memory: f64,
}

impl Totals {
    fn add(&mut self, other: Self) {
        self.cpu += other.cpu;
        self.memory += other.memory;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct NamespaceFacts {
    pods: u32,
    problems: Counts,
}

#[derive(Debug, Default)]
pub struct PodIndex {
    pub issues: HashMap<OwnerKey, Status>,
    pub phases: PodPhases,
    namespaces: HashMap<String, NamespaceFacts>,
    per_node: HashMap<String, u32>,
    requests: Totals,
    limits: Totals,
}

fn resources(map: Option<&BTreeMap<String, Quantity>>) -> Totals {
    let get = |key: &str| map.and_then(|m| m.get(key)).map(|q| q.0.as_str());
    Totals {
        cpu: get(CPU).and_then(cpu_millicores).unwrap_or_default(),
        memory: get(MEMORY).and_then(parse_quantity).unwrap_or_default(),
    }
}

fn pod_resources(pod: &Pod) -> (Totals, Totals) {
    let mut requests = Totals::default();
    let mut limits = Totals::default();
    for container in pod.spec.iter().flat_map(|spec| spec.containers.iter()) {
        let declared = container.resources.as_ref();
        requests.add(resources(declared.and_then(|r| r.requests.as_ref())));
        limits.add(resources(declared.and_then(|r| r.limits.as_ref())));
    }
    (requests, limits)
}

impl PodIndex {
    pub fn build(pods: &[Arc<Pod>], now: Timestamp) -> Self {
        let mut index = Self::default();
        for pod in pods {
            let phase = pod.status.as_ref().and_then(|s| s.phase.as_deref());
            let health = pod_health(pod);
            match health {
                PodHealth::Failing => index.phases.failing += 1,
                PodHealth::Healthy if phase == Some(PHASE_SUCCEEDED) => {}
                PodHealth::Healthy => index.phases.running += 1,
                PodHealth::Warning if phase == Some(PHASE_PENDING) => index.phases.pending += 1,
                PodHealth::Warning => index.phases.running += 1,
            }
            if !is_active(pod) {
                continue;
            }
            let namespace = pod.metadata.namespace.as_deref().unwrap_or_default();
            if !index.namespaces.contains_key(namespace) {
                index
                    .namespaces
                    .insert(namespace.to_owned(), NamespaceFacts::default());
            }
            let Some(facts) = index.namespaces.get_mut(namespace) else {
                unreachable!()
            };
            facts.pods += 1;
            match health {
                PodHealth::Failing => facts.problems.err += 1,
                PodHealth::Warning => facts.problems.warn += 1,
                PodHealth::Healthy => {}
            }
            if let Some(node) = pod.spec.as_ref().and_then(|s| s.node_name.as_deref()) {
                if let Some(count) = index.per_node.get_mut(node) {
                    *count += 1;
                } else {
                    index.per_node.insert(node.to_owned(), 1);
                }
            }
            let (requests, limits) = pod_resources(pod);
            index.requests.add(requests);
            index.limits.add(limits);
            if let Some((issue, _)) = pod_issue(pod, now) {
                let entry = index.issues.entry(owner_of(pod)).or_default();
                *entry = (*entry).max(issue.status());
            }
        }
        index
    }

    pub fn commitments(&self, capacity: Allocatable) -> (Commitment, Commitment) {
        let share = |used: f64, total: f64| {
            if total > 0. { narrow(used / total) } else { 0. }
        };
        let cpu = Commitment {
            requests: share(self.requests.cpu, capacity.cpu_millicores),
            limits: share(self.limits.cpu, capacity.cpu_millicores),
        };
        let memory = Commitment {
            requests: share(self.requests.memory, capacity.memory_bytes),
            limits: share(self.limits.memory, capacity.memory_bytes),
        };
        (cpu, memory)
    }

    pub fn issue(&self, owner: &OwnerKey) -> Option<Status> {
        self.issues.get(owner).copied()
    }
}

pub fn capacity(nodes: &[Arc<Node>]) -> Allocatable {
    nodes
        .iter()
        .map(|node| allocatable(node))
        .fold(Allocatable::default(), |total, next| Allocatable {
            cpu_millicores: total.cpu_millicores + next.cpu_millicores,
            memory_bytes: total.memory_bytes + next.memory_bytes,
        })
}

fn first_label(labels: Option<&BTreeMap<String, String>>, keys: &[&str]) -> Option<String> {
    let labels = labels?;
    keys.iter().find_map(|key| labels.get(*key).cloned())
}

fn condition_healthy(kind: &str, value: &str) -> bool {
    if kind == READY_CONDITION {
        value == CONDITION_TRUE
    } else {
        value == CONDITION_FALSE
    }
}

fn taint_label(taint: &core::Taint) -> String {
    match taint.value.as_deref().filter(|v| !v.is_empty()) {
        Some(value) => format!("{}={value}:{}", taint.key, taint.effect),
        None => format!("{}:{}", taint.key, taint.effect),
    }
}

fn pod_capacity(node: &Node) -> u32 {
    node.status
        .as_ref()
        .and_then(|s| s.allocatable.as_ref())
        .and_then(|m| m.get(PODS))
        .and_then(|q| q.0.parse().ok())
        .unwrap_or_default()
}

pub fn node(node: &Node, pods: u32, usage: Option<&Usage>, now: Timestamp) -> nodes::Node {
    let labels = node.metadata.labels.as_ref();
    let status = node.status.as_ref();
    let info = status.and_then(|s| s.node_info.as_ref());
    let ready = node_ready(node);
    let can_schedule = schedulable(node);
    let (health, state) = match (ready, can_schedule) {
        (false, _) => (Status::Err, STATE_NOT_READY),
        (true, false) => (Status::Warn, STATE_CORDONED),
        (true, true) => (Status::Ok, STATE_READY),
    };
    let platform = info.map(|i| format!("{}/{}", i.operating_system, i.architecture));
    let capacity = allocatable(node);
    let conditions = status
        .and_then(|s| s.conditions.as_ref())
        .into_iter()
        .flatten()
        .map(|c| NodeCondition {
            kind: c.type_.clone(),
            value: c.status.clone(),
            healthy: condition_healthy(&c.type_, &c.status),
            message: c.message.clone().unwrap_or_default(),
        })
        .collect();
    let taints = node
        .spec
        .as_ref()
        .and_then(|s| s.taints.as_ref())
        .into_iter()
        .flatten()
        .map(taint_label)
        .collect();
    let internal_ip = status
        .and_then(|s| s.addresses.as_ref())
        .into_iter()
        .flatten()
        .find(|a| a.type_ == INTERNAL_IP)
        .map(|a| a.address.clone())
        .unwrap_or_default();
    nodes::Node {
        name: node.metadata.name.clone().unwrap_or_default(),
        status: health,
        state: state.to_owned(),
        schedulable: can_schedule,
        instance_type: first_label(labels, &INSTANCE_TYPE_LABELS)
            .or(platform)
            .unwrap_or_default(),
        zone: first_label(labels, &ZONE_LABELS).unwrap_or_default(),
        version: info.map(|i| i.kubelet_version.clone()).unwrap_or_default(),
        internal_ip,
        os_image: info.map(|i| i.os_image.clone()).unwrap_or_default(),
        runtime: info
            .map(|i| i.container_runtime_version.clone())
            .unwrap_or_default(),
        cpu: nodes::Usage {
            used: usage.map_or(0., |u| cores(u.cpu_millicores)),
            allocatable: cores(capacity.cpu_millicores),
        },
        memory: nodes::Usage {
            used: usage.map_or(0., |u| gib(u.memory_bytes)),
            allocatable: gib(capacity.memory_bytes),
        },
        pods,
        pod_capacity: pod_capacity(node),
        age: age(&node.metadata, now),
        conditions,
        taints,
        labels: labels
            .into_iter()
            .flatten()
            .map(|(k, v)| format!("{k}={v}"))
            .collect(),
    }
}

pub fn nodes(
    nodes: &[Arc<Node>],
    index: &PodIndex,
    usage: &[NodeUsage],
    now: Timestamp,
) -> Vec<nodes::Node> {
    let used: HashMap<&str, &Usage> = usage.iter().map(|u| (u.name.as_str(), &u.usage)).collect();
    let mut rows: Vec<nodes::Node> = nodes
        .iter()
        .map(|n| {
            let name = n.metadata.name.as_deref().unwrap_or_default();
            let pods = index.per_node.get(name).copied().unwrap_or_default();
            node(n, pods, used.get(name).copied(), now)
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

pub fn namespaces(
    namespaces: &[Arc<core::Namespace>],
    index: &PodIndex,
    usage: &[PodUsage],
    now: Timestamp,
) -> Vec<Namespace> {
    let mut used: HashMap<&str, Totals> = HashMap::new();
    for pod in usage {
        used.entry(pod.namespace.as_str()).or_default().add(Totals {
            cpu: pod.usage.cpu_millicores,
            memory: pod.usage.memory_bytes,
        });
    }
    let mut rows: Vec<Namespace> = namespaces
        .iter()
        .map(|ns| {
            let name = ns.metadata.name.clone().unwrap_or_default();
            let facts = index.namespaces.get(&name).cloned().unwrap_or_default();
            let totals = used.get(name.as_str()).copied().unwrap_or_default();
            Namespace {
                status: facts.problems.status(),
                phase: ns
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.clone())
                    .unwrap_or_default(),
                pods: facts.pods,
                problems: facts.problems,
                cpu: cores(totals.cpu),
                memory: gib(totals.memory),
                age: age(&ns.metadata, now),
                name,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

fn last_seen(event: &core::Event) -> Option<Timestamp> {
    event
        .series
        .as_ref()
        .and_then(|s| s.last_observed_time.as_ref())
        .map(|t| t.0)
        .or_else(|| event.last_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.event_time.as_ref().map(|t| t.0))
        .or_else(|| event.first_timestamp.as_ref().map(|t| t.0))
        .or_else(|| event.metadata.creation_timestamp.as_ref().map(|t| t.0))
}

pub fn event(event: &core::Event, now: Timestamp) -> Event {
    let reason = event.reason.clone().unwrap_or_default();
    let status = if event.type_.as_deref() != Some(WARNING) {
        Status::Ok
    } else if ERROR_REASONS.contains(&reason.as_str()) {
        Status::Err
    } else {
        Status::Warn
    };
    let involved = &event.involved_object;
    let object = format!(
        "{}/{}",
        involved.kind.as_deref().unwrap_or_default().to_lowercase(),
        involved.name.as_deref().unwrap_or_default()
    );
    let count = event
        .series
        .as_ref()
        .and_then(|s| s.count)
        .or(event.count)
        .and_then(|c| u32::try_from(c).ok())
        .unwrap_or(1);
    Event {
        status,
        reason,
        object,
        namespace: involved.namespace.clone().filter(|ns| !ns.is_empty()),
        message: event
            .message
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        age: last_seen(event)
            .map(|t| between(t, now))
            .unwrap_or_default(),
        count,
    }
}

pub fn events(events: &[Arc<core::Event>], limit: usize, now: Timestamp) -> Vec<Event> {
    let mut rows: Vec<Event> = events.iter().map(|e| event(e, now)).collect();
    rows.sort_by_key(|e| e.age);
    rows.truncate(limit);
    warnings_first(&rows)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::problems::tests::{at, crash_status, now, parse, ready_status, replica_pod};

    fn pod_with_resources(name: &str, node: &str, cpu: &str, memory: &str) -> Arc<Pod> {
        parse(json!({
            "metadata": { "name": name, "namespace": "web" },
            "spec": {
                "nodeName": node,
                "containers": [{
                    "name": "c", "image": "i",
                    "resources": {
                        "requests": { "cpu": cpu, "memory": memory },
                        "limits": { "cpu": "2", "memory": memory }
                    }
                }]
            },
            "status": { "phase": "Running", "conditions": [{ "type": "Ready", "status": "True" }] }
        }))
    }

    fn worker(name: &str) -> Arc<Node> {
        parse(json!({
            "metadata": {
                "name": name,
                "creationTimestamp": at(60),
                "labels": { "topology.kubernetes.io/zone": "us-east-1a", "kubernetes.io/os": "linux" }
            },
            "spec": { "taints": [{ "key": "gpu", "value": "present", "effect": "NoSchedule" }] },
            "status": {
                "allocatable": { "cpu": "4", "memory": "8Gi", "pods": "110" },
                "addresses": [{ "type": "InternalIP", "address": "10.0.0.7" }],
                "nodeInfo": {
                    "architecture": "arm64", "operatingSystem": "linux", "osImage": "Debian",
                    "kubeletVersion": "v1.37.0", "containerRuntimeVersion": "containerd://2.1",
                    "bootID": "", "kernelVersion": "", "kubeProxyVersion": "", "machineID": "", "systemUUID": ""
                },
                "conditions": [
                    { "type": "Ready", "status": "True", "message": "kubelet is posting ready status" },
                    { "type": "MemoryPressure", "status": "False" }
                ]
            }
        }))
    }

    #[test]
    fn overview_sums_requests_limits_and_phases() {
        let pending: Arc<Pod> = parse(json!({
            "metadata": { "name": "p", "namespace": "apps" },
            "status": { "phase": "Pending" }
        }));
        let pods = [
            pod_with_resources("a", "w1", "500m", "1Gi"),
            pod_with_resources("b", "w1", "1500m", "3Gi"),
            replica_pod("api-1-x", "api", "1", &crash_status()),
            pending,
        ];
        let index = PodIndex::build(&pods, now());
        assert_eq!(
            index.phases,
            PodPhases {
                running: 2,
                pending: 1,
                failing: 1
            }
        );
        let nodes = [worker("w1")];
        let (cpu, memory) = index.commitments(capacity(&nodes));
        assert!((cpu.requests - 0.5).abs() < 1e-6);
        assert!((cpu.limits - 1.0).abs() < 1e-6);
        assert!((memory.requests - 0.5).abs() < 1e-6);
        let owner = OwnerKey {
            namespace: "payments".into(),
            kind: "Deployment".into(),
            name: "api".into(),
        };
        assert_eq!(index.issue(&owner), Some(Status::Err));
    }

    #[test]
    fn node_rows_carry_usage_labels_and_pod_counts() {
        let pods = [pod_with_resources("a", "w1", "1", "1Gi")];
        let index = PodIndex::build(&pods, now());
        let usage = [NodeUsage {
            name: "w1".into(),
            usage: Usage {
                cpu_millicores: 1000.,
                memory_bytes: 2. * 1_073_741_824.,
            },
        }];
        let rows = nodes(&[worker("w1")], &index, &usage, now());
        let row = &rows[0];
        assert_eq!((row.status, row.state.as_str()), (Status::Ok, "Ready"));
        assert_eq!(row.instance_type, "linux/arm64");
        assert_eq!(row.zone, "us-east-1a");
        assert_eq!(row.internal_ip, "10.0.0.7");
        assert_eq!(row.pods, 1);
        assert_eq!(row.pod_capacity, 110);
        assert_eq!((row.cpu.used, row.cpu.allocatable), (1., 4.));
        assert_eq!((row.memory.used, row.memory.allocatable), (2., 8.));
        assert_eq!(row.taints, ["gpu=present:NoSchedule"]);
        assert_eq!(row.age, Duration::from_secs(3600));
        assert!(row.conditions.iter().all(|c| c.healthy));
        assert!(row.is_ready());
    }

    #[test]
    fn namespaces_count_pods_problems_and_usage() {
        let namespace: Arc<core::Namespace> = parse(json!({
            "metadata": { "name": "payments", "creationTimestamp": at(1440) },
            "status": { "phase": "Active" }
        }));
        let pods = [
            replica_pod("api-1-x", "api", "1", &crash_status()),
            replica_pod("api-1-y", "api", "1", &ready_status()),
        ];
        let index = PodIndex::build(&pods, now());
        let usage = [PodUsage {
            namespace: "payments".into(),
            name: "api-1-y".into(),
            usage: Usage {
                cpu_millicores: 250.,
                memory_bytes: 536_870_912.,
            },
        }];
        let rows = namespaces(&[namespace], &index, &usage, now());
        let row = &rows[0];
        assert_eq!(row.pods, 2);
        assert_eq!(row.problems, Counts::default().with_err(1));
        assert_eq!(row.status, Status::Err);
        assert_eq!((row.cpu, row.memory), (0.25, 0.5));
        assert_eq!(row.phase, "Active");
    }

    #[test]
    fn events_put_errors_and_warnings_first_then_newest() {
        let event = |reason: &str, kind: &str, minutes: i64| -> Arc<core::Event> {
            parse(json!({
                "metadata": { "name": format!("e-{reason}"), "namespace": "payments" },
                "involvedObject": { "kind": "Pod", "name": "checkout-api-x", "namespace": "payments" },
                "reason": reason, "type": kind, "message": format!("{reason} happened "),
                "lastTimestamp": at(minutes), "count": 3
            }))
        };
        let rows = events(
            &[
                event("Scheduled", "Normal", 1),
                event("Unhealthy", "Warning", 11),
                event("BackOff", "Warning", 2),
                event("Pulled", "Normal", 30),
            ],
            3,
            now(),
        );
        let reasons: Vec<(&str, Status)> =
            rows.iter().map(|e| (e.reason.as_str(), e.status)).collect();
        assert_eq!(
            reasons,
            [
                ("BackOff", Status::Err),
                ("Unhealthy", Status::Warn),
                ("Scheduled", Status::Ok)
            ]
        );
        let first = &rows[0];
        assert_eq!(first.object, "pod/checkout-api-x");
        assert_eq!(first.message, "BackOff happened");
        assert_eq!((first.count, first.age), (3, Duration::from_secs(120)));
    }
}
