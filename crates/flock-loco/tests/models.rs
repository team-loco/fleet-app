#![allow(clippy::unwrap_used)]

use chrono::{DateTime, Utc};
use flock_loco::{
    Error, ObjectRef,
    applications::*,
    builds::*,
    generated::{
        ApplicationStatusPhase, BuildStatusPhase, applications::Application, builds::Build,
    },
    platform::*,
    routes::*,
};
use k8s_openapi::{
    api::{
        apps::v1::{DaemonSet, Deployment, StatefulSet},
        batch::v1::Job,
        core::v1::{Pod, Service},
        discovery::v1::EndpointSlice,
    },
    apimachinery::pkg::apis::meta::v1::{APIResourceList, ObjectMeta},
};
use serde::Deserialize;
use serde_json::json;

fn application() -> Application {
    serde_json::from_str(include_str!("fixtures/application.json")).unwrap()
}

fn build() -> Build {
    serde_json::from_str(include_str!("fixtures/build.json")).unwrap()
}

fn route() -> HttpRoute {
    serde_json::from_str(include_str!("fixtures/routes.json")).unwrap()
}

fn now() -> DateTime<Utc> {
    "2026-10-12T21:03:00Z".parse().unwrap()
}

#[test]
fn application_preserves_controller_message_and_real_names() {
    let model = application_model(&application()).unwrap();
    assert_eq!(model.workspace_namespace, "ws-workspace");
    assert_eq!(model.workload_name, "resource-app");
    assert_eq!(model.phase, ApplicationStatusPhase::Failed);
    assert_eq!(
        model.degraded_reason.as_deref(),
        Some("CreateContainerConfigError")
    );
    assert_eq!(
        model.degraded_message.as_deref(),
        Some(
            "The image runs as root. Loco runs containers as a non-root user; use an image that sets a non-root USER."
        )
    );
    assert_eq!(model.image.as_deref(), Some("registry/app@sha256:abc"));
    assert_eq!(model.path_prefix.as_deref(), Some("/"));
    assert_eq!(model.port, Some(8080));
    assert_eq!(model.resources.unwrap().cpu, "500m");
    assert_eq!(model.staging_env_secret.unwrap().name, "env-placement");
}

#[test]
fn grouping_uses_spec_workspace_instead_of_cr_namespace() {
    let first = application();
    let mut second = first.clone();
    second.spec.workspace_id = Some("second".into());
    let groups = group_applications(&[first, second]).unwrap();
    assert_eq!(
        groups.keys().map(String::as_str).collect::<Vec<_>>(),
        ["ws-second", "ws-workspace"]
    );
    assert_eq!(group_applications(&[]).unwrap().len(), 0);
}

#[test]
fn placement_handles_drift_missing_observation_and_invalid_annotations() {
    let mut app = application();
    assert_eq!(placement(&app).unwrap().sync, PlacementSync::OutOfSync);
    app.status.as_mut().unwrap().observed_placement_revision = Some(14);
    assert_eq!(
        placement(&app).unwrap().sync,
        PlacementSync::AtDesiredRevision
    );
    app.status = None;
    assert_eq!(placement(&app).unwrap().sync, PlacementSync::OutOfSync);
    app.metadata.annotations = None;
    assert_eq!(placement(&app).unwrap().sync, PlacementSync::Unknown);
    for invalid in ["no", "-1", "0", "9223372036854775808"] {
        app.metadata.annotations =
            Some([("loco.io/placement-revision".into(), invalid.into())].into());
        assert!(matches!(placement(&app), Err(Error::InvalidRevision(_))));
    }
}

#[test]
fn unknown_phases_and_missing_identity_do_not_panic() {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/application.json")).unwrap();
    value["status"]["phase"] = json!("FuturePhase");
    let mut app: Application = serde_json::from_value(value).unwrap();
    assert_eq!(
        application_model(&app).unwrap().phase,
        ApplicationStatusPhase::Unknown
    );
    app.spec.workspace_id = None;
    assert!(matches!(
        application_model(&app),
        Err(Error::MissingField("spec.workspaceId"))
    ));
    assert!(matches!(
        ObjectRef::from_metadata("Pod", &ObjectMeta::default()),
        Err(Error::MissingField(_))
    ));
}

