use std::collections::BTreeMap;
#[cfg(feature = "loco")]
use std::time::Duration;

use crate::home::ClusterMetrics;
use crate::{
    ChangesLog, ClusterCatalog, ClusterDetail, ClusterId, Comparison, Counts, FleetHome,
    FluxObject, GitOps, Page, ResourceRef, ResourceSummary, Status,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterSummary {
    pub id: ClusterId,
    pub name: String,
    pub status: Status,
    pub state: String,
    pub distro: String,
    pub region: String,
    pub version: Option<String>,
    pub nodes: Option<u32>,
    pub problems: Counts,
    pub nav: BTreeMap<Page, Counts>,
    pub metrics: Option<ClusterMetrics>,
    #[cfg(feature = "loco")]
    pub loco_agent_synced: Option<Duration>,
}

impl ClusterSummary {
    pub fn is_connected(&self) -> bool {
        self.status != Status::Offline
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub clusters: Vec<ClusterSummary>,
    pub fleet_nav: BTreeMap<Page, Counts>,
    pub home: FleetHome,
    pub resources: Vec<ResourceSummary>,
    pub changes: ChangesLog,
    pub details: BTreeMap<ClusterId, ClusterDetail>,
    pub gitops: GitOps,
    pub flux: Vec<FluxObject>,
    #[cfg(feature = "loco")]
    pub loco: BTreeMap<ClusterId, crate::loco::LocoCluster>,
    pub comparisons: Vec<Comparison>,
    pub catalog: ClusterCatalog,
    pub pods: Vec<crate::pods::Pod>,
}

impl Snapshot {
    pub fn cluster(&self, id: &ClusterId) -> Option<&ClusterSummary> {
        self.clusters.iter().find(|c| &c.id == id)
    }

    pub fn detail(&self, id: &ClusterId) -> Option<&ClusterDetail> {
        self.details.get(id)
    }

    pub fn comparison(&self, resource: &ResourceRef) -> Option<&Comparison> {
        self.comparisons.iter().find(|c| c.matches(resource))
    }

    pub fn connected(&self) -> usize {
        self.clusters.iter().filter(|c| c.is_connected()).count()
    }

    pub fn problems(&self) -> Counts {
        self.clusters.iter().map(|c| c.problems).sum()
    }

    pub fn status(&self) -> Status {
        Status::worst(self.clusters.iter().map(|c| c.status))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn cluster(name: &str, status: Status, problems: Counts) -> ClusterSummary {
        ClusterSummary {
            id: ClusterId::new(name),
            name: name.into(),
            status,
            state: String::new(),
            distro: String::new(),
            region: String::new(),
            version: None,
            nodes: None,
            problems,
            nav: BTreeMap::new(),
            metrics: None,
            #[cfg(feature = "loco")]
            loco_agent_synced: None,
        }
    }

    pub(crate) fn sample() -> Snapshot {
        Snapshot {
            clusters: vec![
                cluster("a", Status::Err, Counts::default().with_err(2).with_warn(3)),
                cluster("b", Status::Ok, Counts::default().with_sus(1)),
                cluster("c", Status::Offline, Counts::default()),
            ],
            fleet_nav: BTreeMap::new(),
            home: FleetHome::default(),
            resources: Vec::new(),
            changes: ChangesLog::default(),
            details: BTreeMap::new(),
            gitops: GitOps::default(),
            flux: Vec::new(),
            #[cfg(feature = "loco")]
            loco: BTreeMap::new(),
            comparisons: Vec::new(),
            catalog: ClusterCatalog::default(),
            pods: Vec::new(),
        }
    }

    #[test]
    fn rollups_sum_problems_and_take_worst_status() {
        let snapshot = sample();
        assert_eq!(snapshot.connected(), 2);
        assert_eq!(
            snapshot.problems(),
            Counts::default().with_err(2).with_warn(3).with_sus(1)
        );
        assert_eq!(snapshot.status(), Status::Err);
    }

    #[test]
    fn empty_fleet_is_ok() {
        assert_eq!(Snapshot::default().status(), Status::Ok);
    }
}
