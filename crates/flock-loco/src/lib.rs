pub mod applications;
pub mod builds;
pub mod generated;
pub mod platform;
pub mod routes;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use thiserror::Error;

pub const API_VERSION: &str = "infra.loco.io/v1alpha1";
pub const BUILD_NAMESPACE: &str = "loco-builds";
pub const MANAGED_BY: &str = "app.kubernetes.io/managed-by";
pub const CONTROLLER: &str = "loco-controller";
pub const WORKSPACE_ID: &str = "loco.io/workspace-id";
pub const RESOURCE_ID: &str = "loco.io/resource-id";
pub const BUILD_ID: &str = "loco.io/build-id";
pub const PLACEMENT_REVISION: &str = "loco.io/placement-revision";
pub const APPLICATION_NAME: &str = "loco.io/application-name";
pub const APPLICATION_NAMESPACE: &str = "loco.io/application-namespace";
pub const WORKSPACE_PREFIX: &str = "ws-";
pub const RESOURCE_PREFIX: &str = "resource-";

#[derive(Debug, Error)]
pub enum Error {
    #[error("missing {0}")]
    MissingField(&'static str),
    #[error("invalid placement revision: {0}")]
    InvalidRevision(String),
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(String),
    #[error("build finished before it started")]
    InvalidDuration,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectRef {
    pub kind: String,
    pub namespace: String,
    pub name: String,
}

impl ObjectRef {
    pub fn from_metadata(kind: &str, metadata: &ObjectMeta) -> Result<Self, Error> {
        let name = required(metadata.name.as_deref(), "metadata.name")?;
        let namespace = required(metadata.namespace.as_deref(), "metadata.namespace")?;
        Ok(Self {
            kind: kind.to_owned(),
            namespace: namespace.to_owned(),
            name: name.to_owned(),
        })
    }
}

pub(crate) fn required<'a>(value: Option<&'a str>, field: &'static str) -> Result<&'a str, Error> {
    value
        .filter(|value| !value.is_empty())
        .ok_or(Error::MissingField(field))
}

pub(crate) fn label<'a>(metadata: &'a ObjectMeta, key: &str) -> Option<&'a str> {
    metadata.labels.as_ref()?.get(key).map(String::as_str)
}

pub(crate) fn annotation<'a>(metadata: &'a ObjectMeta, key: &str) -> Option<&'a str> {
    metadata.annotations.as_ref()?.get(key).map(String::as_str)
}

pub(crate) fn owned_by(metadata: &ObjectMeta, owner: &ObjectMeta, kind: &str) -> bool {
    let Some(uid) = owner.uid.as_deref().filter(|uid| !uid.is_empty()) else {
        return false;
    };
    metadata.namespace == owner.namespace
        && metadata.owner_references.as_ref().is_some_and(|owners| {
            owners.iter().any(|reference| {
                reference.uid == uid
                    && reference.kind == kind
                    && reference.api_version == API_VERSION
            })
        })
}