#[test]
fn ownership_uses_controller_annotations_and_shared_workspace_labels() {
    let app = application();
    let metadata: ObjectMeta = serde_json::from_value(json!({
        "name": "resource-app", "namespace": "ws-workspace",
        "labels": {"app": "resource-app", "app.kubernetes.io/managed-by": "loco-controller"},
        "annotations": {"loco.io/application-name": "resource-app", "loco.io/application-namespace": "loco-system"}
    })).unwrap();
    for kind in [
        "Deployment",
        "Service",
        "HTTPRoute",
        "Secret",
        "ServiceAccount",
        "NetworkPolicy",
    ] {
        assert_eq!(
            related_object(&app, kind, &metadata)
                .unwrap()
                .unwrap()
                .relationship,
            ObjectRelationship::Application
        );
    }
    let shared: ObjectMeta = serde_json::from_value(json!({"name": "allow-workspace", "namespace": "ws-workspace", "labels": {"app.kubernetes.io/managed-by": "loco-controller", "loco.io/workspace-id": "workspace"}})).unwrap();
    assert_eq!(
        related_object(&app, "NetworkPolicy", &shared)
            .unwrap()
            .unwrap()
            .relationship,
        ObjectRelationship::SharedWorkspace
    );
    let mut unrelated = metadata.clone();
    unrelated.namespace = Some("ws-other".into());
    assert!(
        related_object(&app, "Service", &unrelated)
            .unwrap()
            .is_none()
    );
    unrelated = metadata.clone();
    unrelated.annotations = None;
    assert!(
        related_object(&app, "Service", &unrelated)
            .unwrap()
            .is_none()
    );
}

#[test]
fn staging_secret_uses_uid_owner_and_rejects_recreated_application() {
    let mut app = application();
    let metadata: ObjectMeta = serde_json::from_value(json!({"name": "env-placement", "namespace": "loco-system", "ownerReferences": [{"apiVersion": "infra.loco.io/v1alpha1", "kind": "Application", "name": "resource-app", "uid": "application-uid"}]})).unwrap();
    assert!(related_object(&app, "Secret", &metadata).unwrap().is_some());
    app.metadata.uid = Some("new-uid".into());
    assert!(related_object(&app, "Secret", &metadata).unwrap().is_none());
}

#[test]
fn build_duration_digest_and_unavailable_cache_percentage() {
    let model = build_model(&build(), &[], &[], now()).unwrap();
    assert_eq!(model.phase, BuildStatusPhase::Succeeded);
    assert_eq!(model.duration.unwrap().num_seconds(), 131);
    assert_eq!(model.image_digest.as_deref(), Some("sha256:image"));
    assert_eq!(model.cache_digest.as_deref(), Some("sha256:cache"));
    assert!(model.cache_requested);
    assert_eq!(model.cache_hit, CacheHit::Unavailable);
}

#[test]
fn build_running_terminal_and_invalid_times() {
    let mut build = build();
    let status = build.status.as_mut().unwrap();
    status.phase = Some(BuildStatusPhase::Running);
    status.finished_at = None;
    assert_eq!(
        build_model(&build, &[], &[], now())
            .unwrap()
            .duration
            .unwrap()
            .num_seconds(),
        180
    );
    build.status.as_mut().unwrap().phase = Some(BuildStatusPhase::Failed);
    assert_eq!(build_model(&build, &[], &[], now()).unwrap().duration, None);
    build.status.as_mut().unwrap().finished_at = Some("2026-10-12T20:00:00Z".into());
    assert!(matches!(
        build_model(&build, &[], &[], now()),
        Err(Error::InvalidDuration)
    ));
    build.status.as_mut().unwrap().started_at = Some("invalid".into());
    assert!(matches!(
        build_model(&build, &[], &[], now()),
        Err(Error::InvalidTimestamp(_))
    ));
}

#[test]
fn build_jobs_and_pods_use_actual_uid_ownership_and_labels() {
    let build = build();
    let job: Job = serde_json::from_value(json!({"metadata": {"name": "build-id", "namespace": "loco-builds", "uid": "job-uid", "ownerReferences": [{"apiVersion": "infra.loco.io/v1alpha1", "kind": "Build", "name": "build-id", "uid": "build-uid"}]}})).unwrap();
    let pod: Pod = serde_json::from_value(json!({"metadata": {"name": "build-id-random", "namespace": "loco-builds", "labels": {"loco.io/build-id": "build-id", "app.kubernetes.io/managed-by": "loco-controller"}, "ownerReferences": [{"apiVersion": "batch/v1", "kind": "Job", "name": "build-id", "uid": "job-uid"}]}})).unwrap();
    let mut stale = pod.clone();
    stale.metadata.namespace = Some("other".into());
    let model = build_model(&build, &[job], &[pod, stale], now()).unwrap();
    assert_eq!(model.jobs.first().unwrap().name, "build-id");
    assert_eq!(model.pods.len(), 1);
}

fn service() -> Service {
    serde_json::from_value(
        json!({"metadata": {"name": "resource-app", "namespace": "ws-workspace"}}),
    )
    .unwrap()
}

fn endpoints(ready: bool) -> EndpointSlice {
    serde_json::from_value(json!({"metadata": {"name": "app-slice", "namespace": "ws-workspace", "labels": {"kubernetes.io/service-name": "resource-app"}}, "addressType": "IPv4", "endpoints": [{"addresses": ["10.0.0.1"], "conditions": {"ready": ready}}]})).unwrap()
}

