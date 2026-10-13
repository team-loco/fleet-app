use std::time::Duration;

use super::applications::{Application, Applications};
use super::{BuildPhase, Builds, Workspace};
use crate::Status;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3600;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reconnects {
    pub count: u32,
    pub last_at: Option<String>,
    pub reason: Option<String>,
}

impl Reconnects {
    pub fn summary(&self) -> String {
        let mut parts = vec![self.count.to_string()];
        parts.extend(self.last_at.as_ref().map(|at| format!("last {at}")));
        parts.extend(self.reason.clone());
        parts.join(" · ")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentSync {
    pub control_plane: Option<String>,
    pub connected: Option<bool>,
    pub uptime: Option<Duration>,
    pub last_heartbeat: Option<Duration>,
    pub kubernetes_healthy: Option<bool>,
    pub controller_healthy: Option<bool>,
    pub reconnects: Option<Reconnects>,
    pub builds_enabled: Option<bool>,
}

impl AgentSync {
    pub fn stream(&self) -> Option<(Status, String)> {
        let connected = self.connected?;
        let target = self
            .control_plane
            .as_ref()
            .map(|host| format!(" to {host}"))
            .unwrap_or_default();
        let line = match (connected, self.uptime) {
            (true, Some(up)) => format!("connected{target} · up {}", uptime(up)),
            (true, None) => format!("connected{target}"),
            (false, _) => format!("disconnected from{target}"),
        };
        let status = if connected { Status::Ok } else { Status::Err };
        Some((status, line))
    }
}

pub fn uptime(up: Duration) -> String {
    let secs = up.as_secs();
    let hours = secs / SECONDS_PER_HOUR;
    let minutes = secs % SECONDS_PER_HOUR / SECONDS_PER_MINUTE;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub status: Status,
    pub name: String,
    pub namespace: String,
    pub ready: Option<(u32, u32)>,
    pub version: Option<String>,
    pub detail: String,
}

impl Component {
    pub fn ready_label(&self) -> String {
        self.ready.map_or_else(
            || "—".to_owned(),
            |(ready, desired)| format!("{ready}/{desired}"),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLogLine {
    pub time: String,
    pub level: AgentLevel,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Platform {
    pub namespace: String,
    pub images: Option<String>,
    pub agent: AgentSync,
    pub components: Vec<Component>,
    pub log: Vec<AgentLogLine>,
}

impl Platform {
    pub fn subtitle(&self) -> String {
        match &self.images {
            Some(images) => format!("{} · images {images}", self.namespace),
            None => self.namespace.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildQueue {
    pub running: usize,
    pub queued: usize,
}

impl Builds {
    pub fn queue(&self) -> BuildQueue {
        let count = |phase: BuildPhase| {
            self.builds
                .iter()
                .filter(|build| build.phase == phase)
                .count()
        };
        BuildQueue {
            running: count(BuildPhase::Running),
            queued: count(BuildPhase::Pending),
        }
    }
}

impl Application {
    pub fn placement_status(&self) -> Option<Status> {
        match &self.placement.issue {
            Some(issue) => Some(issue.status),
            None => self.placement.out_of_sync().then_some(Status::Warn),
        }
    }
}

impl Applications {
    pub fn unsettled(&self) -> Vec<(&Workspace, &Application)> {
        let mut unsettled: Vec<_> = self
            .iter()
            .filter(|(_, app)| app.placement_status().is_some())
            .collect();
        unsettled.sort_by_key(|(_, app)| std::cmp::Reverse(app.placement_status()));
        unsettled
    }

    pub fn placement_summary(&self) -> PlacementSummary {
        let unsettled = self.unsettled();
        let statuses: Vec<Status> = unsettled
            .iter()
            .filter_map(|(_, app)| app.placement_status())
            .collect();
        let count = |wanted: Status| statuses.iter().filter(|s| **s == wanted).count();
        PlacementSummary {
            total: self.total,
            at_desired: self.total.saturating_sub(unsettled.len()),
            badges: [Status::Progress, Status::Warn, Status::Err]
                .into_iter()
                .map(|status| (status, count(status)))
                .filter(|(_, n)| *n > 0)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementSummary {
    pub total: usize,
    pub at_desired: usize,
    pub badges: Vec<(Status, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loco::applications::tests::application;
    use crate::loco::{AppPhase, Placement, PlacementIssue, WorkspaceApps};

    #[test]
    fn uptime_reads_hours_and_minutes() {
        assert_eq!(
            uptime(Duration::from_secs(3 * 3600 + 12 * 60 + 5)),
            "3h 12m"
        );
        assert_eq!(uptime(Duration::from_mins(9)), "9m");
    }

    #[test]
    fn stream_is_unavailable_until_observed() {
        assert_eq!(AgentSync::default().stream(), None);
        let agent = AgentSync {
            control_plane: Some("api.loco.build".into()),
            connected: Some(true),
            uptime: Some(Duration::from_mins(192)),
            ..AgentSync::default()
        };
        assert_eq!(
            agent.stream(),
            Some((Status::Ok, "connected to api.loco.build · up 3h 12m".into()))
        );
        let reconnects = Reconnects {
            count: 2,
            last_at: Some("18:04".into()),
            reason: Some("stream closed by server (GOAWAY)".into()),
        };
        assert_eq!(
            reconnects.summary(),
            "2 · last 18:04 · stream closed by server (GOAWAY)"
        );
    }

    #[test]
    fn unsettled_placements_count_against_the_total() {
        let issue = |status| PlacementIssue {
            status,
            message: String::new(),
        };
        let mut failing = application("api", AppPhase::Ready, Placement::default());
        failing.placement.issue = Some(issue(Status::Err));
        let mut applying = application("docs", AppPhase::Deploying, Placement::default());
        applying.placement.issue = Some(issue(Status::Progress));
        let drifted = application(
            "jobs",
            AppPhase::Ready,
            Placement {
                desired: Some(2),
                applied: Some(1),
                issue: None,
            },
        );
        let fine = application("web", AppPhase::Ready, Placement::default());
        let applications = Applications {
            total: 10,
            workspaces: vec![WorkspaceApps {
                workspace: Workspace {
                    namespace: "ws-1".into(),
                    name: None,
                },
                applications: vec![failing, applying, drifted, fine],
            }],
        };
        let summary = applications.placement_summary();
        assert_eq!(summary.at_desired, 7);
        assert_eq!(
            summary.badges,
            [(Status::Progress, 1), (Status::Warn, 1), (Status::Err, 1)]
        );
    }
}
