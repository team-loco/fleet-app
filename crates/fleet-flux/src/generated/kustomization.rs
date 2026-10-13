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
    group = "kustomize.toolkit.fluxcd.io",
    version = "v1",
    kind = "Kustomization",
    plural = "kustomizations"
)]
#[kube(namespaced)]
#[kube(status = "KustomizationStatus")]
#[kube(schema = "disabled")]
pub struct KustomizationSpec {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "buildMetadata"
    )]
    pub build_metadata: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "commonMetadata"
    )]
    pub common_metadata: Option<KustomizationCommonMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decryption: Option<KustomizationDecryption>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "deletionPolicy"
    )]
    pub deletion_policy: Option<KustomizationDeletionPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "dependsOn")]
    pub depends_on: Option<Vec<KustomizationDependsOn>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "healthCheckExprs"
    )]
    pub health_check_exprs: Option<Vec<KustomizationHealthCheckExprs>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "healthChecks"
    )]
    pub health_checks: Option<Vec<KustomizationHealthChecks>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<KustomizationIgnore>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "ignoreMissingComponents"
    )]
    pub ignore_missing_components: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<KustomizationImages>>,
    pub interval: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "kubeConfig"
    )]
    pub kube_config: Option<KustomizationKubeConfig>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "namePrefix"
    )]
    pub name_prefix: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "nameSuffix"
    )]
    pub name_suffix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patches: Option<Vec<KustomizationPatches>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "postBuild")]
    pub post_build: Option<KustomizationPostBuild>,
    pub prune: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "retryInterval"
    )]
    pub retry_interval: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "serviceAccountName"
    )]
    pub service_account_name: Option<String>,
    #[serde(rename = "sourceRef")]
    pub source_ref: KustomizationSourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "targetNamespace"
    )]
    pub target_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationCommonMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationDecryption {
    pub provider: KustomizationDecryptionProvider,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<KustomizationDecryptionSecretRef>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "serviceAccountName"
    )]
    pub service_account_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KustomizationDecryptionProvider {
    #[serde(rename = "sops")]
    Sops,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationDecryptionSecretRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KustomizationDeletionPolicy {
    MirrorPrune,
    Delete,
    WaitForTermination,
    Orphan,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationDependsOn {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "readyExpr")]
    pub ready_expr: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationHealthCheckExprs {
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
pub struct KustomizationHealthChecks {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "apiVersion"
    )]
    pub api_version: Option<String>,
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationIgnore {
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<KustomizationIgnoreTarget>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationIgnoreTarget {
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
pub struct KustomizationImages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "newName")]
    pub new_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "newTag")]
    pub new_tag: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationKubeConfig {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "configMapRef"
    )]
    pub config_map_ref: Option<KustomizationKubeConfigConfigMapRef>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "secretRef")]
    pub secret_ref: Option<KustomizationKubeConfigSecretRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationKubeConfigConfigMapRef {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationKubeConfigSecretRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationPatches {
    pub patch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<KustomizationPatchesTarget>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationPatchesTarget {
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
pub struct KustomizationPostBuild {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substitute: Option<BTreeMap<String, String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "substituteFrom"
    )]
    pub substitute_from: Option<Vec<KustomizationPostBuildSubstituteFrom>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "substituteStrategy"
    )]
    pub substitute_strategy: Option<KustomizationPostBuildSubstituteStrategy>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationPostBuildSubstituteFrom {
    pub kind: KustomizationPostBuildSubstituteFromKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KustomizationPostBuildSubstituteFromKind {
    Secret,
    ConfigMap,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KustomizationPostBuildSubstituteStrategy {
    WithVariables,
    Always,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationSourceRef {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "apiVersion"
    )]
    pub api_version: Option<String>,
    pub kind: KustomizationSourceRefKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KustomizationSourceRefKind {
    #[serde(rename = "OCIRepository")]
    OciRepository,
    GitRepository,
    Bucket,
    ExternalArtifact,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<KustomizationStatusHistory>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<KustomizationStatusInventory>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAppliedOriginRevision"
    )]
    pub last_applied_origin_revision: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAppliedRevision"
    )]
    pub last_applied_revision: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "lastAttemptedRevision"
    )]
    pub last_attempted_revision: Option<String>,
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
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationStatusHistory {
    pub digest: String,
    #[serde(rename = "firstReconciled")]
    pub first_reconciled: String,
    #[serde(rename = "lastReconciled")]
    pub last_reconciled: String,
    #[serde(rename = "lastReconciledDuration")]
    pub last_reconciled_duration: String,
    #[serde(rename = "lastReconciledStatus")]
    pub last_reconciled_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
    #[serde(rename = "totalReconciliations")]
    pub total_reconciliations: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationStatusInventory {
    pub entries: Vec<KustomizationStatusInventoryEntries>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KustomizationStatusInventoryEntries {
    pub id: String,
    pub v: String,
}
