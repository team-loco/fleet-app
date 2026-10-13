use k8s_openapi::{
    api::{core::v1::Service, discovery::v1::EndpointSlice},
    apimachinery::pkg::apis::meta::v1::{Condition, ObjectMeta},
};
use serde::{Deserialize, Serialize};

use crate::{Error, ObjectRef, label};

pub const GATEWAY_NAME: &str = "eg";
pub const SERVICE_NAME_LABEL: &str = "kubernetes.io/service-name";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gateway {
    pub metadata: ObjectMeta,
    pub spec: GatewaySpec,
    #[serde(default)]
    pub status: GatewayStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GatewaySpec {
    pub listeners: Vec<Listener>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Listener {
    pub name: String,
    pub hostname: Option<String>,
    pub port: i32,
    pub protocol: String,
    pub tls: Option<ListenerTls>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerTls {
    #[serde(default)]
    pub certificate_refs: Vec<CertificateRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CertificateRef {
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GatewayStatus {
    #[serde(default)]
    pub listeners: Vec<ListenerStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerStatus {
    pub name: String,
    pub attached_routes: Option<i32>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListenerModel {
    pub name: String,
    pub hostname: Option<String>,
    pub port: i32,
    pub protocol: String,
    pub attached_routes: Option<i32>,
    pub certificates: Vec<ObjectRef>,
    pub problems: Vec<String>,
}

pub fn gateway_listeners(gateway: &Gateway) -> Result<Vec<ListenerModel>, Error> {
    let reference = ObjectRef::from_metadata("Gateway", &gateway.metadata)?;
    let mut listeners = Vec::new();
    for listener in &gateway.spec.listeners {
        let status = gateway
            .status
            .listeners
            .iter()
            .find(|status| status.name == listener.name);
        let certificates = listener
            .tls
            .as_ref()
            .map(|tls| {
                tls.certificate_refs
                    .iter()
                    .map(|certificate| ObjectRef {
                        kind: "Secret".to_owned(),
                        namespace: certificate
                            .namespace
                            .clone()
                            .unwrap_or_else(|| reference.namespace.clone()),
                        name: certificate.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let problems = status
            .map(|status| {
                status
                    .conditions
                    .iter()
                    .filter(|condition| {
                        condition.status == "False"
                            && condition.observed_generation == gateway.metadata.generation
                    })
                    .map(|condition| condition.message.clone())
                    .collect()
            })
            .unwrap_or_default();
        listeners.push(ListenerModel {
            name: listener.name.clone(),
            hostname: listener.hostname.clone(),
            port: listener.port,
            protocol: listener.protocol.clone(),
            attached_routes: status.and_then(|status| status.attached_routes),
            certificates,
            problems,
        });
    }
    Ok(listeners)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpRoute {
    pub metadata: ObjectMeta,
    pub spec: HttpRouteSpec,
    #[serde(default)]
    pub status: HttpRouteStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpRouteSpec {
    #[serde(default)]
    pub hostnames: Vec<String>,
    #[serde(default)]
    pub rules: Vec<RouteRule>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    #[serde(default)]
    pub backend_refs: Vec<BackendRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackendRef {
    pub name: String,
    pub namespace: Option<String>,
    pub group: Option<String>,
    pub kind: Option<String>,
    pub port: Option<i32>,
    pub weight: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct HttpRouteStatus {
    #[serde(default)]
    pub parents: Vec<RouteParentStatus>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteParentStatus {
    pub parent_ref: ParentRef,
    pub controller_name: String,
    #[serde(default)]
    pub conditions: Vec<Condition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParentRef {
    pub name: String,
    pub namespace: Option<String>,
    pub group: Option<String>,
    pub kind: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteProblem {
    BackendNotFound,
    NoReadyEndpoints,
    Rejected,
    Unknown,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendModel {
    pub service: ObjectRef,
    pub port: Option<i32>,
    pub ready_endpoints: Option<usize>,
    pub total_endpoints: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteModel {
    pub route: ObjectRef,
    pub hostnames: Vec<String>,
    pub backends: Vec<BackendModel>,
    pub problem: RouteProblem,
    pub messages: Vec<String>,
}

pub fn route_model(
    route: &HttpRoute,
    gateway_namespace: &str,
    services: &[Service],
    slices: &[EndpointSlice],
) -> Result<RouteModel, Error> {
    let reference = ObjectRef::from_metadata("HTTPRoute", &route.metadata)?;
    let parents: Vec<_> = route
        .status
        .parents
        .iter()
        .filter(|parent| {
            parent.parent_ref.name == GATEWAY_NAME
                && parent
                    .parent_ref
                    .namespace
                    .as_deref()
                    .unwrap_or(&reference.namespace)
                    == gateway_namespace
                && parent.parent_ref.kind.as_deref().unwrap_or("Gateway") == "Gateway"
                && parent
                    .parent_ref
                    .group
                    .as_deref()
                    .unwrap_or("gateway.networking.k8s.io")
                    == "gateway.networking.k8s.io"
                && parent.controller_name == "gateway.envoyproxy.io/gatewayclass-controller"
        })
        .collect();
    let conditions: Vec<_> = parents
        .iter()
        .flat_map(|parent| &parent.conditions)
        .filter(|condition| condition.observed_generation == route.metadata.generation)
        .collect();
    let accepted = conditions
        .iter()
        .any(|condition| condition.type_ == "Accepted" && condition.status == "True");
    let mut problem = if accepted {
        RouteProblem::None
    } else {
        RouteProblem::Unknown
    };
    let mut messages = Vec::new();
    for condition in conditions {
        if condition.status == "False" {
            let rank = if condition.reason == "BackendNotFound" {
                RouteProblem::BackendNotFound
            } else {
                RouteProblem::Rejected
            };
            problem = problem.min(rank);
            messages.push(condition.message.clone());
        }
    }
    let mut backends = Vec::new();
    for backend in route.spec.rules.iter().flat_map(|rule| &rule.backend_refs) {
        if backend.weight == Some(0) {
            continue;
        }
        if backend
            .group
            .as_deref()
            .is_some_and(|group| !group.is_empty())
            || backend
                .kind
                .as_deref()
                .is_some_and(|kind| kind != "Service")
        {
            problem = problem.min(RouteProblem::Unknown);
            continue;
        }
        let namespace = backend.namespace.as_deref().unwrap_or(&reference.namespace);
        let service = ObjectRef {
            kind: "Service".to_owned(),
            namespace: namespace.to_owned(),
            name: backend.name.clone(),
        };
        let matched_service = services.iter().find(|candidate| {
            candidate.metadata.namespace.as_deref() == Some(namespace)
                && candidate.metadata.name.as_deref() == Some(backend.name.as_str())
        });
        let exists = matched_service.is_some();
        let external = matched_service
            .and_then(|service| service.spec.as_ref())
            .is_some_and(|spec| spec.type_.as_deref() == Some("ExternalName"));
        let endpoint_slices: Vec<_> = slices
            .iter()
            .filter(|slice| {
                slice.metadata.namespace.as_deref() == Some(namespace)
                    && label(&slice.metadata, SERVICE_NAME_LABEL) == Some(backend.name.as_str())
            })
            .collect();
        let counts = if exists && !external {
            let mut endpoints = std::collections::BTreeMap::new();
            for endpoint in endpoint_slices.iter().flat_map(|slice| &slice.endpoints) {
                let key = endpoint
                    .target_ref
                    .as_ref()
                    .and_then(|target| target.uid.clone())
                    .unwrap_or_else(|| {
                        let mut addresses = endpoint.addresses.clone();
                        addresses.sort();
                        addresses.join(",")
                    });
                let ready = endpoint
                    .conditions
                    .as_ref()
                    .and_then(|conditions| conditions.ready)
                    .unwrap_or(true);
                endpoints
                    .entry(key)
                    .and_modify(|value| *value = *value || ready)
                    .or_insert(ready);
            }
            let ready = endpoints.values().filter(|ready| **ready).count();
            Some((ready, endpoints.len()))
        } else {
            None
        };
        if !exists {
            problem = problem.min(RouteProblem::BackendNotFound);
        }
        if counts.is_some_and(|(ready, _)| ready == 0) {
            problem = problem.min(RouteProblem::NoReadyEndpoints);
        }
        if exists && counts.is_none() {
            problem = problem.min(RouteProblem::Unknown);
        }
        backends.push(BackendModel {
            service,
            port: backend.port,
            ready_endpoints: counts.map(|counts| counts.0),
            total_endpoints: counts.map(|counts| counts.1),
        });
    }
    Ok(RouteModel {
        route: reference,
        hostnames: route.spec.hostnames.clone(),
        backends,
        problem,
        messages,
    })
}

pub fn rank_routes(routes: &mut [RouteModel]) {
    routes.sort_by(|left, right| {
        left.problem
            .cmp(&right.problem)
            .then_with(|| left.route.cmp(&right.route))
    });
}
