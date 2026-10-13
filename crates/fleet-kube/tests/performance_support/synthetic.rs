use std::sync::Arc;

use k8s_openapi::api::core::v1::{ConfigMap, Pod};
use kube::api::PartialObjectMeta;
use serde_json::json;

const POD_COUNT: usize = 10_000;
const CONFIGMAP_COUNT: usize = 5_000;
const OBJECT_STRIDE: usize = 7919;

fn fixture<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    let Ok(object) = serde_json::from_value(value) else {
        panic!("invalid synthetic fixture");
    };
    object
}

pub fn pods() -> Vec<Arc<Pod>> {
    (0..POD_COUNT)
        .map(|index| {
            let shuffled = (index * OBJECT_STRIDE) % POD_COUNT;
            let object = fixture(json!({
                "metadata": {"name": format!("pod-{shuffled:05}"), "namespace": "scale",
                    "labels": {"app": "scale"}},
                "spec": {"nodeName": "node", "containers": [{"name": "pause", "image": "pause",
                    "resources": {"requests": {"cpu": "1m", "memory": "1Mi"}}}]},
                "status": {"phase": "Running", "conditions": [{"type": "Ready", "status": "True"}],
                    "containerStatuses": [{"name": "pause", "image": "pause", "imageID": "pause",
                        "ready": true, "restartCount": 0, "state": {"running": {}}}]}
            }));
            Arc::new(object)
        })
        .collect()
}

pub fn configmaps() -> Vec<PartialObjectMeta<ConfigMap>> {
    (0..CONFIGMAP_COUNT)
        .map(|index| {
            let shuffled = (index * OBJECT_STRIDE) % CONFIGMAP_COUNT;
            fixture(
                json!({"metadata": {"name": format!("map-{shuffled:05}"), "namespace": "scale"}}),
            )
        })
        .collect()
}
