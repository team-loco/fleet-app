#![allow(clippy::all, clippy::pedantic, unused_imports)]

#[allow(unused_imports)]
mod prelude {
    pub use k8s_openapi::apimachinery::pkg::apis::meta::v1::Condition;
    pub use kube::CustomResource;
    pub use serde::{Deserialize, Serialize};
    pub use std::collections::BTreeMap;
}

use self::prelude::*;

#[derive(CustomResource, Serialize, Deserialize, Clone, Debug)]
#[kube(
    group = "source.toolkit.fluxcd.io",
    version = "v1",
    kind = "HelmRepository",
    plural = "helmrepositories"
)]
#[kube(namespaced)]
#[kube(status = "HelmRepositoryStatus")]
#[kube(schema = "disabled")]
pub struct HelmRepositorySpec {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "accessFrom"
    )]
    pub access_from: Option<HelmRepositoryAccessFrom>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "certSecretRef"
    )]
    pub cert_secret_ref: Option<HelmRepositoryCertSecretRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insecure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "passCredentials"
    )]
    pub pass_credentials: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<HelmRepositoryProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<HelmRepositorySecretRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub r#type: Option<HelmRepositoryType>,
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmRepositoryAccessFrom {
    #[serde(rename = "namespaceSelectors")]
    pub namespace_selectors: Vec<HelmRepositoryAccessFromNamespaceSelectors>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmRepositoryAccessFromNamespaceSelectors {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "matchLabels"
    )]
    pub match_labels: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmRepositoryCertSecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmRepositoryProvider {
    #[serde(rename = "generic")]
    Generic,
    #[serde(rename = "aws")]
    Aws,
    #[serde(rename = "azure")]
    Azure,
    #[serde(rename = "gcp")]
    Gcp,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmRepositorySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmRepositoryType {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "oci")]
    Oci,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmRepositoryStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<HelmRepositoryStatusArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastHandledReconcileAt"
    )]
    pub last_handled_reconcile_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedGeneration"
    )]
    pub observed_generation: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmRepositoryStatusArtifact {
    pub digest: String,
    #[serde(rename = "lastUpdateTime")]
    pub last_update_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
    pub path: String,
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    pub url: String,
}
