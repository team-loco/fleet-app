use std::time::Duration;

use futures::{StreamExt, TryStreamExt, stream};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::Client;
use kube::ResourceExt;
use kube::api::ListParams;
use serde_json::{Value, json};

use super::failure::{ActionFailureKind, action_failure};
use crate::error::{Error, Result};
use crate::manager::ClusterHandle;

pub const MIRROR_ANNOTATION: &str = "kubernetes.io/config.mirror";
const DAEMON_SET: &str = "DaemonSet";
const NODE_FIELD: &str = "spec.nodeName";
const EVICTION_RETRY: Duration = Duration::from_secs(5);
const EVICTION_TIMEOUT: Duration = Duration::from_secs(60);
const EVICTION_LIMIT: usize = 8;
const EVICTION_API_VERSION: &str = "policy/v1";
const EVICTION_KIND: &str = "Eviction";
const JSON: &str = "application/json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    pub namespace: String,
    pub name: String,
}

fn daemon_owned(pod: &Pod) -> bool {
    pod.owner_references()
        .iter()
        .any(|owner| owner.controller == Some(true) && owner.kind == DAEMON_SET)
}

fn mirrored(pod: &Pod) -> bool {
    pod.annotations().contains_key(MIRROR_ANNOTATION)
}

pub fn drain_candidates(pods: &[Pod]) -> Vec<&Pod> {
    pods.iter()
        .filter(|pod| pod.metadata.deletion_timestamp.is_none())
        .filter(|pod| !daemon_owned(pod))
        .filter(|pod| !mirrored(pod))
        .collect()
}

fn eviction(namespace: &str, name: &str, uid: &str) -> Result<http::Request<Vec<u8>>> {
    let body = json!({
        "apiVersion": EVICTION_API_VERSION,
        "kind": EVICTION_KIND,
        "metadata": { "name": name, "namespace": namespace },
        "deleteOptions": { "preconditions": { "uid": uid } }
    });
    let path = format!("/api/v1/namespaces/{namespace}/pods/{name}/eviction");
    let request = http::Request::post(path)
        .header(http::header::CONTENT_TYPE, JSON)
        .body(serde_json::to_vec(&body)?)?;
    Ok(request)
}

async fn present(pods: &Api<Pod>, name: &str, uid: &str) -> Result<bool> {
    let found = pods.get_opt(name).await?;
    Ok(found.is_some_and(|pod| pod.metadata.uid.as_deref() == Some(uid)))
}

async fn evict(client: &Client, target: &Target) -> Result<bool> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), &target.namespace);
    let deadline = tokio::time::Instant::now() + EVICTION_TIMEOUT;
    loop {
        let request = eviction(&target.namespace, &target.name, &target.uid)?;
        let error = match client.request::<Value>(request).await {
            Ok(_) => return Ok(true),
            Err(error) => Error::Kube(error),
        };
        match action_failure(&error).kind {
            ActionFailureKind::NotFound => return Ok(true),
            ActionFailureKind::Conflict => {
                if present(&pods, &target.name, &target.uid).await? {
                    return Err(error);
                }
                return Ok(true);
            }
            ActionFailureKind::Blocked => {}
            _ => return Err(error),
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(EVICTION_RETRY).await;
    }
}

struct Target {
    namespace: String,
    name: String,
    uid: String,
}

fn target(pod: &Pod) -> Result<Target> {
    let uid = pod
        .metadata
        .uid
        .clone()
        .ok_or(Error::MissingField("metadata.uid"))?;
    Ok(Target {
        namespace: pod.namespace().unwrap_or_default(),
        name: pod.name_any(),
        uid,
    })
}

pub async fn drain(handle: &ClusterHandle, node: &str) -> Result<Vec<Blocked>> {
    let client = handle.client()?;
    let all: Api<Pod> = Api::all(client.clone());
    let selector = format!("{NODE_FIELD}={node}");
    let listed = all.list(&ListParams::default().fields(&selector)).await?;
    let targets: Vec<Target> = drain_candidates(&listed.items)
        .into_iter()
        .map(target)
        .collect::<Result<_>>()?;
    let evictions = targets.into_iter().enumerate().map(|(ix, target)| {
        let client = client.clone();
        async move {
            let evicted = evict(&client, &target).await?;
            let blocked = (!evicted).then_some(Blocked {
                namespace: target.namespace,
                name: target.name,
            });
            Ok::<_, Error>((ix, blocked))
        }
    });
    let mut outcomes: Vec<(usize, Option<Blocked>)> = stream::iter(evictions)
        .buffer_unordered(EVICTION_LIMIT)
        .try_collect()
        .await?;
    outcomes.sort_by_key(|(ix, _)| *ix);
    Ok(outcomes
        .into_iter()
        .filter_map(|(_, blocked)| blocked)
        .collect())
}

