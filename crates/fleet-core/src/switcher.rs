use std::ops::Range;

use crate::fuzzy::fuzzy;
use crate::{ClusterId, ClusterSummary};

pub const ALL_CLUSTERS: &str = "All clusters";
pub const ADD_CLUSTER: &str = "Add cluster";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    AllClusters,
    Cluster(ClusterId),
    AddCluster,
}

impl Entry {
    pub fn for_selection(selected: Option<&ClusterId>) -> Self {
        selected.map_or(Self::AllClusters, |id| Self::Cluster(id.clone()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub entry: Entry,
    pub score: i64,
    pub highlights: Vec<Range<usize>>,
}

fn row(entry: Entry, label: &str, query: &str) -> Option<Row> {
    if query.trim().is_empty() {
        return Some(Row {
            entry,
            score: 0,
            highlights: Vec::new(),
        });
    }
    let found = fuzzy(query, label)?;
    Some(Row {
        entry,
        score: found.score,
        highlights: found.ranges,
    })
}

pub fn rows(clusters: &[ClusterSummary], query: &str) -> Vec<Row> {
    let all = row(Entry::AllClusters, ALL_CLUSTERS, query);
    let clusters = clusters
        .iter()
        .filter_map(|cluster| row(Entry::Cluster(cluster.id.clone()), &cluster.name, query));
    let add = row(Entry::AddCluster, ADD_CLUSTER, query);
    all.into_iter().chain(clusters).chain(add).collect()
}

pub fn initial_highlight(rows: &[Row], selected: Option<&ClusterId>, query: &str) -> Option<usize> {
    if query.trim().is_empty() {
        let current = Entry::for_selection(selected);
        return rows
            .iter()
            .position(|row| row.entry == current)
            .or_else(|| (!rows.is_empty()).then_some(0));
    }
    let best = rows.iter().map(|row| row.score).max()?;
    rows.iter().position(|row| row.score == best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet::tests::sample;

    fn entries(rows: &[Row]) -> Vec<Entry> {
        rows.iter().map(|row| row.entry.clone()).collect()
    }

    fn cluster(id: &str) -> Entry {
        Entry::Cluster(ClusterId::new(id))
    }

    #[test]
    fn empty_query_lists_all_clusters_every_cluster_then_add() {
        let rows = rows(&sample().clusters, "");
        assert_eq!(
            entries(&rows),
            [
                Entry::AllClusters,
                cluster("a"),
                cluster("b"),
                cluster("c"),
                Entry::AddCluster
            ]
        );
    }

    #[test]
    fn typing_filters_by_name_and_marks_the_matched_letters() {
        let mut snapshot = sample();
        snapshot.clusters[1].name = "staging".into();
        let rows = rows(&snapshot.clusters, "stg");
        assert_eq!(entries(&rows), [cluster("b")]);
        assert_eq!(rows[0].highlights, [0..2, 3..4]);
    }

    #[test]
    fn filtering_can_reach_all_clusters_and_add_cluster() {
        let snapshot = sample();
        assert_eq!(
            entries(&rows(&snapshot.clusters, "all")),
            [Entry::AllClusters]
        );
        assert_eq!(
            entries(&rows(&snapshot.clusters, "add")),
            [Entry::AddCluster]
        );
        assert_eq!(rows(&snapshot.clusters, "zzz"), Vec::new());
    }

    #[test]
    fn highlight_starts_on_the_current_selection() {
        let snapshot = sample();
        let listed = rows(&snapshot.clusters, "");
        assert_eq!(initial_highlight(&listed, None, ""), Some(0));
        let b = ClusterId::new("b");
        assert_eq!(initial_highlight(&listed, Some(&b), ""), Some(2));
        let gone = ClusterId::new("gone");
        assert_eq!(initial_highlight(&listed, Some(&gone), ""), Some(0));
    }

    #[test]
    fn highlight_follows_the_best_match_while_filtering() {
        let mut snapshot = sample();
        snapshot.clusters[0].name = "prod-us-east".into();
        snapshot.clusters[1].name = "staging".into();
        snapshot.clusters[2].name = "prod-eu-west".into();
        let listed = rows(&snapshot.clusters, "eu");
        let best = initial_highlight(&listed, None, "eu").map(|ix| listed[ix].entry.clone());
        assert_eq!(best, Some(cluster("c")));
        assert_eq!(initial_highlight(&[], None, "eu"), None);
    }
}
