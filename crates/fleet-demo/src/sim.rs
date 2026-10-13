use fleet_core::{ClusterId, Command, ResourceRef, Snapshot, Status};

use crate::compare;

pub const CONNECTING: &str = "Connecting";
pub const REAUTHENTICATING: &str = "Re-authenticating";
const DISCONNECTED: &str = "Disconnected";
const HEALTHY: &str = "Healthy";

pub fn apply_now(snapshot: &mut Snapshot, command: &Command) -> bool {
    match command {
        Command::SetPreferences(cluster, preferences) => {
            let Some(settings) = snapshot.catalog.get_mut(cluster) else {
                return false;
            };
            settings.preferences = preferences.clone();
            true
        }
        Command::Disconnect(cluster) => {
            set_cluster(snapshot, cluster, Status::Offline, DISCONNECTED).is_some()
        }
        _ => false,
    }
}

pub fn reconnect(snapshot: &mut Snapshot, cluster: &ClusterId, state: &str) -> Option<()> {
    set_cluster(snapshot, cluster, Status::Progress, state)
}

pub fn connected(snapshot: &mut Snapshot, cluster: &ClusterId) {
    if let Some(settings) = snapshot.catalog.get_mut(cluster) {
        settings.auth_expired = false;
    }
    set_cluster(snapshot, cluster, Status::Ok, HEALTHY);
}

pub fn copy_spec(snapshot: &mut Snapshot, from: &ClusterId, to: &ResourceRef) -> Option<()> {
    let comparison = snapshot.comparisons.iter_mut().find(|c| c.matches(to))?;
    compare::copy_spec(comparison, from, &to.cluster).then_some(())
}

pub fn rolled_out(snapshot: &mut Snapshot, target: &ResourceRef) {
    let comparison = snapshot.comparisons.iter_mut().find(|c| c.matches(target));
    if let Some(comparison) = comparison {
        compare::finish_rollout(comparison, &target.cluster);
    }
}

fn set_cluster(
    snapshot: &mut Snapshot,
    cluster: &ClusterId,
    status: Status,
    state: &str,
) -> Option<()> {
    let summary = snapshot.clusters.iter_mut().find(|c| &c.id == cluster)?;
    summary.status = status;
    state.clone_into(&mut summary.state);
    Some(())
}

#[cfg(test)]
mod tests {
    use fleet_core::{Page, Preferences, Seen, Uid};

    use super::*;
    use crate::simulate::{self, Settle};
    use crate::snapshot;

    fn cluster(name: &str) -> ClusterId {
        ClusterId::new(name)
    }

    fn status(snapshot: &Snapshot, name: &str) -> Option<(Status, String)> {
        let summary = snapshot.cluster(&cluster(name))?;
        Some((summary.status, summary.state.clone()))
    }

    #[test]
    fn reauthenticating_shows_progress_then_heals() {
        let mut state = snapshot();
        let pending = simulate::apply(&mut state, &Command::Reauthenticate(cluster("qa-gke")));
        assert_eq!(pending, Some(Settle::Connected(cluster("qa-gke"))));
        assert_eq!(
            status(&state, "qa-gke"),
            Some((Status::Progress, REAUTHENTICATING.into()))
        );
        if let Some(pending) = pending {
            simulate::settle(&mut state, &pending);
        }
        assert_eq!(status(&state, "qa-gke"), Some((Status::Ok, HEALTHY.into())));
        let settings = state.catalog.get(&cluster("qa-gke"));
        assert!(settings.is_some_and(|s| !s.auth_expired));
    }

    #[test]
    fn disconnecting_takes_the_cluster_offline() {
        let mut state = snapshot();
        assert!(apply_now(
            &mut state,
            &Command::Disconnect(cluster("staging"))
        ));
        assert_eq!(
            status(&state, "staging"),
            Some((Status::Offline, DISCONNECTED.into()))
        );
        assert!(!apply_now(
            &mut state,
            &Command::Disconnect(cluster("ghost"))
        ));
    }

    #[test]
    fn preferences_are_stored_per_cluster() {
        let mut state = snapshot();
        let preferences = Preferences {
            confirm_destructive: false,
            ..Preferences::default()
        };
        let command = Command::SetPreferences(cluster("dev-kind"), preferences.clone());
        assert!(apply_now(&mut state, &command));
        let stored = state.catalog.get(&cluster("dev-kind"));
        assert_eq!(stored.map(|s| &s.preferences), Some(&preferences));
    }

    #[test]
    fn copying_a_spec_finishes_the_rollout_later() {
        let mut state = snapshot();
        let target = ResourceRef {
            cluster: cluster("prod-us-east"),
            kind: "Deployment".into(),
            namespace: Some("payments".into()),
            name: "checkout-api".into(),
        };
        let command = Command::CopySpec {
            from: cluster("prod-eu-west"),
            to: Seen::new(target.clone(), Uid::default()),
            path: "spec.template".into(),
        };
        let pending = simulate::apply(&mut state, &command);
        assert_eq!(pending, Some(Settle::RolledOut(target.clone())));
        let rolling = state.comparison(&target).map(|c| c.clusters[0].status);
        assert_eq!(rolling, Some(Status::Progress));
        if let Some(pending) = pending {
            simulate::settle(&mut state, &pending);
        }
        let settled = state.comparison(&target).map(|c| c.clusters[0].status);
        assert_eq!(settled, Some(Status::Ok));
    }

    #[test]
    fn navigation_commands_leave_the_data_alone() {
        let mut state = snapshot();
        assert!(!apply_now(&mut state, &Command::SelectPage(Page::Pods)));
        assert_eq!(state, snapshot());
    }
}
