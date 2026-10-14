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
    kind = "HelmChart",
    plural = "helmcharts"
)]
#[kube(namespaced)]
#[kube(status = "HelmChartStatus")]
#[kube(schema = "disabled")]
pub struct HelmChartSpec {
    pub chart: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "ignoreMissingValuesFiles"
    )]
    pub ignore_missing_values_files: Option<bool>,
    pub interval: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "reconcileStrategy"
    )]
    pub reconcile_strategy: Option<HelmChartReconcileStrategy>,
    #[serde(rename = "sourceRef")]
    pub source_ref: HelmChartSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "valuesFiles"
    )]
    pub values_files: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<HelmChartVerify>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmChartReconcileStrategy {
    ChartVersion,
    Revision,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmChartSourceRef {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "apiVersion"
    )]
    pub api_version: Option<String>,
    pub kind: HelmChartSourceRefKind,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmChartSourceRefKind {
    HelmRepository,
    GitRepository,
    Bucket,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmChartVerify {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "matchOIDCIdentity"
    )]
    pub match_oidc_identity: Option<Vec<HelmChartVerifyMatchOidcIdentity>>,
    pub provider: HelmChartVerifyProvider,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<HelmChartVerifySecretRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmChartVerifyMatchOidcIdentity {
    pub issuer: String,
    pub subject: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmChartVerifyProvider {
    #[serde(rename = "cosign")]
    Cosign,
    #[serde(rename = "notation")]
    Notation,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmChartVerifySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmChartStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<HelmChartStatusArtifact>,
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
        rename = "observedChartName"
    )]
    pub observed_chart_name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedGeneration"
    )]
    pub observed_generation: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedSourceArtifactRevision"
    )]
    pub observed_source_artifact_revision: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedValuesFiles"
    )]
    pub observed_values_files: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmChartStatusArtifact {
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
