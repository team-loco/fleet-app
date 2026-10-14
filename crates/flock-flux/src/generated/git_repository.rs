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
    kind = "GitRepository",
    plural = "gitrepositories"
)]
#[kube(namespaced)]
#[kube(status = "GitRepositoryStatus")]
#[kube(schema = "disabled")]
pub struct GitRepositorySpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<GitRepositoryInclude>>,
    pub interval: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<GitRepositoryProvider>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "proxySecretRef"
    )]
    pub proxy_secret_ref: Option<GitRepositoryProxySecretRef>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "recurseSubmodules"
    )]
    pub recurse_submodules: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ref")]
    pub r#ref: Option<GitRepositoryRef>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<GitRepositorySecretRef>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "serviceAccountName"
    )]
    pub service_account_name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "sparseCheckout"
    )]
    pub sparse_checkout: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<GitRepositoryVerify>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryInclude {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "fromPath")]
    pub from_path: Option<String>,
    pub repository: GitRepositoryIncludeRepository,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "toPath")]
    pub to_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryIncludeRepository {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GitRepositoryProvider {
    #[serde(rename = "generic")]
    Generic,
    #[serde(rename = "aws")]
    Aws,
    #[serde(rename = "azure")]
    Azure,
    #[serde(rename = "github")]
    Github,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryProxySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositorySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryVerify {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<GitRepositoryVerifyMode>,
    #[serde(rename = "secretRef")]
    pub secret_ref: GitRepositoryVerifySecretRef,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GitRepositoryVerifyMode {
    #[serde(rename = "head")]
    Head,
    #[serde(rename = "HEAD")]
    HeadX,
    Tag,
    #[serde(rename = "TagAndHEAD")]
    TagAndHead,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryVerifySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<GitRepositoryStatusArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "includedArtifacts"
    )]
    pub included_artifacts: Option<Vec<GitRepositoryStatusIncludedArtifacts>>,
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
        rename = "observedInclude"
    )]
    pub observed_include: Option<Vec<GitRepositoryStatusObservedInclude>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedRecurseSubmodules"
    )]
    pub observed_recurse_submodules: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedSparseCheckout"
    )]
    pub observed_sparse_checkout: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "sourceVerificationMode"
    )]
    pub source_verification_mode: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryStatusArtifact {
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
pub struct GitRepositoryStatusIncludedArtifacts {
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
pub struct GitRepositoryStatusObservedInclude {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "fromPath")]
    pub from_path: Option<String>,
    pub repository: GitRepositoryStatusObservedIncludeRepository,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "toPath")]
    pub to_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitRepositoryStatusObservedIncludeRepository {
    pub name: String,
}
