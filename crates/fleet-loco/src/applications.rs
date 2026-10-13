use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use crate::generated::{
    ApplicationStatusPhase,
    applications::{
        Application, ApplicationServiceSpecDeploymentEnvSecretRef,
        ApplicationServiceSpecDeploymentHealthCheck, ApplicationServiceSpecResources,
    },
};
use crate::{
    APPLICATION_NAME, APPLICATION_NAMESPACE, CONTROLLER, Error, MANAGED_BY, ObjectRef,
    PLACEMENT_REVISION, RESOURCE_PREFIX, WORKSPACE_ID, WORKSPACE_PREFIX, annotation, label,
    owned_by, required,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlacementSync {
    AtDesiredRevision,
    OutOfSync,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub desired_revision: Option<i64>,
    pub applied_revision: Option<i64>,
    pub sync: PlacementSync,
}

pub fn placement(application: &Application) -> Result<Placement, Error> {
    let desired_revision = annotation(&application.metadata, PLACEMENT_REVISION)
        .map(|value| {
            value
                .parse::<i64>()
                .ok()
                .filter(|revision| *revision > 0)
                .ok_or_else(|| Error::InvalidRevision(value.to_owned()))
        })
        .transpose()?;
    let applied_revision = application
        .status
        .as_ref()
        .and_then(|status| status.observed_placement_revision)
        .filter(|revision| *revision > 0);
    let sync = match (desired_revision, applied_revision) {
        (Some(desired), Some(applied)) if desired == applied => PlacementSync::AtDesiredRevision,
        (Some(_), _) => PlacementSync::OutOfSync,
        _ => PlacementSync::Unknown,
    };
    Ok(Placement {
        desired_revision,
        applied_revision,
        sync,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationModel {
    pub application: ObjectRef,
    pub workspace_namespace: String,
    pub workload_name: String,
    pub phase: ApplicationStatusPhase,
    pub message: Option<String>,
    pub degraded_reason: Option<String>,
    pub degraded_message: Option<String>,
    pub image: Option<String>,
    pub port: Option<i32>,
    pub resources: Option<ApplicationServiceSpecResources>,
    pub health_check: Option<ApplicationServiceSpecDeploymentHealthCheck>,
    pub staging_env_secret: Option<ApplicationServiceSpecDeploymentEnvSecretRef>,
    pub deployment_id: Option<String>,
    pub environment: Option<String>,
    pub ready_replicas: Option<i32>,
    pub desired_replicas: Option<i32>,
    pub hostname: Option<String>,
    pub path_prefix: Option<String>,
    pub placement: Placement,
}

pub fn application_model(application: &Application) -> Result<ApplicationModel, Error> {
    let workspace_id = required(application.spec.workspace_id.as_deref(), "spec.workspaceId")?;
    let resource_id = required(application.spec.resource_id.as_deref(), "spec.resourceId")?;
    let reference = ObjectRef::from_metadata("Application", &application.metadata)?;
    let status = application.status.as_ref();
    let degraded = status
        .and_then(|status| status.conditions.as_ref())
        .and_then(|conditions| {
            conditions
                .iter()
                .find(|condition| condition.type_ == "Degraded" && condition.status == "True")
        });
    let service = application.spec.service_spec.as_ref();
    let routing = service.and_then(|service| service.routing.as_ref());
    let placement = placement(application)?;
    Ok(ApplicationModel {
        application: reference,
        workspace_namespace: format!("{WORKSPACE_PREFIX}{workspace_id}"),
        workload_name: format!("{RESOURCE_PREFIX}{resource_id}"),
        phase: status
            .and_then(|status| status.phase.clone())
            .unwrap_or(ApplicationStatusPhase::Unknown),
        message: status.and_then(|status| status.message.clone()),
        degraded_reason: degraded.map(|condition| condition.reason.clone()),
        degraded_message: degraded.map(|condition| condition.message.clone()),
        image: service.map(|service| service.deployment.image.clone()),
        port: service.map(|service| service.deployment.port),
        resources: service.map(|service| service.resources.clone()),
        health_check: service.and_then(|service| service.deployment.health_check.clone()),
        staging_env_secret: service.and_then(|service| service.deployment.env_secret_ref.clone()),
        deployment_id: application.spec.deployment_id.clone(),
        environment: application.spec.environment_name.clone(),
        ready_replicas: status.and_then(|status| status.ready_replicas),
        desired_replicas: status.and_then(|status| status.desired_replicas),
        hostname: routing.map(|routing| routing.host_name.clone()),
        path_prefix: routing.map(|routing| routing.path_prefix.clone()),
        placement,
    })
}

pub fn group_applications(
    applications: &[Application],
) -> Result<BTreeMap<String, Vec<ApplicationModel>>, Error> {
    let mut groups: BTreeMap<String, Vec<ApplicationModel>> = BTreeMap::new();
    for application in applications {
        let model = application_model(application)?;
        groups
            .entry(model.workspace_namespace.clone())
            .or_default()
            .push(model);
    }
    for applications in groups.values_mut() {
        applications.sort_by(|left, right| left.application.cmp(&right.application));
    }
    Ok(groups)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectRelationship {
    Application,
    SharedWorkspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedObject {
    pub reference: ObjectRef,
    pub relationship: ObjectRelationship,
}

pub fn related_object(
    application: &Application,
    kind: &str,
    metadata: &ObjectMeta,
) -> Result<Option<RelatedObject>, Error> {
    let model = application_model(application)?;
    let in_workspace = metadata.namespace.as_deref() == Some(model.workspace_namespace.as_str());
    let controller_managed = label(metadata, MANAGED_BY) == Some(CONTROLLER);
    let annotation_owner = annotation(metadata, APPLICATION_NAME)
        == Some(model.application.name.as_str())
        && annotation(metadata, APPLICATION_NAMESPACE)
            == Some(model.application.namespace.as_str());
    let app_label = label(metadata, "app") == Some(model.workload_name.as_str());
    let direct_owner = owned_by(metadata, &application.metadata, "Application");
    let relationship =
        if direct_owner || (in_workspace && controller_managed && annotation_owner && app_label) {
            ObjectRelationship::Application
        } else if in_workspace
            && controller_managed
            && kind == "NetworkPolicy"
            && label(metadata, WORKSPACE_ID) == application.spec.workspace_id.as_deref()
            && !annotation_owner
        {
            ObjectRelationship::SharedWorkspace
        } else {
            return Ok(None);
        };
    let reference = ObjectRef::from_metadata(kind, metadata)?;
    Ok(Some(RelatedObject {
        reference,
        relationship,
    }))
}
