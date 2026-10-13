use std::collections::BTreeMap;
use std::time::Instant;

use fleet_core::loco::{AppPhase, BuildLogLine, LocoCluster, LogTone};
use fleet_core::{ClusterId, ResourceRef, Status};

use super::LocoSettings;

const CANCEL_REQUESTED: &str = "cancel requested from Fleet";
const CANCEL_FAILED: &str = "cancel failed: ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    Restarting { since: Instant },
    Canceling,
    CancelFailed(String),
}

pub fn apply_pending(
    loco: &mut LocoCluster,
    cluster: &ClusterId,
    pending: &mut BTreeMap<ResourceRef, Pending>,
    now: Instant,
    settings: &LocoSettings,
) {
    pending.retain(|resource, action| {
        if &resource.cluster != cluster {
            return true;
        }
        match action {
            Pending::Restarting { since } => {
                let Some(application) = loco.applications.find_mut(resource) else {
                    return false;
                };
                let elapsed = now.saturating_duration_since(*since);
                let rolled_out = application
                    .objects
                    .iter()
                    .find(|object| object.kind == "Deployment")
                    .is_some_and(|deployment| deployment.status == Status::Ok);
                let settled = elapsed >= settings.restart_grace && rolled_out;
                if settled || elapsed >= settings.restart_timeout {
                    return false;
                }
                application.phase = AppPhase::Deploying;
                true
            }
            Pending::Canceling => note(loco, resource, LogTone::Dim, CANCEL_REQUESTED.to_owned()),
            Pending::CancelFailed(error) => note(
                loco,
                resource,
                LogTone::Error,
                format!("{CANCEL_FAILED}{error}"),
            ),
        }
    });
}

fn note(loco: &mut LocoCluster, resource: &ResourceRef, tone: LogTone, text: String) -> bool {
    let Some(build) = loco.builds.find_mut(resource) else {
        return false;
    };
    if !build.phase.is_active() {
        return false;
    }
    build.log.push(BuildLogLine::new(tone, text));
    true
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use fleet_core::Uid;
    use fleet_core::loco::{
        Application, Applications, Build, BuildPhase, Builds, ControllerObject, Placement,
        Workspace, WorkspaceApps,
    };

    use super::*;

    fn reference(kind: &str, name: &str) -> ResourceRef {
        ResourceRef {
            cluster: ClusterId::new("b"),
            kind: kind.into(),
            namespace: Some("loco-system".into()),
            name: name.into(),
        }
    }

    fn workspace() -> Workspace {
        Workspace {
            namespace: "ws-1".into(),
            name: None,
        }
    }

    fn cluster(deployment: Status, build: BuildPhase) -> LocoCluster {
        let application = Application {
            resource: reference("Application", "resource-web"),
            name: "resource-web".into(),
            environment: String::new(),
            region: None,
            phase: AppPhase::Ready,
            ready_replicas: None,
            desired_replicas: None,
            placement: Placement::default(),
            hostname: None,
            degraded: None,
            image: None,
            rollout: None,
            resources: None,
            health_check: None,
            workload_uid: Uid::default(),
            objects: vec![ControllerObject {
                status: deployment,
                kind: "Deployment".into(),
                namespace: "ws-1".into(),
                name: Some("resource-web".into()),
                detail: String::new(),
            }],
            route: None,
        };
        let build = Build {
            resource: reference("Build", "b1"),
            id: "b1".into(),
            service: "resource-web".into(),
            workspace: workspace(),
            phase: build,
            started: None,
            took: None,
            image_digest: None,
            cache_hit_percent: None,
            job_uid: Uid::default(),
            job: None,
            node: None,
            fetch: Status::Ok,
            buildkit: Status::Progress,
            log: Vec::new(),
        };
        LocoCluster {
            applications: Applications {
                total: 1,
                workspaces: vec![WorkspaceApps {
                    workspace: workspace(),
                    applications: vec![application],
                }],
            },
            builds: Builds {
                builds: vec![build],
                ..Builds::default()
            },
            ..LocoCluster::default()
        }
    }

    fn phase(loco: &LocoCluster) -> Option<AppPhase> {
        loco.applications.iter().next().map(|(_, app)| app.phase)
    }

    #[test]
    fn restart_shows_deploying_until_the_rollout_settles() {
        let settings = LocoSettings::default();
        let start = Instant::now();
        let id = ClusterId::new("b");
        let app = reference("Application", "resource-web");
        let mut pending = BTreeMap::from([(app, Pending::Restarting { since: start })]);
        let mut early = cluster(Status::Ok, BuildPhase::Running);
        apply_pending(&mut early, &id, &mut pending, start, &settings);
        assert_eq!(phase(&early), Some(AppPhase::Deploying));
        let later = start + settings.restart_grace;
        let mut rolling = cluster(Status::Progress, BuildPhase::Running);
        apply_pending(&mut rolling, &id, &mut pending, later, &settings);
        assert_eq!(phase(&rolling), Some(AppPhase::Deploying));
        let mut done = cluster(Status::Ok, BuildPhase::Running);
        apply_pending(&mut done, &id, &mut pending, later, &settings);
        assert_eq!(phase(&done), Some(AppPhase::Ready));
        assert!(pending.is_empty());
    }

    #[test]
    fn restart_gives_up_after_the_timeout() {
        let settings = LocoSettings::default();
        let start = Instant::now();
        let id = ClusterId::new("b");
        let app = reference("Application", "resource-web");
        let mut pending = BTreeMap::from([(app, Pending::Restarting { since: start })]);
        let mut stuck = cluster(Status::Err, BuildPhase::Running);
        let timeout = start + settings.restart_timeout + Duration::from_secs(1);
        apply_pending(&mut stuck, &id, &mut pending, timeout, &settings);
        assert!(pending.is_empty());
    }

    #[test]
    fn cancel_notes_the_request_until_the_build_stops() {
        let settings = LocoSettings::default();
        let now = Instant::now();
        let id = ClusterId::new("b");
        let build = reference("Build", "b1");
        let other = ResourceRef {
            cluster: ClusterId::new("a"),
            ..build.clone()
        };
        let mut pending = BTreeMap::from([
            (build.clone(), Pending::Canceling),
            (other.clone(), Pending::Canceling),
        ]);
        let mut running = cluster(Status::Ok, BuildPhase::Running);
        apply_pending(&mut running, &id, &mut pending, now, &settings);
        let log: Vec<&str> = running
            .builds
            .builds
            .iter()
            .flat_map(|b| b.log.iter().map(|l| l.text.as_str()))
            .collect();
        assert_eq!(log, [CANCEL_REQUESTED]);
        let mut canceled = cluster(Status::Ok, BuildPhase::Canceled);
        apply_pending(&mut canceled, &id, &mut pending, now, &settings);
        assert_eq!(pending.keys().collect::<Vec<_>>(), [&other]);
    }

    #[test]
    fn failed_cancels_show_the_error() {
        let settings = LocoSettings::default();
        let id = ClusterId::new("b");
        let build = reference("Build", "b1");
        let mut pending = BTreeMap::from([(build, Pending::CancelFailed("forbidden".into()))]);
        let mut running = cluster(Status::Ok, BuildPhase::Running);
        apply_pending(&mut running, &id, &mut pending, Instant::now(), &settings);
        let last = running
            .builds
            .builds
            .first()
            .and_then(|b| b.log.last())
            .map(|l| (l.tone, l.text.clone()));
        assert_eq!(
            last,
            Some((LogTone::Error, "cancel failed: forbidden".to_owned()))
        );
    }
}
