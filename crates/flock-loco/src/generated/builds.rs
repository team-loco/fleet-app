use super::BuildStatusPhase;

#[allow(unused_imports)]
mod prelude {
    pub use kube::CustomResource;
    pub use serde::{Deserialize, Serialize};
}

use self::prelude::*;

#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[kube(
    group = "infra.loco.io",
    version = "v1alpha1",
    kind = "Build",
    plural = "builds"
)]
#[kube(namespaced)]
#[kube(status = "BuildStatus")]
#[kube(schema = "disabled")]
#[kube(derive = "PartialEq")]
pub struct BuildSpec {
    #[serde(rename = "buildId")]
    pub build_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "cacheRef")]
    pub cache_ref: Option<String>,
    pub context: String,
    #[serde(rename = "dockerfilePath")]
    pub dockerfile_path: String,
    #[serde(rename = "imageRepository")]
    pub image_repository: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "organizationId"
    )]
    pub organization_id: Option<String>,
    #[serde(rename = "resourceId")]
    pub resource_id: String,
    #[serde(rename = "sourceURL")]
    pub source_url: String,
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BuildStatus {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "cacheDigest"
    )]
    pub cache_digest: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "finishedAt"
    )]
    pub finished_at: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "imageDigest"
    )]
    pub image_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<BuildStatusPhase>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "startedAt")]
    pub started_at: Option<String>,
}
