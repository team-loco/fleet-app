use std::time::Duration;

use crate::filter::{StatusFilter, TextFilter};
use crate::overview::{named, plural, verb};
use crate::{Page, Status, Uid};

const READY_COLUMN: &str = "Ready";
const STATUS_COLUMN: &str = "Status";
const RATIO: char = '/';

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceKind {
    Deployment,
    StatefulSet,
    DaemonSet,
    Job,
    CronJob,
    Service,
    Ingress,
    ConfigMap,
    Secret,
    PersistentVolumeClaim,
}

impl ResourceKind {
    pub const ALL: [Self; 10] = [
        Self::Deployment,
        Self::StatefulSet,
        Self::DaemonSet,
        Self::Job,
        Self::CronJob,
        Self::Service,
        Self::Ingress,
        Self::ConfigMap,
        Self::Secret,
        Self::PersistentVolumeClaim,
    ];

    pub const fn page(self) -> Page {
        match self {
            Self::Deployment => Page::Deployments,
            Self::StatefulSet => Page::StatefulSets,
            Self::DaemonSet => Page::DaemonSets,
            Self::Job => Page::Jobs,
            Self::CronJob => Page::CronJobs,
            Self::Service => Page::Services,
            Self::Ingress => Page::Ingresses,
            Self::ConfigMap => Page::ConfigMaps,
            Self::Secret => Page::Secrets,
            Self::PersistentVolumeClaim => Page::Pvcs,
        }
    }

    pub fn for_page(page: Page) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.page() == page)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Deployment => "Deployment",
            Self::StatefulSet => "StatefulSet",
            Self::DaemonSet => "DaemonSet",
            Self::Job => "Job",
            Self::CronJob => "CronJob",
            Self::Service => "Service",
            Self::Ingress => "Ingress",
            Self::ConfigMap => "ConfigMap",
            Self::Secret => "Secret",
            Self::PersistentVolumeClaim => "PersistentVolumeClaim",
        }
    }

    pub const fn columns(self) -> &'static [&'static str] {
        match self {
            Self::Deployment => &["Ready", "Up-to-date", "Available", "Image"],
            Self::StatefulSet => &["Ready", "Service", "Image"],
            Self::DaemonSet => &["Desired", "Ready", "Node selector", "Image"],
            Self::Job => &["Completions", "Duration", "Status"],
            Self::CronJob => &["Schedule", "Suspend", "Active", "Last schedule"],
            Self::Service => &["Type", "Cluster IP", "Ports"],
            Self::Ingress => &["Class", "Hosts", "Address"],
            Self::ConfigMap => &["Keys"],
            Self::Secret => &["Type", "Keys"],
            Self::PersistentVolumeClaim => &["Status", "Capacity", "Storage class", "Volume"],
        }
    }

    pub fn status_cell(self, row: &Resource) -> Option<String> {
        let columns = self.columns();
        let ready = columns.iter().position(|c| *c == READY_COLUMN);
        let ready = ready
            .and_then(|ix| row.cells.get(ix))
            .filter(|cell| cell.contains(RATIO))
            .map(|cell| format!("{cell} ready"));
        let status = || {
            let ix = columns.iter().position(|c| *c == STATUS_COLUMN)?;
            row.cells.get(ix).filter(|cell| !cell.is_empty()).cloned()
        };
        ready.or_else(status)
    }

    pub const fn is_workload(self) -> bool {
        matches!(
            self,
            Self::Deployment | Self::StatefulSet | Self::DaemonSet | Self::Job | Self::CronJob
        )
    }
}

pub const POD_LIST: &str = "Pod";
pub const EVENT_LIST: &str = "Event";

pub fn namespaced_list(page: Page) -> Option<&'static str> {
    match page {
        Page::Pods => Some(POD_LIST),
        Page::Events => Some(EVENT_LIST),
        other => ResourceKind::for_page(other).map(ResourceKind::name),
    }
}

pub fn namespace_choices(
    known: impl IntoIterator<Item = String>,
    chosen: Option<&str>,
) -> Vec<String> {
    let mut names: Vec<String> = known.into_iter().chain(chosen.map(str::to_owned)).collect();
    names.sort();
    names.dedup();
    names
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub namespace: String,
    pub name: String,
    pub uid: Uid,
    pub status: Status,
    pub cells: Vec<String>,
    pub age: Duration,
    pub yaml: String,
}

impl Resource {
    pub fn matches(&self, namespace: Option<&str>, query: &TextFilter) -> bool {
        let in_namespace = namespace.is_none_or(|ns| ns == self.namespace);
        let fields = [self.name.as_str(), self.namespace.as_str()];
        in_namespace && query.matches(Some(&self.namespace), &fields)
    }
}

pub fn filter_resources(
    rows: &[Resource],
    namespace: Option<&str>,
    query: &TextFilter,
) -> Vec<Resource> {
    rows.iter()
        .filter(|row| row.matches(namespace, query))
        .cloned()
        .collect()
}

pub fn filter_counts(rows: &[Resource]) -> Vec<(StatusFilter, usize)> {
    StatusFilter::counts(rows.iter().map(|row| row.status))
}

