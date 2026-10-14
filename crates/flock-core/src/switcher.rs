use crate::{ClusterId, ClusterSummary};

pub const ALL_CLUSTERS: &str = "All clusters";
pub const ADD_CLUSTER: &str = "Add cluster…";

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

pub fn rows(clusters: &[ClusterSummary]) -> Vec<Entry> {
    let clusters = clusters
        .iter()
        .map(|cluster| Entry::Cluster(cluster.id.clone()));
    std::iter::once(Entry::AllClusters)
        .chain(clusters)
        .chain(std::iter::once(Entry::AddCluster))
        .collect()
}

pub fn initial_highlight(rows: &[Entry], selected: Option<&ClusterId>) -> Option<usize> {
    let current = Entry::for_selection(selected);
    rows.iter()
        .position(|row| *row == current)
        .or_else(|| (!rows.is_empty()).then_some(0))
}

pub fn label<'a>(entry: &'a Entry, clusters: &'a [ClusterSummary]) -> &'a str {
    match entry {
        Entry::AllClusters => ALL_CLUSTERS,
        Entry::Cluster(id) => clusters
            .iter()
            .find(|cluster| &cluster.id == id)
            .map_or(id.as_str(), |cluster| cluster.name.as_str()),
        Entry::AddCluster => ADD_CLUSTER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flock::tests::sample;

    fn cluster(id: &str) -> Entry {
        Entry::Cluster(ClusterId::new(id))
    }

    #[test]
    fn the_menu_lists_all_clusters_every_cluster_then_add() {
        let rows = rows(&sample().clusters);
        assert_eq!(
            rows,
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
    fn highlight_starts_on_the_current_selection() {
        let listed = rows(&sample().clusters);
        assert_eq!(initial_highlight(&listed, None), Some(0));
        let b = ClusterId::new("b");
        assert_eq!(initial_highlight(&listed, Some(&b)), Some(2));
        let gone = ClusterId::new("gone");
        assert_eq!(initial_highlight(&listed, Some(&gone)), Some(0));
        assert_eq!(initial_highlight(&[], None), None);
    }

    #[test]
    fn labels_name_each_row() {
        let snapshot = sample();
        assert_eq!(label(&Entry::AllClusters, &snapshot.clusters), ALL_CLUSTERS);
        assert_eq!(label(&cluster("b"), &snapshot.clusters), "b");
        assert_eq!(label(&cluster("gone"), &snapshot.clusters), "gone");
        assert_eq!(label(&Entry::AddCluster, &snapshot.clusters), ADD_CLUSTER);
    }
}
