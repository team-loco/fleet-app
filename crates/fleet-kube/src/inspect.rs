use fleet_core::ResourceRef;
use fleet_core::resources::ResourceKind;
use fleet_flux::ObjectKind;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::manager::ClusterHandle;
use crate::ops::{ObjectRef, strip_managed_fields};

const SECRET_KIND: &str = "Secret";
const SECRET_FIELDS: [&str; 2] = ["data", "stringData"];
pub const SECRET_MASK: &str = "••••••";
const CORE_KINDS: [(&str, &str); 3] = [("Pod", "v1"), ("Node", "v1"), ("Namespace", "v1")];

fn resource_api_version(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Deployment => "apps/v1",
        ResourceKind::StatefulSet => "apps/v1",
        ResourceKind::DaemonSet => "apps/v1",
        ResourceKind::Job => "batch/v1",
        ResourceKind::CronJob => "batch/v1",
        ResourceKind::Service => "v1",
        ResourceKind::Ingress => "networking.k8s.io/v1",
        ResourceKind::ConfigMap => "v1",
        ResourceKind::Secret => "v1",
        ResourceKind::PersistentVolumeClaim => "v1",
    }
}

const FLUX_KINDS: [ObjectKind; 6] = [
    ObjectKind::Kustomization,
    ObjectKind::HelmRelease,
    ObjectKind::GitRepository,
    ObjectKind::HelmRepository,
    ObjectKind::HelmChart,
    ObjectKind::OCIRepository,
];

pub fn api_version(kind: &str) -> Option<&'static str> {
    let builtin = ResourceKind::ALL
        .into_iter()
        .find(|k| k.name() == kind)
        .map(resource_api_version);
    let core = || {
        CORE_KINDS
            .iter()
            .find(|(name, _)| *name == kind)
            .map(|(_, version)| *version)
    };
    let flux = || {
        FLUX_KINDS
            .into_iter()
            .find(|k| k.as_str() == kind)
            .map(ObjectKind::api_version)
    };
    builtin.or_else(core).or_else(flux)
}

pub fn object_ref(target: &ResourceRef) -> Option<ObjectRef> {
    Some(ObjectRef {
        api_version: api_version(&target.kind)?.to_owned(),
        kind: target.kind.clone(),
        namespace: target.namespace.clone(),
        name: target.name.clone(),
    })
}

pub fn mask_secret(object: &mut Value) {
    for field in SECRET_FIELDS {
        let Some(entries) = object.get_mut(field).and_then(Value::as_object_mut) else {
            continue;
        };
        for value in entries.values_mut() {
            *value = Value::from(SECRET_MASK);
        }
    }
}

pub fn render(object: &mut Value, kind: &str) -> Result<String> {
    strip_managed_fields(object);
    if kind == SECRET_KIND {
        mask_secret(object);
    }
    Ok(serde_norway::to_string(object)?)
}

pub async fn yaml(handle: &ClusterHandle, target: &ResourceRef) -> Result<String> {
    let reference = object_ref(target).ok_or_else(|| Error::UnknownKind {
        api_version: String::new(),
        kind: target.kind.clone(),
    })?;
    let object = handle.get(&reference).await?;
    let mut value = serde_json::to_value(&object)?;
    render(&mut value, &target.kind)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn secrets_render_without_their_values() {
        let mut secret = json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": { "name": "stripe", "managedFields": [{ "manager": "kubectl" }] },
            "type": "Opaque",
            "data": { "webhook-secret": "c2VjcmV0" }
        });
        let Ok(text) = render(&mut secret, "Secret") else {
            panic!("render");
        };
        assert!(text.contains("webhook-secret: ••••••"), "{text}");
        assert!(!text.contains("c2VjcmV0"));
        assert!(!text.contains("managedFields"));
    }

    #[test]
    fn kinds_resolve_their_api_versions() {
        assert_eq!(api_version("Deployment"), Some("apps/v1"));
        assert_eq!(api_version("Node"), Some("v1"));
        assert_eq!(
            api_version("HelmRelease"),
            Some("helm.toolkit.fluxcd.io/v2")
        );
        assert_eq!(api_version("Widget"), None);
    }
}