#[test]
fn routes_rank_missing_backend_and_no_ready_endpoints_before_healthy() {
    let route = route();
    let healthy = route_model(&route, "loco-system", &[service()], &[endpoints(true)]).unwrap();
    let unhealthy = route_model(&route, "loco-system", &[service()], &[endpoints(false)]).unwrap();
    let missing = route_model(&route, "loco-system", &[], &[]).unwrap();
    assert_eq!(healthy.problem, RouteProblem::None);
    assert_eq!(healthy.backends.first().unwrap().ready_endpoints, Some(1));
    assert_eq!(unhealthy.problem, RouteProblem::NoReadyEndpoints);
    assert_eq!(missing.problem, RouteProblem::BackendNotFound);
    let mut routes = [healthy, missing, unhealthy];
    rank_routes(&mut routes);
    assert_eq!(
        routes.map(|route| route.problem),
        [
            RouteProblem::BackendNotFound,
            RouteProblem::NoReadyEndpoints,
            RouteProblem::None
        ]
    );
}

#[test]
fn service_without_slices_has_no_ready_endpoints() {
    let model = route_model(&route(), "loco-system", &[service()], &[]).unwrap();
    assert_eq!(model.problem, RouteProblem::NoReadyEndpoints);
}

#[test]
fn stale_and_other_gateway_conditions_are_unknown() {
    let mut route = route();
    route.metadata.generation = Some(3);
    assert_eq!(
        route_model(&route, "loco-system", &[service()], &[endpoints(true)])
            .unwrap()
            .problem,
        RouteProblem::Unknown
    );
    route.metadata.generation = Some(2);
    route.status.parents.first_mut().unwrap().parent_ref.name = "other".into();
    assert_eq!(
        route_model(&route, "loco-system", &[service()], &[endpoints(true)])
            .unwrap()
            .problem,
        RouteProblem::Unknown
    );
}

#[test]
fn backend_not_found_condition_is_preserved_and_weight_zero_ignored() {
    let mut route = route();
    let condition = route
        .status
        .parents
        .first_mut()
        .unwrap()
        .conditions
        .first_mut()
        .unwrap();
    condition.type_ = "ResolvedRefs".into();
    condition.status = "False".into();
    condition.reason = "BackendNotFound".into();
    condition.message = "backend missing".into();
    let model = route_model(&route, "loco-system", &[service()], &[endpoints(true)]).unwrap();
    assert_eq!(model.problem, RouteProblem::BackendNotFound);
    assert_eq!(model.messages, ["backend missing"]);
    route
        .spec
        .rules
        .first_mut()
        .unwrap()
        .backend_refs
        .first_mut()
        .unwrap()
        .weight = Some(0);
    let model = route_model(&route, "loco-system", &[], &[]).unwrap();
    assert_eq!(model.backends.len(), 0);
}

#[test]
fn gateway_listener_tls_namespace_and_missing_status_are_preserved() {
    let gateway: Gateway = serde_json::from_str(include_str!("fixtures/gateway.json")).unwrap();
    let models = gateway_listeners(&gateway).unwrap();
    let https = models.first().unwrap();
    assert_eq!(https.attached_routes, Some(41));
    assert_eq!(https.certificates.first().unwrap().namespace, "loco-system");
    assert_eq!(models.last().unwrap().attached_routes, None);
}

#[derive(Deserialize)]
struct Workloads {
    deployment: Deployment,
    daemonset: DaemonSet,
    statefulset: StatefulSet,
    pod: Pod,
}

fn workloads() -> Workloads {
    serde_json::from_str(include_str!("fixtures/workloads.json")).unwrap()
}

#[test]
fn workload_health_and_images_from_fixtures() {
    let workloads = workloads();
    let deployment = deployment_health(&workloads.deployment).unwrap();
    assert_eq!(deployment.health, Health::Healthy);
    assert_eq!(deployment.images, ["agent:sha-abc"]);
    assert_eq!(
        daemonset_health(&workloads.daemonset).unwrap().health,
        Health::Healthy
    );
    assert_eq!(
        statefulset_health(&workloads.statefulset).unwrap().health,
        Health::Healthy
    );
    assert_eq!(pod_health(&workloads.pod).unwrap().health, Health::Healthy);
}

