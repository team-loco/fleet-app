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
    kind = "OCIRepository",
    root = "OciRepository",
    plural = "ocirepositories"
)]
#[kube(namespaced)]
#[kube(status = "OciRepositoryStatus")]
#[kube(schema = "disabled")]
pub struct OciRepositorySpec {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "certSecretRef"
    )]
    pub cert_secret_ref: Option<OciRepositoryCertSecretRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insecure: Option<bool>,
    pub interval: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "layerSelector"
    )]
    pub layer_selector: Option<OciRepositoryLayerSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<OciRepositoryProvider>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "proxySecretRef"
    )]
    pub proxy_secret_ref: Option<OciRepositoryProxySecretRef>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ref")]
    pub r#ref: Option<OciRepositoryRef>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<OciRepositorySecretRef>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "serviceAccountName"
    )]
    pub service_account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<OciRepositoryVerify>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryCertSecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryLayerSelector {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "mediaType")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OciRepositoryLayerSelectorOperation>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum OciRepositoryLayerSelectorOperation {
    #[serde(rename = "extract")]
    Extract,
    #[serde(rename = "copy")]
    Copy,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum OciRepositoryProvider {
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
pub struct OciRepositoryProxySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semver: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "semverFilter"
    )]
    pub semver_filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositorySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryVerify {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "matchOIDCIdentity"
    )]
    pub match_oidc_identity: Option<Vec<OciRepositoryVerifyMatchOidcIdentity>>,
    pub provider: OciRepositoryVerifyProvider,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<OciRepositoryVerifySecretRef>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "trustedRootSecretRef"
    )]
    pub trusted_root_secret_ref: Option<OciRepositoryVerifyTrustedRootSecretRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryVerifyMatchOidcIdentity {
    pub issuer: String,
    pub subject: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum OciRepositoryVerifyProvider {
    #[serde(rename = "cosign")]
    Cosign,
    #[serde(rename = "notation")]
    Notation,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryVerifySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryVerifyTrustedRootSecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<OciRepositoryStatusArtifact>,
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedIgnore"
    )]
    pub observed_ignore: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedLayerSelector"
    )]
    pub observed_layer_selector: Option<OciRepositoryStatusObservedLayerSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryStatusArtifact {
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OciRepositoryStatusObservedLayerSelector {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "mediaType")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OciRepositoryStatusObservedLayerSelectorOperation>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum OciRepositoryStatusObservedLayerSelectorOperation {
    #[serde(rename = "extract")]
    Extract,
    #[serde(rename = "copy")]
    Copy,
}
