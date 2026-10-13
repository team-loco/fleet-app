use std::collections::BTreeMap;

use fleet_core::{ClusterId, Counts, NavSection, Page, Snapshot, Status};

const ALL_CLUSTERS_GLYPH: &str = "◎";
const ALL_CLUSTERS_TOOLTIP: &str = "All clusters";
const TOOLTIP_SEPARATOR: &str = " · ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ClusterStripRow {
    pub target: Option<ClusterId>,
    pub glyph: &'static str,
    pub status: Option<Status>,
    pub tooltip: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResourceStripRow {
    pub page: Page,
    pub status: Status,
    pub count: u32,
    pub tooltip: &'static str,
    pub selected: bool,
}

pub(super) fn cluster_strip_rows(
    snapshot: &Snapshot,
    selected: Option<&ClusterId>,
) -> Vec<ClusterStripRow> {
    let all = ClusterStripRow {
        target: None,
        glyph: ALL_CLUSTERS_GLYPH,
        status: None,
        tooltip: ALL_CLUSTERS_TOOLTIP.to_owned(),
        selected: selected.is_none(),
    };
    let clusters = snapshot.clusters.iter().map(|cluster| ClusterStripRow {
        target: Some(cluster.id.clone()),
        glyph: cluster.status.glyph(),
        status: Some(cluster.status),
        tooltip: format!("{}{TOOLTIP_SEPARATOR}{}", cluster.name, cluster.state),
        selected: selected == Some(&cluster.id),
    });
    std::iter::once(all).chain(clusters).collect()
}

pub(super) fn resource_strip_rows(
    sections: &[NavSection],
    counts: &BTreeMap<Page, Counts>,
    current: Page,
) -> Vec<ResourceStripRow> {
    sections
        .iter()
        .flat_map(|section| section.pages.iter().copied())
        .filter_map(|page| {
            let (status, count) = counts.get(&page)?.badges().into_iter().next()?;
            Some(ResourceStripRow {
                page,
                status,
                count,
                tooltip: page.label(),
                selected: page == current,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use fleet_core::{CLUSTER_NAV, ClusterSummary};

    use super::*;

    fn cluster(id: &str, status: Status, state: &str) -> ClusterSummary {
        ClusterSummary {
            id: ClusterId::new(id),
            name: id.to_owned(),
            status,
            state: state.to_owned(),
            distro: String::new(),
            region: String::new(),
            version: None,
            nodes: None,
            problems: Counts::default(),
            nav: BTreeMap::new(),
            metrics: None,
        }
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            clusters: vec![
                cluster("a", Status::Ok, "Healthy"),
                cluster("b", Status::Offline, "Auth expired"),
            ],
            ..Snapshot::default()
        }
    }

    #[test]
    fn cluster_rows_lead_with_all_clusters_then_one_glyph_per_cluster() {
        let rows = cluster_strip_rows(&snapshot(), None);
        let glyphs: Vec<_> = rows.iter().map(|row| row.glyph).collect();
        assert_eq!(glyphs, [ALL_CLUSTERS_GLYPH, "●", "○"]);
        assert_eq!(rows[0].target, None);
        assert_eq!(rows[1].target, Some(ClusterId::new("a")));
        assert_eq!(rows[2].status, Some(Status::Offline));
    }

    #[test]
    fn cluster_rows_name_state_in_tooltip_and_mark_selection() {
        let selected = ClusterId::new("b");
        let rows = cluster_strip_rows(&snapshot(), Some(&selected));
        assert_eq!(rows[1].tooltip, "a · Healthy");
        assert_eq!(rows[2].tooltip, "b · Auth expired");
        assert_eq!(rows[0].tooltip, "All clusters");
        let marked: Vec<_> = rows.iter().map(|row| row.selected).collect();
        assert_eq!(marked, [false, false, true]);
    }

    #[test]
    fn all_clusters_is_selected_in_fleet_mode() {
        let rows = cluster_strip_rows(&snapshot(), None);
        let marked: Vec<_> = rows.iter().map(|row| row.selected).collect();
        assert_eq!(marked, [true, false, false]);
    }

    #[test]
    fn resource_rows_keep_only_pages_with_problems_worst_first() {
        let counts = BTreeMap::from([
            (Page::Pods, Counts::total(40)),
            (Page::Deployments, Counts::total(9).with_warn(2).with_err(1)),
            (Page::Nodes, Counts::total(3).with_sus(4)),
        ]);
        let rows = resource_strip_rows(CLUSTER_NAV, &counts, Page::Nodes);
        let summary: Vec<_> = rows
            .iter()
            .map(|row| (row.page, row.status, row.count, row.selected))
            .collect();
        assert_eq!(
            summary,
            [
                (Page::Nodes, Status::Suspended, 4, true),
                (Page::Deployments, Status::Err, 1, false),
            ]
        );
        assert_eq!(rows[1].tooltip, Page::Deployments.label());
    }

    #[test]
    fn resource_rows_are_empty_without_problems() {
        let counts = BTreeMap::from([(Page::Pods, Counts::total(40))]);
        let rows = resource_strip_rows(CLUSTER_NAV, &counts, Page::Pods);
        assert_eq!(rows, Vec::new());
    }
}