#[test]
fn workload_rollups_preserve_unknown_stale_and_suspended() {
    let mut workloads = workloads();
    workloads
        .deployment
        .status
        .as_mut()
        .unwrap()
        .observed_generation = Some(1);
    assert_eq!(
        deployment_health(&workloads.deployment).unwrap().health,
        Health::Unknown
    );
    workloads
        .deployment
        .status
        .as_mut()
        .unwrap()
        .observed_generation = Some(2);
    workloads.deployment.status.as_mut().unwrap().ready_replicas = None;
    assert_eq!(
        deployment_health(&workloads.deployment).unwrap().health,
        Health::Unhealthy
    );
    workloads.deployment.spec.as_mut().unwrap().replicas = Some(0);
    assert_eq!(
        deployment_health(&workloads.deployment).unwrap().health,
        Health::Suspended
    );
    assert_eq!(health_rollup([]), Health::Unknown);
    assert_eq!(
        health_rollup([Health::Healthy, Health::Unhealthy]),
        Health::Unhealthy
    );
}

#[test]
fn catalog_uses_installed_names_and_label_selected_clickhouse() {
    let catalog = component_catalog();
    let agent = catalog
        .iter()
        .find(|component| component.name == "agent")
        .unwrap();
    assert_eq!(
        agent.selector,
        ComponentSelector::Name("loco-core-agent".into())
    );
    let workloads = workloads();
    let health = deployment_health(&workloads.deployment).unwrap();
    assert_eq!(component_health(agent, &[health]).health, Health::Healthy);
    let clickhouse = catalog
        .iter()
        .find(|component| component.name == "clickhouse")
        .unwrap();
    let pod = pod_health(&workloads.pod).unwrap();
    assert_eq!(component_health(clickhouse, &[pod]).health, Health::Healthy);
    assert_eq!(component_health(clickhouse, &[]).health, Health::Unknown);
    assert_eq!(
        catalog
            .iter()
            .find(|component| component.name == "envoy-gateway")
            .unwrap()
            .namespace,
        "loco-system"
    );
}

#[test]
fn agent_sync_exposes_discovery_only() {
    let mut discovery: APIResourceList = serde_json::from_value(json!({"groupVersion": "infra.loco.io/v1alpha1", "resources": [{"name": "builds", "singularName": "build", "namespaced": true, "kind": "Build", "verbs": ["list"]}]})).unwrap();
    let sync = agent_sync(Some(&discovery));
    assert_eq!(sync.builds_enabled, Observation::Observed(true));
    assert_eq!(sync.last_heartbeat, Observation::Unavailable);
    assert_eq!(sync.connected, Observation::Unavailable);
    assert_eq!(sync.reconnects, Observation::Unavailable);
    discovery.resources.clear();
    assert_eq!(
        agent_sync(Some(&discovery)).builds_enabled,
        Observation::Observed(false)
    );
    assert_eq!(agent_sync(None).builds_enabled, Observation::Unavailable);
}

#[test]
fn rollout_with_all_old_replicas_ready_is_still_progressing() {
    let mut workloads = workloads();
    workloads
        .deployment
        .status
        .as_mut()
        .unwrap()
        .updated_replicas = Some(0);
    assert_eq!(
        deployment_health(&workloads.deployment).unwrap().health,
        Health::Progressing
    );
    workloads
        .statefulset
        .status
        .as_mut()
        .unwrap()
        .current_revision = Some("old".into());
    workloads
        .statefulset
        .status
        .as_mut()
        .unwrap()
        .update_revision = Some("new".into());
    assert_eq!(
        statefulset_health(&workloads.statefulset).unwrap().health,
        Health::Progressing
    );
    workloads
        .daemonset
        .status
        .as_mut()
        .unwrap()
        .updated_number_scheduled = Some(0);
    assert_eq!(
        daemonset_health(&workloads.daemonset).unwrap().health,
        Health::Progressing
    );
}

#[test]
fn dual_stack_endpoint_slices_count_each_pod_once() {
    let mut ipv4 = endpoints(true);
    ipv4.endpoints
        .as_mut()
        .unwrap()
        .first_mut()
        .unwrap()
        .target_ref = Some(serde_json::from_value(json!({"uid": "pod-uid"})).unwrap());
    let mut ipv6 = ipv4.clone();
    ipv6.address_type = "IPv6".into();
    ipv6.endpoints
        .as_mut()
        .unwrap()
        .first_mut()
        .unwrap()
        .addresses = vec!["::1".into()];
    let model = route_model(&route(), "loco-system", &[service()], &[ipv4, ipv6]).unwrap();
    assert_eq!(model.backends.first().unwrap().ready_endpoints, Some(1));
}

#[test]
fn omitted_go_replica_counters_mean_zero_when_status_exists() {
    let mut app = application();
    app.status.as_mut().unwrap().ready_replicas = None;
    app.status.as_mut().unwrap().desired_replicas = None;
    let model = application_model(&app).unwrap();
    assert_eq!(model.ready_replicas, Some(0));
    assert_eq!(model.desired_replicas, Some(0));
    app.status = None;
    let model = application_model(&app).unwrap();
    assert_eq!(model.ready_replicas, None);
}
