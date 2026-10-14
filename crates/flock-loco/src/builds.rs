use chrono::{DateTime, TimeDelta, Utc};
use k8s_openapi::api::{batch::v1::Job, core::v1::Pod};

use crate::generated::{BuildStatusPhase, builds::Build};
use crate::{BUILD_ID, CONTROLLER, Error, MANAGED_BY, ObjectRef, label, owned_by};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheHit {
    Unavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildModel {
    pub build: ObjectRef,
    pub workspace_id: String,
    pub resource_id: String,
    pub phase: BuildStatusPhase,
    pub message: Option<String>,
    pub duration: Option<TimeDelta>,
    pub image_digest: Option<String>,
    pub cache_digest: Option<String>,
    pub cache_requested: bool,
    pub cache_hit: CacheHit,
    pub jobs: Vec<ObjectRef>,
    pub pods: Vec<ObjectRef>,
}

fn timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, Error> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|_| Error::InvalidTimestamp(value.to_owned()))
        })
        .transpose()
}

pub fn build_model(
    build: &Build,
    jobs: &[Job],
    pods: &[Pod],
    now: DateTime<Utc>,
) -> Result<BuildModel, Error> {
    let reference = ObjectRef::from_metadata("Build", &build.metadata)?;
    let status = build.status.as_ref();
    let phase = status
        .and_then(|status| status.phase.clone())
        .unwrap_or(BuildStatusPhase::Unknown);
    let started = timestamp(status.and_then(|status| status.started_at.as_deref()))?;
    let finished = timestamp(status.and_then(|status| status.finished_at.as_deref()))?;
    let end = match phase {
        BuildStatusPhase::Running => Some(finished.unwrap_or(now)),
        _ => finished,
    };
    let duration = match (started, end) {
        (Some(start), Some(end)) => {
            let duration = end.signed_duration_since(start);
            if duration < TimeDelta::zero() {
                return Err(Error::InvalidDuration);
            }
            Some(duration)
        }
        _ => None,
    };
    let matching_jobs: Vec<_> = jobs
        .iter()
        .filter(|job| owned_by(&job.metadata, &build.metadata, "Build"))
        .collect();
    let job_refs = matching_jobs
        .iter()
        .map(|job| ObjectRef::from_metadata("Job", &job.metadata))
        .collect::<Result<Vec<_>, _>>()?;
    let mut pod_refs = Vec::new();
    for pod in pods {
        let job_owner = pod
            .metadata
            .owner_references
            .as_ref()
            .is_some_and(|owners| {
                owners.iter().any(|owner| {
                    owner.kind == "Job"
                        && owner.api_version == "batch/v1"
                        && matching_jobs
                            .iter()
                            .any(|job| job.metadata.uid.as_deref() == Some(owner.uid.as_str()))
                })
            });
        let build_label = label(&pod.metadata, BUILD_ID) == Some(build.spec.build_id.as_str());
        let managed = label(&pod.metadata, MANAGED_BY) == Some(CONTROLLER);
        if pod.metadata.namespace == build.metadata.namespace && job_owner && build_label && managed
        {
            let reference = ObjectRef::from_metadata("Pod", &pod.metadata)?;
            pod_refs.push(reference);
        }
    }
    pod_refs.sort();
    let mut job_refs = job_refs;
    job_refs.sort();
    let cache_requested = build
        .spec
        .cache_ref
        .as_ref()
        .is_some_and(|value| !value.is_empty());
    Ok(BuildModel {
        build: reference,
        workspace_id: build.spec.workspace_id.clone(),
        resource_id: build.spec.resource_id.clone(),
        phase,
        message: status.and_then(|status| status.message.clone()),
        duration,
        image_digest: status.and_then(|status| status.image_digest.clone()),
        cache_digest: status.and_then(|status| status.cache_digest.clone()),
        cache_requested,
        cache_hit: CacheHit::Unavailable,
        jobs: job_refs,
        pods: pod_refs,
    })
}
