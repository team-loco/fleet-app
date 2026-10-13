use chrono::{DateTime, Utc};
use k8s_openapi::{
    api::{
        apps::v1::{DaemonSet, Deployment, StatefulSet},
        core::v1::Pod,
    },
    apimachinery::pkg::apis::meta::v1::APIResourceList,
};

use crate::{API_VERSION, Error, ObjectRef};

const DEFAULT_REPLICAS: i32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentSelector {
    Name(String),
    Labels(Vec<(String, String)>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub namespace: String,
    pub kind: String,
    pub selector: ComponentSelector,
}

pub fn component_catalog() -> Vec<Component> {
    let named = |name: &str, namespace: &str, kind: &str, workload: &str| Component {
        name: name.to_owned(),
        namespace: namespace.to_owned(),
        kind: kind.to_owned(),
        selector: ComponentSelector::Name(workload.to_owned()),
    };
    vec![
        named("agent", "loco-system", "Deployment", "loco-core-agent"),
        named(
            "loco-controller",
            "loco-system",
            "Deployment",
            "loco-controller",
        ),
        named(
            "loco-build-controller",
            "loco-system",
            "Deployment",
            "loco-build-controller",
        ),
        named(
            "envoy-gateway",
            "loco-system",
            "Deployment",
            "envoy-gateway",
        ),
        Component {
            name: "envoy-proxy".to_owned(),
            namespace: "loco-system".to_owned(),
            kind: "Deployment".to_owned(),
            selector: ComponentSelector::Labels(vec![
                (
                    "gateway.envoyproxy.io/owning-gateway-name".to_owned(),
                    "eg".to_owned(),
                ),
                (
                    "gateway.envoyproxy.io/owning-gateway-namespace".to_owned(),
                    "loco-system".to_owned(),
                ),
            ]),
        },
        named("cert-manager", "cert-manager", "Deployment", "cert-manager"),
        named("cilium", "kube-system", "DaemonSet", "cilium"),
        named(
            "otel-collector-node",
            "observability",
            "DaemonSet",
            "otel-col-daemon",
        ),
        named(
            "otel-collector",
            "observability",
            "Deployment",
            "otel-col-deploy",
        ),
        Component {
            name: "clickhouse".to_owned(),
            namespace: "observability".to_owned(),
            kind: "Pod".to_owned(),
            selector: ComponentSelector::Labels(vec![
                ("app.kubernetes.io/name".to_owned(), "clickhouse".to_owned()),
                (
                    "app.kubernetes.io/instance".to_owned(),
                    "loco-obs".to_owned(),
                ),
            ]),
        },
        named(
            "observability-proxy",
            "observability",
            "Deployment",
            "loco-obs-obs-proxy",
        ),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Health {
    Unhealthy,
    Progressing,
    Unknown,
    Suspended,
    Healthy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkloadHealth {
    pub reference: ObjectRef,
    pub health: Health,
    pub ready: Option<i32>,
    pub desired: Option<i32>,
    pub images: Vec<String>,
    pub labels: std::collections::BTreeMap<String, String>,
}

fn readiness(ready: Option<i32>, desired: Option<i32>, current: bool) -> Health {
    if !current {
        return Health::Unknown;
    }
    match (ready, desired) {
        (_, Some(0)) => Health::Suspended,
        (Some(ready), Some(desired)) if ready == desired => Health::Healthy,
        (Some(0), Some(_)) => Health::Unhealthy,
        (Some(_), Some(_)) => Health::Progressing,
        _ => Health::Unknown,
    }
}

pub fn deployment_health(deployment: &Deployment) -> Result<WorkloadHealth, Error> {
    let reference = ObjectRef::from_metadata("Deployment", &deployment.metadata)?;
    let desired = deployment
        .spec
        .as_ref()
        .map(|spec| spec.replicas.unwrap_or(DEFAULT_REPLICAS));
    let status = deployment.status.as_ref();
    let ready = status.map(|status| status.ready_replicas.unwrap_or_default());
    let current = status.is_some_and(|status| {
        status.observed_generation.is_some()
            && status.observed_generation == deployment.metadata.generation
    });
    let failed = status
        .and_then(|status| status.conditions.as_ref())
        .is_some_and(|conditions| {
            conditions
                .iter()
                .any(|condition| condition.type_ == "Progressing" && condition.status == "False")
        });
    let health = if current && failed {
        Health::Unhealthy
    } else {
        readiness(ready, desired, current)
    };
    let rolling = status.is_some_and(|status| {
        status.updated_replicas.unwrap_or_default() != desired.unwrap_or_default()
    });
    let health = if health == Health::Healthy && rolling {
        Health::Progressing
    } else {
        health
    };
    let images = deployment
        .spec
        .as_ref()
        .and_then(|spec| spec.template.spec.as_ref())
        .map(|spec| {
            spec.containers
                .iter()
                .filter_map(|container| container.image.clone())
                .collect()
        })
        .unwrap_or_default();
    Ok(WorkloadHealth {
        reference,
        health,
        ready,
        desired,
        images,
        labels: deployment.metadata.labels.clone().unwrap_or_default(),
    })
}

pub fn daemonset_health(daemonset: &DaemonSet) -> Result<WorkloadHealth, Error> {
    let reference = ObjectRef::from_metadata("DaemonSet", &daemonset.metadata)?;
    let status = daemonset.status.as_ref();
    let ready = status.map(|status| status.number_ready);
    let desired = status.map(|status| status.desired_number_scheduled);
    let current = status.is_some_and(|status| {
        status.observed_generation.is_some()
            && status.observed_generation == daemonset.metadata.generation
    });
    let health = readiness(ready, desired, current);
    let rolling = status.is_some_and(|status| {
        status.updated_number_scheduled.unwrap_or_default() != status.desired_number_scheduled
    });
    let health = if health == Health::Healthy && rolling {
        Health::Progressing
    } else {
        health
    };
    let images = daemonset
        .spec
        .as_ref()
        .and_then(|spec| spec.template.spec.as_ref())
        .map(|spec| {
            spec.containers
                .iter()
                .filter_map(|container| container.image.clone())
                .collect()
        })
        .unwrap_or_default();
    Ok(WorkloadHealth {
        reference,
        health,
        ready,
        desired,
        images,
        labels: daemonset.metadata.labels.clone().unwrap_or_default(),
    })
}

pub fn statefulset_health(statefulset: &StatefulSet) -> Result<WorkloadHealth, Error> {
    let reference = ObjectRef::from_metadata("StatefulSet", &statefulset.metadata)?;
    let status = statefulset.status.as_ref();
    let ready = status.map(|status| status.ready_replicas.unwrap_or_default());
    let desired = statefulset
        .spec
        .as_ref()
        .map(|spec| spec.replicas.unwrap_or(DEFAULT_REPLICAS));
    let current = status.is_some_and(|status| {
        status.observed_generation.is_some()
            && status.observed_generation == statefulset.metadata.generation
    });
    let health = readiness(ready, desired, current);
    let rolling = status.is_some_and(|status| status.current_revision != status.update_revision);
    let health = if health == Health::Healthy && rolling {
        Health::Progressing
    } else {
        health
    };
    let images = statefulset
        .spec
        .as_ref()
        .and_then(|spec| spec.template.spec.as_ref())
        .map(|spec| {
            spec.containers
                .iter()
                .filter_map(|container| container.image.clone())
                .collect()
        })
        .unwrap_or_default();
    Ok(WorkloadHealth {
        reference,
        health,
        ready,
        desired,
        images,
        labels: statefulset.metadata.labels.clone().unwrap_or_default(),
    })
}

pub fn pod_health(pod: &Pod) -> Result<WorkloadHealth, Error> {
    let reference = ObjectRef::from_metadata("Pod", &pod.metadata)?;
    let ready = pod
        .status
        .as_ref()
        .and_then(|status| status.conditions.as_ref())
        .map(|conditions| {
            i32::from(
                conditions
                    .iter()
                    .any(|condition| condition.type_ == "Ready" && condition.status == "True"),
            )
        });
    let desired = Some(DEFAULT_REPLICAS);
    let health = readiness(ready, desired, pod.status.is_some());
    let images = pod
        .spec
        .as_ref()
        .map(|spec| {
            spec.containers
                .iter()
                .filter_map(|container| container.image.clone())
                .collect()
        })
        .unwrap_or_default();
    Ok(WorkloadHealth {
        reference,
        health,
        ready,
        desired,
        images,
        labels: pod.metadata.labels.clone().unwrap_or_default(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentHealth {
    pub component: Component,
    pub health: Health,
    pub workloads: Vec<WorkloadHealth>,
}

pub fn component_health(component: &Component, workloads: &[WorkloadHealth]) -> ComponentHealth {
    let matches = |workload: &&WorkloadHealth| {
        if workload.reference.namespace != component.namespace
            || workload.reference.kind != component.kind
        {
            return false;
        }
        match &component.selector {
            ComponentSelector::Name(name) => &workload.reference.name == name,
            ComponentSelector::Labels(labels) => labels
                .iter()
                .all(|(key, value)| workload.labels.get(key) == Some(value)),
        }
    };
    let mut workloads: Vec<_> = workloads.iter().filter(matches).cloned().collect();
    workloads.sort_by(|left, right| left.reference.cmp(&right.reference));
    let health = health_rollup(workloads.iter().map(|workload| workload.health));
    ComponentHealth {
        component: component.clone(),
        health,
        workloads,
    }
}

pub fn health_rollup(health: impl IntoIterator<Item = Health>) -> Health {
    health.into_iter().min().unwrap_or(Health::Unknown)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observation<T> {
    Observed(T),
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSync {
    pub connected: Observation<bool>,
    pub last_heartbeat: Observation<DateTime<Utc>>,
    pub reconnects: Observation<u64>,
    pub reported_kubernetes_health: Observation<bool>,
    pub reported_controller_health: Observation<bool>,
    pub builds_enabled: Observation<bool>,
}

pub fn agent_sync(discovery: Option<&APIResourceList>) -> AgentSync {
    let builds_enabled = match discovery {
        Some(resources) if resources.group_version == API_VERSION => Observation::Observed(
            resources
                .resources
                .iter()
                .any(|resource| resource.name == "builds"),
        ),
        _ => Observation::Unavailable,
    };
    AgentSync {
        connected: Observation::Unavailable,
        last_heartbeat: Observation::Unavailable,
        reconnects: Observation::Unavailable,
        reported_kubernetes_health: Observation::Unavailable,
        reported_controller_health: Observation::Unavailable,
        builds_enabled,
    }
}
