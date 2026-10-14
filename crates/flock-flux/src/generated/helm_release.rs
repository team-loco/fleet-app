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
    group = "helm.toolkit.fluxcd.io",
    version = "v2",
    kind = "HelmRelease",
    plural = "helmreleases"
)]
#[kube(namespaced)]
#[kube(status = "HelmReleaseStatus")]
#[kube(schema = "disabled")]
pub struct HelmReleaseSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<HelmReleaseChart>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "chartRef")]
    pub chart_ref: Option<HelmReleaseChartRef>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "commonMetadata"
    )]
    pub common_metadata: Option<HelmReleaseCommonMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "dependsOn")]
    pub depends_on: Option<Vec<HelmReleaseDependsOn>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "driftDetection"
    )]
    pub drift_detection: Option<HelmReleaseDriftDetection>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "healthCheckExprs"
    )]
    pub health_check_exprs: Option<Vec<HelmReleaseHealthCheckExprs>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<HelmReleaseInstall>,
    pub interval: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "kubeConfig"
    )]
    pub kube_config: Option<HelmReleaseKubeConfig>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "maxHistory"
    )]
    pub max_history: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "persistentClient"
    )]
    pub persistent_client: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "postRenderStrategy"
    )]
    pub post_render_strategy: Option<HelmReleasePostRenderStrategy>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "postRenderers"
    )]
    pub post_renderers: Option<Vec<HelmReleasePostRenderers>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "releaseName"
    )]
    pub release_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<HelmReleaseRollback>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "serviceAccountName"
    )]
    pub service_account_name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "storageNamespace"
    )]
    pub storage_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "targetNamespace"
    )]
    pub target_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<HelmReleaseTest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uninstall: Option<HelmReleaseUninstall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<HelmReleaseUpgrade>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<serde_json::Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "valuesFrom"
    )]
    pub values_from: Option<Vec<HelmReleaseValuesFrom>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "waitStrategy"
    )]
    pub wait_strategy: Option<HelmReleaseWaitStrategy>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseChart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HelmReleaseChartMetadata>,
    pub spec: HelmReleaseChartSpec,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseChartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseChartSpec {
    pub chart: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "ignoreMissingValuesFiles"
    )]
    pub ignore_missing_values_files: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "reconcileStrategy"
    )]
    pub reconcile_strategy: Option<HelmReleaseChartSpecReconcileStrategy>,
    #[serde(rename = "sourceRef")]
    pub source_ref: HelmReleaseChartSpecSourceRef,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "valuesFiles"
    )]
    pub values_files: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<HelmReleaseChartSpecVerify>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseChartSpecReconcileStrategy {
    ChartVersion,
    Revision,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseChartSpecSourceRef {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "apiVersion"
    )]
    pub api_version: Option<String>,
    pub kind: HelmReleaseChartSpecSourceRefKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseChartSpecSourceRefKind {
    HelmRepository,
    GitRepository,
    Bucket,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseChartSpecVerify {
    pub provider: HelmReleaseChartSpecVerifyProvider,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<HelmReleaseChartSpecVerifySecretRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseChartSpecVerifyProvider {
    #[serde(rename = "cosign")]
    Cosign,
    #[serde(rename = "notation")]
    Notation,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseChartSpecVerifySecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseChartRef {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "apiVersion"
    )]
    pub api_version: Option<String>,
    pub kind: HelmReleaseChartRefKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseChartRefKind {
    #[serde(rename = "OCIRepository")]
    OciRepository,
    HelmChart,
    ExternalArtifact,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseCommonMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseDependsOn {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "readyExpr")]
    pub ready_expr: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseDriftDetection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<HelmReleaseDriftDetectionIgnore>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<HelmReleaseDriftDetectionMode>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseDriftDetectionIgnore {
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<HelmReleaseDriftDetectionIgnoreTarget>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseDriftDetectionIgnoreTarget {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "annotationSelector"
    )]
    pub annotation_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "labelSelector"
    )]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseDriftDetectionMode {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseHealthCheckExprs {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub current: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "inProgress"
    )]
    pub in_progress: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseInstall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crds: Option<HelmReleaseInstallCrds>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "createNamespace"
    )]
    pub create_namespace: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableHooks"
    )]
    pub disable_hooks: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableOpenAPIValidation"
    )]
    pub disable_open_api_validation: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableSchemaValidation"
    )]
    pub disable_schema_validation: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableTakeOwnership"
    )]
    pub disable_take_ownership: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableWait"
    )]
    pub disable_wait: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableWaitForJobs"
    )]
    pub disable_wait_for_jobs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<HelmReleaseInstallRemediation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "serverSideApply"
    )]
    pub server_side_apply: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "skipCRDs")]
    pub skip_cr_ds: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<HelmReleaseInstallStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseInstallCrds {
    Skip,
    Create,
    CreateReplace,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseInstallRemediation {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "ignoreTestFailures"
    )]
    pub ignore_test_failures: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "remediateLastFailure"
    )]
    pub remediate_last_failure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseInstallStrategy {
    pub name: HelmReleaseInstallStrategyName,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "retryInterval"
    )]
    pub retry_interval: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseInstallStrategyName {
    RemediateOnFailure,
    RetryOnFailure,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseKubeConfig {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "configMapRef"
    )]
    pub config_map_ref: Option<HelmReleaseKubeConfigConfigMapRef>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<HelmReleaseKubeConfigSecretRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseKubeConfigConfigMapRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseKubeConfigSecretRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleasePostRenderStrategy {
    #[serde(rename = "nohooks")]
    Nohooks,
    #[serde(rename = "combined")]
    Combined,
    #[serde(rename = "separate")]
    Separate,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleasePostRenderers {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kustomize: Option<HelmReleasePostRenderersKustomize>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleasePostRenderersKustomize {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<HelmReleasePostRenderersKustomizeImages>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patches: Option<Vec<HelmReleasePostRenderersKustomizePatches>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleasePostRenderersKustomizeImages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "newName")]
    pub new_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "newTag")]
    pub new_tag: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleasePostRenderersKustomizePatches {
    pub patch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<HelmReleasePostRenderersKustomizePatchesTarget>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleasePostRenderersKustomizePatchesTarget {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "annotationSelector"
    )]
    pub annotation_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "labelSelector"
    )]
    pub label_selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseRollback {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "cleanupOnFail"
    )]
    pub cleanup_on_fail: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableHooks"
    )]
    pub disable_hooks: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableWait"
    )]
    pub disable_wait: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableWaitForJobs"
    )]
    pub disable_wait_for_jobs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recreate: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "serverSideApply"
    )]
    pub server_side_apply: Option<HelmReleaseRollbackServerSideApply>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseRollbackServerSideApply {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "disabled")]
    Disabled,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseTest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<HelmReleaseTestFilters>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "ignoreFailures"
    )]
    pub ignore_failures: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseTestFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseUninstall {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "deletionPropagation"
    )]
    pub deletion_propagation: Option<HelmReleaseUninstallDeletionPropagation>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableHooks"
    )]
    pub disable_hooks: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableWait"
    )]
    pub disable_wait: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "keepHistory"
    )]
    pub keep_history: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseUninstallDeletionPropagation {
    #[serde(rename = "background")]
    Background,
    #[serde(rename = "foreground")]
    Foreground,
    #[serde(rename = "orphan")]
    Orphan,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseUpgrade {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "chartNameChangeStrategy"
    )]
    pub chart_name_change_strategy: Option<HelmReleaseUpgradeChartNameChangeStrategy>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "cleanupOnFail"
    )]
    pub cleanup_on_fail: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crds: Option<HelmReleaseUpgradeCrds>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableHooks"
    )]
    pub disable_hooks: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableOpenAPIValidation"
    )]
    pub disable_open_api_validation: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableSchemaValidation"
    )]
    pub disable_schema_validation: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableTakeOwnership"
    )]
    pub disable_take_ownership: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableWait"
    )]
    pub disable_wait: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "disableWaitForJobs"
    )]
    pub disable_wait_for_jobs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "preserveValues"
    )]
    pub preserve_values: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<HelmReleaseUpgradeRemediation>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "serverSideApply"
    )]
    pub server_side_apply: Option<HelmReleaseUpgradeServerSideApply>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<HelmReleaseUpgradeStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseUpgradeChartNameChangeStrategy {
    InPlaceUpdate,
    Reinstall,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseUpgradeCrds {
    Skip,
    Create,
    CreateReplace,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseUpgradeRemediation {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "ignoreTestFailures"
    )]
    pub ignore_test_failures: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "remediateLastFailure"
    )]
    pub remediate_last_failure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retries: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<HelmReleaseUpgradeRemediationStrategy>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseUpgradeRemediationStrategy {
    #[serde(rename = "rollback")]
    Rollback,
    #[serde(rename = "uninstall")]
    Uninstall,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseUpgradeServerSideApply {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "disabled")]
    Disabled,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseUpgradeStrategy {
    pub name: HelmReleaseUpgradeStrategyName,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "retryInterval"
    )]
    pub retry_interval: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseUpgradeStrategyName {
    RemediateOnFailure,
    RetryOnFailure,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseValuesFrom {
    pub kind: HelmReleaseValuesFromKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<bool>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "targetPath"
    )]
    pub target_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "valuesKey")]
    pub values_key: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseValuesFromKind {
    Secret,
    ConfigMap,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseWaitStrategy {
    pub name: HelmReleaseWaitStrategyName,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseWaitStrategyName {
    #[serde(rename = "poller")]
    Poller,
    #[serde(rename = "legacy")]
    Legacy,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failures: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "helmChart")]
    pub helm_chart: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<HelmReleaseStatusHistory>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "installFailures"
    )]
    pub install_failures: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<HelmReleaseStatusInventory>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAttemptedConfigDigest"
    )]
    pub last_attempted_config_digest: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAttemptedGeneration"
    )]
    pub last_attempted_generation: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAttemptedReleaseAction"
    )]
    pub last_attempted_release_action: Option<HelmReleaseStatusLastAttemptedReleaseAction>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAttemptedReleaseActionDuration"
    )]
    pub last_attempted_release_action_duration: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAttemptedRevision"
    )]
    pub last_attempted_revision: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAttemptedRevisionDigest"
    )]
    pub last_attempted_revision_digest: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAttemptedValuesChecksum"
    )]
    pub last_attempted_values_checksum: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastHandledForceAt"
    )]
    pub last_handled_force_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastHandledReconcileAt"
    )]
    pub last_handled_reconcile_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastHandledResetAt"
    )]
    pub last_handled_reset_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastReleaseRevision"
    )]
    pub last_release_revision: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedCommonMetadataDigest"
    )]
    pub observed_common_metadata_digest: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedGeneration"
    )]
    pub observed_generation: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "observedPostRenderersDigest"
    )]
    pub observed_post_renderers_digest: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "storageNamespace"
    )]
    pub storage_namespace: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "upgradeFailures"
    )]
    pub upgrade_failures: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseStatusHistory {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "apiVersion"
    )]
    pub api_version: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "appVersion"
    )]
    pub app_version: Option<String>,
    #[serde(rename = "chartName")]
    pub chart_name: String,
    #[serde(rename = "chartVersion")]
    pub chart_version: String,
    #[serde(rename = "configDigest")]
    pub config_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<String>,
    pub digest: String,
    #[serde(rename = "firstDeployed")]
    pub first_deployed: String,
    #[serde(rename = "lastDeployed")]
    pub last_deployed: String,
    pub name: String,
    pub namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ociDigest")]
    pub oci_digest: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "testHooks")]
    pub test_hooks: Option<BTreeMap<String, HelmReleaseStatusHistoryTestHooks>>,
    pub version: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseStatusHistoryTestHooks {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastCompleted"
    )]
    pub last_completed: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastStarted"
    )]
    pub last_started: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseStatusInventory {
    pub entries: Vec<HelmReleaseStatusInventoryEntries>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HelmReleaseStatusInventoryEntries {
    pub id: String,
    pub v: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum HelmReleaseStatusLastAttemptedReleaseAction {
    #[serde(rename = "install")]
    Install,
    #[serde(rename = "upgrade")]
    Upgrade,
}