pub fn blocked_line(blocked: &[Blocked]) -> String {
    let names: Vec<String> = blocked
        .iter()
        .map(|pod| format!("{}/{}", pod.namespace, pod.name))
        .collect();
    let noun = if blocked.len() == 1 { "pod" } else { "pods" };
    format!(
        "{} {noun} blocked by a disruption budget: {}",
        blocked.len(),
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference, Time};

    use super::*;

    fn pod(name: &str) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.into()),
                namespace: Some("apps".into()),
                ..ObjectMeta::default()
            },
            ..Pod::default()
        }
    }

    fn owned(name: &str, kind: &str) -> Pod {
        let mut pod = pod(name);
        pod.metadata.owner_references = Some(vec![OwnerReference {
            kind: kind.into(),
            controller: Some(true),
            ..OwnerReference::default()
        }]);
        pod
    }

    #[test]
    fn drain_skips_daemon_sets_mirrors_and_terminating_pods() {
        let mut mirror = pod("kube-apiserver-n1");
        mirror.metadata.annotations = Some(BTreeMap::from([(
            MIRROR_ANNOTATION.to_owned(),
            "x".to_owned(),
        )]));
        let mut leaving = pod("leaving");
        leaving.metadata.deletion_timestamp = Some(Time(jiff::Timestamp::UNIX_EPOCH));
        let pods = vec![
            owned("web-1", "ReplicaSet"),
            owned("fluent-bit-x", DAEMON_SET),
            mirror,
            leaving,
            pod("bare"),
            owned("db-0", "StatefulSet"),
        ];
        let names: Vec<String> = drain_candidates(&pods)
            .iter()
            .map(|pod| pod.name_any())
            .collect();
        assert_eq!(names, ["web-1", "bare", "db-0"]);
    }

    const ROUND_TRIP: Duration = Duration::from_secs(1);
    const BLOCKED_POD: &str = "checkout-1";
    const PODS: usize = 10;
    const TOO_MANY: &str = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","reason":"TooManyRequests","code":429,"message":"Cannot evict pod as it would violate the pod's disruption budget."}"#;
    const EVICTED: &str = r#"{"kind":"Status","apiVersion":"v1","status":"Success","code":201}"#;

    fn pod_json(name: &str, uid: &str, node: &str) -> String {
        format!(
            r#"{{"apiVersion":"v1","kind":"Pod","metadata":{{"name":"{name}","namespace":"apps","uid":"{uid}"}},"spec":{{"nodeName":"{node}","containers":[]}}}}"#
        )
    }

    fn pod_list() -> String {
        let items: Vec<String> = (1..PODS)
            .map(|ix| format!("web-{ix}"))
            .chain(std::iter::once(BLOCKED_POD.to_owned()))
            .map(|name| pod_json(&name, &name, NODE_A))
            .collect();
        format!(
            r#"{{"kind":"PodList","apiVersion":"v1","metadata":{{}},"items":[{}]}}"#,
            items.join(",")
        )
    }

    #[tokio::test(start_paused = true)]
    async fn evictions_overlap_and_budget_blocked_pods_keep_retrying() {
        let list = pod_list();
        let (client, traffic) = crate::fake::client(ROUND_TRIP, move |method, path| {
            if method == http::Method::POST && path.contains(&format!("/pods/{BLOCKED_POD}/")) {
                return Some((http::StatusCode::TOO_MANY_REQUESTS, TOO_MANY.to_owned()));
            }
            if method == http::Method::POST {
                return Some((http::StatusCode::CREATED, EVICTED.to_owned()));
            }
            if let Some(name) = path.strip_prefix(PODS_PATH) {
                return Some((http::StatusCode::OK, pod_json(name, name, NODE_A)));
            }
            path.starts_with("/api/v1/pods")
                .then(|| (http::StatusCode::OK, list.clone()))
        });
        let (_manager, handle) = crate::fake::handle(&client);
        let started = tokio::time::Instant::now();
        let drained = drain(&handle, NODE_A).await;
        let took = started.elapsed();
        let Ok(blocked) = drained else {
            panic!("drain failed: {drained:?}");
        };
        assert_eq!(
            blocked,
            [Blocked {
                namespace: "apps".into(),
                name: BLOCKED_POD.into()
            }]
        );
        assert!(traffic.peak() >= EVICTION_LIMIT, "{}", traffic.peak());
        assert!(
            took < EVICTION_TIMEOUT + EVICTION_RETRY + ROUND_TRIP * 3,
            "{took:?}"
        );
        let retries = traffic
            .requests()
            .iter()
            .filter(|(method, path)| method == http::Method::POST && path.contains(BLOCKED_POD))
            .count();
        assert!(retries > 1, "{retries}");
    }

    const NODE_A: &str = "node-a";
    const NODE_B: &str = "node-b";
    const PODS_PATH: &str = "/api/v1/namespaces/apps/pods/";
    const DB: &str = "db-0";
    const OLD_UID: &str = "old-node-a-pod";
    const NEW_UID: &str = "new-node-b-pod";
    const CONFLICT: &str = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","reason":"Conflict","code":409,"message":"Precondition failed: UID in precondition"}"#;

    struct Replacement {
        current: String,
        attempts: usize,
        evicted: Vec<String>,
    }

    fn precondition(body: &Value) -> Option<String> {
        body.pointer("/deleteOptions/preconditions/uid")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    fn replaced_after(
        state: &std::sync::Mutex<Replacement>,
        replace_after: usize,
        method: &http::Method,
        path: &str,
        body: &Value,
    ) -> Option<crate::fake::Reply> {
        let mut state = crate::manager::lock(state);
        let node = if state.current == OLD_UID {
            NODE_A
        } else {
            NODE_B
        };
        let pod = pod_json(DB, &state.current, node);
        if method == http::Method::GET && path.starts_with("/api/v1/pods") {
            let list = format!(
                r#"{{"kind":"PodList","apiVersion":"v1","metadata":{{}},"items":[{pod}]}}"#
            );
            return Some((http::StatusCode::OK, list));
        }
        if method == http::Method::GET && path == format!("{PODS_PATH}{DB}") {
            return Some((http::StatusCode::OK, pod));
        }
        if method != http::Method::POST || path != format!("{PODS_PATH}{DB}/eviction") {
            return None;
        }
        state.attempts += 1;
        if precondition(body).is_some_and(|wanted| wanted != state.current) {
            return Some((http::StatusCode::CONFLICT, CONFLICT.to_owned()));
        }
        if state.attempts <= replace_after {
            if state.attempts == replace_after {
                NEW_UID.clone_into(&mut state.current);
            }
            return Some((http::StatusCode::TOO_MANY_REQUESTS, TOO_MANY.to_owned()));
        }
        let evicted = state.current.clone();
        state.evicted.push(evicted);
        Some((http::StatusCode::CREATED, EVICTED.to_owned()))
    }

    async fn drain_with_replacement(
        replace_after: usize,
    ) -> (Vec<Blocked>, Vec<String>, Vec<Option<String>>) {
        let state = std::sync::Arc::new(std::sync::Mutex::new(Replacement {
            current: OLD_UID.to_owned(),
            attempts: 0,
            evicted: Vec::new(),
        }));
        let shared = state.clone();
        let (client, traffic) =
            crate::fake::client_reading_bodies(ROUND_TRIP, move |method, path, body| {
                replaced_after(&shared, replace_after, method, path, body)
            });
        let (_manager, handle) = crate::fake::handle(&client);
        let Ok(blocked) = drain(&handle, NODE_A).await else {
            panic!("drain finishes");
        };
        let evicted = crate::manager::lock(&state).evicted.clone();
        let sent = traffic
            .bodies(&http::Method::POST, &format!("{PODS_PATH}{DB}/eviction"))
            .iter()
            .map(precondition)
            .collect();
        (blocked, evicted, sent)
    }

    #[tokio::test(start_paused = true)]
    async fn a_blocked_pod_replaced_on_another_node_is_never_evicted() {
        let (blocked, evicted, sent) = drain_with_replacement(1).await;
        assert_eq!(blocked, Vec::new());
        assert_eq!(evicted, Vec::<String>::new());
        assert_eq!(sent, [Some(OLD_UID.to_owned()), Some(OLD_UID.to_owned())]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_listed_pod_is_evicted_by_its_uid() {
        let (blocked, evicted, sent) = drain_with_replacement(0).await;
        assert_eq!(blocked, Vec::new());
        assert_eq!(evicted, [OLD_UID.to_owned()]);
        assert_eq!(sent, [Some(OLD_UID.to_owned())]);
    }

    #[test]
    fn blocked_evictions_are_listed() {
        let blocked = vec![Blocked {
            namespace: "payments".into(),
            name: "checkout-1".into(),
        }];
        assert_eq!(
            blocked_line(&blocked),
            "1 pod blocked by a disruption budget: payments/checkout-1"
        );
    }
}
