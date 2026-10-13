use std::collections::BTreeMap;

use fleet_core::ResourceRef;
use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::Api;
use kube::api::ListParams;

use crate::error::{Error, Result};
use crate::manager::ClusterHandle;
use crate::ops::ForwardInfo;

const SERVICE: &str = "Service";
const POD: &str = "Pod";
const RUNNING: &str = "Running";
const READY: &str = "Ready";
const TRUE: &str = "True";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
    pub pod: String,
    pub port: u16,
}

fn is_ready(pod: &Pod) -> bool {
    if pod.metadata.deletion_timestamp.is_some() {
        return false;
    }
    let Some(status) = &pod.status else {
        return false;
    };
    let running = status.phase.as_deref() == Some(RUNNING);
    let ready = status
        .conditions
        .iter()
        .flatten()
        .any(|condition| condition.type_ == READY && condition.status == TRUE);
    running && ready
}

fn named_port(pod: &Pod, name: &str) -> Option<u16> {
    pod.spec
        .as_ref()?
        .containers
        .iter()
        .flat_map(|container| container.ports.iter().flatten())
        .find(|port| port.name.as_deref() == Some(name))
        .and_then(|port| u16::try_from(port.container_port).ok())
}

pub fn selector(service: &Service) -> Option<String> {
    let selector: &BTreeMap<String, String> = service.spec.as_ref()?.selector.as_ref()?;
    if selector.is_empty() {
        return None;
    }
    let terms: Vec<String> = selector
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    Some(terms.join(","))
}

pub fn resolve_backend(service: &Service, pods: &[Pod], port: u16) -> Result<Backend> {
    let name = service.metadata.name.clone().unwrap_or_default();
    let service_port = service
        .spec
        .as_ref()
        .and_then(|spec| spec.ports.as_ref())
        .into_iter()
        .flatten()
        .find(|candidate| u16::try_from(candidate.port).ok() == Some(port))
        .ok_or_else(|| Error::NoServicePort {
            service: name.clone(),
            port,
        })?;
    let mut ready: Vec<&Pod> = pods.iter().filter(|pod| is_ready(pod)).collect();
    ready.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
    let pod = ready.first().ok_or(Error::NoReadyPod(name))?;
    let pod_name = pod.metadata.name.clone().unwrap_or_default();
    let target = match &service_port.target_port {
        None => Some(port),
        Some(IntOrString::Int(number)) => u16::try_from(*number).ok(),
        Some(IntOrString::String(port_name)) => {
            let found = named_port(pod, port_name);
            if found.is_none() {
                return Err(Error::NoNamedPort {
                    pod: pod_name,
                    name: port_name.clone(),
                });
            }
            found
        }
    };
    let port = target.ok_or(Error::MissingField("spec.ports.targetPort"))?;
    Ok(Backend {
        pod: pod_name,
        port,
    })
}

async fn backend(handle: &ClusterHandle, target: &ResourceRef, remote: u16) -> Result<Backend> {
    let namespace = target
        .namespace
        .as_deref()
        .ok_or(Error::MissingField("metadata.namespace"))?;
    match target.kind.as_str() {
        POD => Ok(Backend {
            pod: target.name.clone(),
            port: remote,
        }),
        SERVICE => {
            let client = handle.client()?;
            let services: Api<Service> = Api::namespaced(client.clone(), namespace);
            let service = services.get(&target.name).await?;
            let labels =
                selector(&service).ok_or_else(|| Error::NoSelector(target.name.clone()))?;
            let pods: Api<Pod> = Api::namespaced(client, namespace);
            let listed = pods.list(&ListParams::default().labels(&labels)).await?;
            resolve_backend(&service, &listed.items, remote)
        }
        other => Err(Error::Unsupported {
            action: "Port-forward",
            kind: other.to_owned(),
        }),
    }
}

pub async fn start(
    handle: &ClusterHandle,
    target: &ResourceRef,
    local: u16,
    remote: u16,
) -> Result<ForwardInfo> {
    let backend = backend(handle, target, remote).await?;
    let namespace = target.namespace.as_deref().unwrap_or_default();
    handle
        .port_forward(namespace, &backend.pod, backend.port, local)
        .await
}

#[cfg(test)]
mod tests {
    use k8s_openapi::api::core::v1::{
        Container, ContainerPort, PodCondition, PodSpec, PodStatus, ServicePort, ServiceSpec,
    };
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};

    use super::*;

    fn service(target: Option<IntOrString>) -> Service {
        Service {
            metadata: ObjectMeta {
                name: Some("podinfo".into()),
                ..ObjectMeta::default()
            },
            spec: Some(ServiceSpec {
                selector: Some(BTreeMap::from([
                    ("app".into(), "podinfo".into()),
                    ("tier".into(), "web".into()),
                ])),
                ports: Some(vec![ServicePort {
                    port: 80,
                    target_port: target,
                    ..ServicePort::default()
                }]),
                ..ServiceSpec::default()
            }),
            ..Service::default()
        }
    }

    fn pod(name: &str, ready: bool) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.into()),
                ..ObjectMeta::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "web".into(),
                    ports: Some(vec![ContainerPort {
                        name: Some("http".into()),
                        container_port: 9898,
                        ..ContainerPort::default()
                    }]),
                    ..Container::default()
                }],
                ..PodSpec::default()
            }),
            status: Some(PodStatus {
                phase: Some(RUNNING.into()),
                conditions: Some(vec![PodCondition {
                    type_: READY.into(),
                    status: if ready { TRUE } else { "False" }.into(),
                    ..PodCondition::default()
                }]),
                ..PodStatus::default()
            }),
        }
    }

    #[test]
    fn named_target_ports_resolve_on_the_first_ready_pod() {
        let svc = service(Some(IntOrString::String("http".into())));
        let pods = [
            pod("podinfo-c", true),
            pod("podinfo-a", false),
            pod("podinfo-b", true),
        ];
        let backend = resolve_backend(&svc, &pods, 80).ok();
        assert_eq!(
            backend,
            Some(Backend {
                pod: "podinfo-b".into(),
                port: 9898
            })
        );
    }

    #[test]
    fn numeric_and_missing_target_ports() {
        let pods = [pod("podinfo-a", true)];
        let numeric = resolve_backend(&service(Some(IntOrString::Int(8080))), &pods, 80);
        assert_eq!(numeric.ok().map(|b| b.port), Some(8080));
        let same = resolve_backend(&service(None), &pods, 80);
        assert_eq!(same.ok().map(|b| b.port), Some(80));
    }

    #[test]
    fn explains_why_a_service_cannot_be_forwarded() {
        let svc = service(Some(IntOrString::String("metrics".into())));
        let ready = [pod("podinfo-a", true)];
        assert!(matches!(
            resolve_backend(&svc, &ready, 80),
            Err(Error::NoNamedPort { name, .. }) if name == "metrics"
        ));
        assert!(matches!(
            resolve_backend(&svc, &ready, 443),
            Err(Error::NoServicePort { port: 443, .. })
        ));
        let mut terminating = pod("podinfo-a", true);
        terminating.metadata.deletion_timestamp = Some(Time(jiff::Timestamp::UNIX_EPOCH));
        assert!(matches!(
            resolve_backend(&svc, &[terminating, pod("podinfo-b", false)], 80),
            Err(Error::NoReadyPod(name)) if name == "podinfo"
        ));
    }

    #[test]
    fn selectors_become_label_queries() {
        assert_eq!(
            selector(&service(None)).as_deref(),
            Some("app=podinfo,tier=web")
        );
        assert_eq!(selector(&Service::default()), None);
    }
}
