#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Page {
    Overview,
    Problems,
    GitOps,
    Changes,
    ClusterOverview,
    Nodes,
    Namespaces,
    Events,
    #[cfg(feature = "loco")]
    LocoPlatform,
    #[cfg(feature = "loco")]
    LocoApplications,
    #[cfg(feature = "loco")]
    LocoBuilds,
    #[cfg(feature = "loco")]
    LocoRoutes,
    Pods,
    Deployments,
    StatefulSets,
    DaemonSets,
    Jobs,
    CronJobs,
    Kustomizations,
    HelmReleases,
    Sources,
    Services,
    Ingresses,
    ConfigMaps,
    Secrets,
    Pvcs,
}

impl Page {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Problems => "Problems",
            Self::GitOps => "GitOps",
            Self::Changes => "Changes",
            Self::ClusterOverview => "Overview",
            Self::Nodes => "Nodes",
            Self::Namespaces => "Namespaces",
            Self::Events => "Events",
            #[cfg(feature = "loco")]
            Self::LocoPlatform => "Platform",
            #[cfg(feature = "loco")]
            Self::LocoApplications => "Applications",
            #[cfg(feature = "loco")]
            Self::LocoBuilds => "Builds",
            #[cfg(feature = "loco")]
            Self::LocoRoutes => "Routes",
            Self::Pods => "Pods",
            Self::Deployments => "Deployments",
            Self::StatefulSets => "StatefulSets",
            Self::DaemonSets => "DaemonSets",
            Self::Jobs => "Jobs",
            Self::CronJobs => "CronJobs",
            Self::Kustomizations => "Kustomizations",
            Self::HelmReleases => "HelmReleases",
            Self::Sources => "Sources",
            Self::Services => "Services",
            Self::Ingresses => "Ingresses",
            Self::ConfigMaps => "ConfigMaps",
            Self::Secrets => "Secrets",
            Self::Pvcs => "PVCs",
        }
    }

    pub const fn is_fleet(self) -> bool {
        matches!(
            self,
            Self::Overview | Self::Problems | Self::GitOps | Self::Changes
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavSection {
    pub label: Option<&'static str>,
    pub pages: &'static [Page],
}

pub const FLEET_NAV: &[NavSection] = &[NavSection {
    label: None,
    pages: &[Page::Overview, Page::Problems, Page::GitOps, Page::Changes],
}];

pub const CLUSTER_NAV: &[NavSection] = &[
    NavSection {
        label: None,
        pages: &[
            Page::ClusterOverview,
            Page::Nodes,
            Page::Namespaces,
            Page::Events,
        ],
    },
    #[cfg(feature = "loco")]
    NavSection {
        label: Some("Loco"),
        pages: &[
            Page::LocoPlatform,
            Page::LocoApplications,
            Page::LocoBuilds,
            Page::LocoRoutes,
        ],
    },
    NavSection {
        label: Some("Workloads"),
        pages: &[
            Page::Pods,
            Page::Deployments,
            Page::StatefulSets,
            Page::DaemonSets,
            Page::Jobs,
            Page::CronJobs,
        ],
    },
    NavSection {
        label: Some("Flux"),
        pages: &[Page::Kustomizations, Page::HelmReleases, Page::Sources],
    },
    NavSection {
        label: Some("Network"),
        pages: &[Page::Services, Page::Ingresses],
    },
    NavSection {
        label: Some("Config"),
        pages: &[Page::ConfigMaps, Page::Secrets],
    },
    NavSection {
        label: Some("Storage"),
        pages: &[Page::Pvcs],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fleet_nav_holds_only_fleet_pages() {
        let fleet_pages = FLEET_NAV.iter().flat_map(|s| s.pages.iter());
        assert!(fleet_pages.copied().all(Page::is_fleet));
        let cluster_pages = CLUSTER_NAV.iter().flat_map(|s| s.pages.iter());
        assert!(!cluster_pages.copied().any(Page::is_fleet));
    }

    #[cfg(feature = "loco")]
    #[test]
    fn loco_section_follows_cluster_basics() {
        assert_eq!(CLUSTER_NAV[1].label, Some("Loco"));
    }
}