pub fn sentence(kind: ResourceKind, rows: &[Resource]) -> String {
    let label = kind.page().label();
    if rows.is_empty() {
        return format!("There are no {label} in this cluster.");
    }
    let mut flagged: Vec<&Resource> = rows.iter().filter(|r| r.status != Status::Ok).collect();
    if flagged.is_empty() {
        return format!(
            "Nothing needs attention across {}.",
            plural(rows.len(), kind.name(), label)
        );
    }
    flagged.sort_by_key(|r| std::cmp::Reverse(r.status));
    let described: Vec<String> = flagged
        .iter()
        .map(|row| match kind.status_cell(row) {
            Some(cell) => format!("{} ({cell})", row.name),
            None => row.name.clone(),
        })
        .collect();
    let described: Vec<&str> = described.iter().map(String::as_str).collect();
    format!(
        "{} {} attention.",
        named(&described),
        verb(flagged.len(), "needs", "need")
    )
}

pub fn namespaces_of(rows: &[Resource]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.namespace.clone()).collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn resource(namespace: &str, name: &str, status: Status) -> Resource {
        Resource {
            uid: Uid::default(),
            namespace: namespace.into(),
            name: name.into(),
            status,
            cells: Vec::new(),
            age: Duration::ZERO,
            yaml: String::new(),
        }
    }

    #[test]
    fn every_kind_maps_to_its_own_page() {
        for kind in ResourceKind::ALL {
            assert_eq!(ResourceKind::for_page(kind.page()), Some(kind));
            assert_ne!(kind.columns().len(), 0);
        }
        assert_eq!(ResourceKind::for_page(Page::Nodes), None);
        assert_eq!(namespaced_list(Page::Pods), Some(POD_LIST));
        assert_eq!(namespaced_list(Page::Events), Some(EVENT_LIST));
        assert_eq!(namespaced_list(Page::Secrets), Some("Secret"));
        assert_eq!(namespaced_list(Page::Nodes), None);
        assert_eq!(namespaced_list(Page::Namespaces), None);
    }

    #[test]
    fn filter_matches_namespace_and_text() {
        let rows = vec![
            resource("payments", "checkout-api", Status::Err),
            resource("payments", "payments-worker", Status::Ok),
            resource("web", "web-frontend", Status::Ok),
        ];
        let names = |found: Vec<Resource>| found.into_iter().map(|r| r.name).collect::<Vec<_>>();
        assert_eq!(
            names(filter_resources(
                &rows,
                Some("payments"),
                &TextFilter::default()
            )),
            ["checkout-api", "payments-worker"]
        );
        assert_eq!(
            names(filter_resources(&rows, None, &TextFilter::parse(" WEB "))),
            ["web-frontend"]
        );
        assert_eq!(
            names(filter_resources(&rows, None, &TextFilter::parse("pay"))),
            ["checkout-api", "payments-worker"]
        );
        assert_eq!(
            names(filter_resources(
                &rows,
                Some("web"),
                &TextFilter::parse("checkout")
            )),
            [""; 0]
        );
        assert_eq!(
            names(filter_resources(&rows, None, &TextFilter::parse("ns:web"))),
            ["web-frontend"]
        );
        let counts: Vec<usize> = filter_counts(&rows).into_iter().map(|(_, n)| n).collect();
        assert_eq!(counts, [3, 1]);
    }

    #[test]
    fn namespace_choices_include_the_pick_even_before_it_loads() {
        let known = ["web".to_owned(), "payments".to_owned(), "web".to_owned()];
        assert_eq!(
            namespace_choices(known.clone(), Some("loco-system")),
            ["loco-system", "payments", "web"]
        );
        assert_eq!(namespace_choices(known, None), ["payments", "web"]);
    }

    fn with_cells(mut row: Resource, cells: &[&str]) -> Resource {
        row.cells = cells.iter().map(|c| (*c).to_owned()).collect();
        row
    }

    #[test]
    fn sentence_names_flagged_rows_worst_first_with_their_state() {
        let rows = vec![
            with_cells(
                resource("monitoring", "grafana", Status::Warn),
                &["0/1", "1", "0", "grafana"],
            ),
            with_cells(
                resource("payments", "checkout-api", Status::Err),
                &["2/3", "3", "2", "checkout"],
            ),
            resource("web", "web-frontend", Status::Ok),
        ];
        assert_eq!(
            sentence(ResourceKind::Deployment, &rows),
            "checkout-api (2/3 ready) and grafana (0/1 ready) need attention."
        );
    }

    #[test]
    fn sentence_uses_the_status_column_and_caps_names() {
        let rows: Vec<Resource> = ["a", "b", "c", "d"]
            .into_iter()
            .map(|name| with_cells(resource("ml", name, Status::Err), &["0/1", "4m", "Failed"]))
            .collect();
        assert_eq!(
            sentence(ResourceKind::Job, &rows),
            "a (Failed), b (Failed), c (Failed) and 1 more need attention."
        );
        assert_eq!(
            sentence(ResourceKind::Job, &rows[..1]),
            "a (Failed) needs attention."
        );
    }

    #[test]
    fn sentence_when_nothing_is_wrong() {
        let rows = vec![resource("web", "a", Status::Ok)];
        assert_eq!(
            sentence(ResourceKind::ConfigMap, &rows),
            "Nothing needs attention across 1 ConfigMap."
        );
        assert_eq!(
            sentence(ResourceKind::PersistentVolumeClaim, &[]),
            "There are no PVCs in this cluster."
        );
    }

    #[test]
    fn namespaces_are_sorted_and_unique() {
        let rows = vec![
            resource("web", "a", Status::Ok),
            resource("payments", "b", Status::Ok),
            resource("web", "c", Status::Ok),
        ];
        assert_eq!(namespaces_of(&rows), ["payments", "web"]);
    }
}
