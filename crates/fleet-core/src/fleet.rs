use std::collections::BTreeMap;

use crate::home::{ClusterMetrics, problems_badge};
use crate::{
    CLUSTER_NAV, ChangesLog, ClusterCatalog, ClusterDetail, ClusterId, Comparison, Counts,
    FLEET_NAV, FleetHome, FluxObject, Forward, GitOps, NavSection, Page, ResourceKind, ResourceRef,
    ResourceSummary, Status,
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
    pub forwards: Vec<Forward>,
}

impl Snapshot {
    pub fn cluster(&self, id: &ClusterId) -> Option<&ClusterSummary> {
        self.clusters.iter().find(|c| &c.id == id)
    }

    pub fn nav_sections(&self, cluster: Option<&ClusterId>) -> Vec<&'static NavSection> {
        match cluster {
            None => FLEET_NAV.iter().collect(),
            Some(id) => CLUSTER_NAV
                .iter()
                .filter(|section| self.shows_section(section, id))
                .collect(),
        }
    }

    #[cfg(feature = "loco")]
    fn shows_section(&self, section: &NavSection, id: &ClusterId) -> bool {
        !section.is_loco() || self.loco.contains_key(id)
    }

    #[cfg(not(feature = "loco"))]
    #[allow(clippy::unused_self)]
    fn shows_section(&self, _: &NavSection, _: &ClusterId) -> bool {
        true
    }

    pub fn detail(&self, id: &ClusterId) -> Option<&ClusterDetail> {
        self.details.get(id)
    }

    pub fn comparison(&self, resource: &ResourceRef) -> Option<&Comparison> {
        self.comparisons.iter().find(|c| c.matches(resource))
    }

    pub fn yaml(&self, resource: &ResourceRef) -> Option<&str> {
        let kind = ResourceKind::ALL
            .into_iter()
            .find(|kind| kind.name() == resource.kind)?;
        self.detail(&resource.cluster)?
            .resources(kind)
            .iter()
            .find(|row| {
                Some(row.namespace.as_str()) == resource.namespace.as_deref()
                    && row.name == resource.name
            })
            .map(|row| row.yaml.as_str())
            .filter(|yaml| !yaml.is_empty())
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

    pub fn remove_cluster(&mut self, id: &ClusterId) {
        self.clusters.retain(|c| &c.id != id);
        self.details.remove(id);
        self.home.problems.retain(|p| &p.target.cluster != id);
        self.resources.retain(|r| &r.resource.cluster != id);
        self.flux.retain(|o| &o.key.cluster != id);
        #[cfg(feature = "loco")]
        self.loco.remove(id);
        self.catalog.clusters.retain(|c| &c.cluster != id);
        self.pods.retain(|p| &p.cluster != id);
        self.forwards.retain(|f| &f.target.cluster != id);
        let badge = problems_badge(&self.home.problems);
        self.fleet_nav.insert(Page::Problems, badge);
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
            forwards: Vec::new(),
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

    fn problem(cluster: &str, status: Status) -> crate::home::Problem {
        crate::home::Problem {
            status,
            target: ResourceRef {
                cluster: ClusterId::new(cluster),
                kind: "Pod".into(),
                namespace: Some("ns".into()),
                name: "p".into(),
            },
            message: String::new(),
            age: std::time::Duration::ZERO,
        }
    }

    #[test]
    fn removing_a_cluster_drops_its_problems_and_badge_counts() {
        let mut snapshot = sample();
        snapshot.home.problems = vec![
            problem("a", Status::Err),
            problem("a", Status::Warn),
            problem("b", Status::Warn),
        ];
        snapshot
            .fleet_nav
            .insert(Page::Problems, problems_badge(&snapshot.home.problems));
        snapshot
            .details
            .insert(ClusterId::new("a"), ClusterDetail::default());
        snapshot.remove_cluster(&ClusterId::new("a"));
        assert!(snapshot.cluster(&ClusterId::new("a")).is_none());
        assert!(snapshot.detail(&ClusterId::new("a")).is_none());
        assert_eq!(snapshot.home.problems, vec![problem("b", Status::Warn)]);
        assert_eq!(
            snapshot.fleet_nav.get(&Page::Problems),
            Some(&Counts::default().with_warn(1))
        );
    }

    #[test]
    fn yaml_comes_from_the_inspected_row() {
        let mut snapshot = sample();
        let mut row = crate::resources::tests::resource("payments", "checkout-api", Status::Ok);
        row.yaml = "kind: Deployment".into();
        let mut detail = ClusterDetail::default();
        detail.resources.insert(ResourceKind::Deployment, vec![row]);
        snapshot.details.insert(ClusterId::new("a"), detail);
        let target = ResourceRef {
            cluster: ClusterId::new("a"),
            kind: "Deployment".into(),
            namespace: Some("payments".into()),
            name: "checkout-api".into(),
        };
        assert_eq!(snapshot.yaml(&target), Some("kind: Deployment"));
        let other = ResourceRef {
            name: "cart".into(),
            ..target.clone()
        };
        assert_eq!(snapshot.yaml(&other), None);
        let pod = ResourceRef {
            kind: "Pod".into(),
            ..target
        };
        assert_eq!(snapshot.yaml(&pod), None);
    }

    #[test]
    fn empty_fleet_is_ok() {
        assert_eq!(Snapshot::default().status(), Status::Ok);
    }

    fn labels(sections: &[&NavSection]) -> Vec<Option<&'static str>> {
        sections.iter().map(|section| section.label).collect()
    }

    #[test]
    fn fleet_home_shows_the_fleet_nav() {
        let snapshot = sample();
        assert_eq!(
            snapshot.nav_sections(None),
            FLEET_NAV.iter().collect::<Vec<_>>()
        );
    }

    #[cfg(not(feature = "loco"))]
    #[test]
    fn clusters_show_the_whole_cluster_nav() {
        let snapshot = sample();
        let sections = snapshot.nav_sections(Some(&ClusterId::new("a")));
        assert_eq!(sections, CLUSTER_NAV.iter().collect::<Vec<_>>());
        assert!(!labels(&sections).contains(&Some("Loco")));
    }

    #[cfg(feature = "loco")]
    #[test]
    fn loco_section_needs_loco_on_the_cluster() {
        let mut snapshot = sample();
        snapshot
            .loco
            .insert(ClusterId::new("a"), crate::loco::LocoCluster::default());
        let with = labels(&snapshot.nav_sections(Some(&ClusterId::new("a"))));
        let without = labels(&snapshot.nav_sections(Some(&ClusterId::new("b"))));
        assert!(with.contains(&Some("Loco")));
        assert!(!without.contains(&Some("Loco")));
        assert_eq!(with.len(), without.len() + 1);
    }
}
