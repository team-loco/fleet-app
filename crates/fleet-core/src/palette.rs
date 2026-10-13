use std::cmp::Reverse;
use std::ops::Range;

use crate::fuzzy::fuzzy;
use crate::resource::{kind_info, kind_matches, kind_rank, short_kind};
use crate::{
    CLUSTER_NAV, ClusterId, ClusterSummary, Command, FLEET_NAV, ResourceRef, ResourceSummary,
    Snapshot, Status,
};

const MAX_RESOURCES: usize = 50;
const PRIVILEGED_PORTS: u16 = 1024;
const LOCAL_PORT_OFFSET: u16 = 8000;
const CLUSTER_PREFIX: &str = "@";
const NAMESPACE_PREFIX: &str = "ns:";
const KIND_PREFIX: &str = "k:";
const LABEL_PREFIX: &str = "l:";
const COMMAND_PREFIX: &str = ">";
const FLEET: &str = "Fleet";
const CLUSTER_KIND: &str = "cluster";
const PAGE_KIND: &str = "page";
const LOGS_KEY: &str = "⌘L";
const COMPARE_KEY: &str = "⌘⇧D";
const OWNER_KEY: &str = "⌘⇧G";

const GLOBAL_COMMANDS: &[(&str, &str, Command)] = &[
    ("Toggle clusters panel", "⌘⇧B", Command::ToggleClustersPanel),
    (
        "Toggle resources panel",
        "⌘B",
        Command::ToggleResourcesPanel,
    ),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub cluster: Option<String>,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub label: Option<(String, Option<String>)>,
    pub commands_only: bool,
}

impl Query {
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim_start();
        let (commands_only, rest) = match trimmed.strip_prefix(COMMAND_PREFIX) {
            Some(rest) => (true, rest),
            None => (false, trimmed),
        };
        let mut query = Self {
            commands_only,
            ..Self::default()
        };
        let mut words = Vec::new();
        for token in rest.split_whitespace() {
            if let Some(cluster) = token.strip_prefix(CLUSTER_PREFIX) {
                query.cluster = Some(cluster.to_lowercase());
            } else if let Some(namespace) = token.strip_prefix(NAMESPACE_PREFIX) {
                query.namespace = Some(namespace.to_lowercase());
            } else if let Some(kind) = token.strip_prefix(KIND_PREFIX) {
                query.kind = Some(kind.to_owned());
            } else if let Some(label) = token.strip_prefix(LABEL_PREFIX) {
                query.label = Some(match label.split_once('=') {
                    Some((key, value)) => (key.to_owned(), Some(value.to_owned())),
                    None => (label.to_owned(), None),
                });
            } else {
                words.push(token);
            }
        }
        query.text = words.join(" ");
        query
    }

    fn filters_resources(&self) -> bool {
        self.namespace.is_some() || self.kind.is_some() || self.label.is_some()
    }

    fn in_cluster(&self, name: &str) -> bool {
        self.cluster
            .as_ref()
            .is_none_or(|wanted| name.to_lowercase().contains(wanted))
    }

    fn keeps(&self, summary: &ResourceSummary) -> bool {
        let resource = &summary.resource;
        let namespace_ok = self.namespace.as_ref().is_none_or(|wanted| {
            resource
                .namespace
                .as_ref()
                .is_some_and(|ns| ns.to_lowercase().contains(wanted))
        });
        let kind_ok = self
            .kind
            .as_ref()
            .is_none_or(|wanted| kind_matches(&resource.kind, wanted));
        let label_ok =
            self.label
                .as_ref()
                .is_none_or(|(key, value)| match (summary.labels.get(key), value) {
                    (Some(actual), Some(wanted)) => actual == wanted,
                    (Some(_), None) => true,
                    (None, _) => false,
                });
        namespace_ok && kind_ok && label_ok
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
    Clusters,
    Pages,
    Resources,
    Commands,
}

impl Section {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Clusters => "Clusters",
            Self::Pages => "Pages",
            Self::Resources => "Resources",
            Self::Commands => "Commands",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub section: Section,
    pub kind: String,
    pub name: String,
    pub highlights: Vec<Range<usize>>,
    pub namespace: String,
    pub cluster: String,
    pub status: Option<Status>,
    pub detail: String,
    pub shortcut: Option<&'static str>,
    pub command: Command,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Results {
    pub hits: Vec<Hit>,
    pub clusters: usize,
}

impl Results {
    pub fn summary(&self) -> String {
        let matches = plural(self.hits.len(), "match", "matches");
        let clusters = plural(self.clusters, "cluster", "clusters");
        format!("{matches} · {clusters}")
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    let noun = if n == 1 { one } else { many };
    format!("{n} {noun}")
}

pub fn search(snapshot: &Snapshot, current: Option<&ClusterId>, input: &str) -> Results {
    let query = Query::parse(input);
    let clusters: Vec<&ClusterSummary> = snapshot
        .clusters
        .iter()
        .filter(|c| query.in_cluster(&c.name))
        .collect();
    let mut hits = Vec::new();
    let browsing = !query.commands_only && !query.filters_resources();
    let unscoped = query.cluster.is_none();
    if browsing {
        hits.extend(cluster_hits(&clusters, &query.text));
    }
    if browsing && unscoped {
        hits.extend(page_hits(snapshot, current, &query.text));
    }
    let resources = if query.commands_only {
        Vec::new()
    } else {
        resource_hits(snapshot, &query)
    };
    let contextual = contextual_commands(snapshot, &resources);
    hits.extend(resources.into_iter().map(|(hit, _)| hit));
    hits.extend(contextual);
    if query.commands_only || (browsing && unscoped) {
        hits.extend(global_commands(&query.text));
    }
    Results {
        hits,
        clusters: clusters.len(),
    }
}

fn matched(text: &str, query: &str) -> Option<(i64, Vec<Range<usize>>)> {
    if query.trim().is_empty() {
        return Some((0, Vec::new()));
    }
    fuzzy(query, text).map(|m| (m.score, m.ranges))
}

fn cluster_hits(clusters: &[&ClusterSummary], text: &str) -> Vec<Hit> {
    let mut scored: Vec<(i64, Hit)> = clusters
        .iter()
        .filter_map(|cluster| {
            let (score, highlights) = matched(&cluster.name, text)?;
            let hit = Hit {
                section: Section::Clusters,
                kind: CLUSTER_KIND.into(),
                name: cluster.name.clone(),
                highlights,
                namespace: String::new(),
                cluster: format!("{} · {}", cluster.distro, cluster.region),
                status: Some(cluster.status),
                detail: cluster.state.clone(),
                shortcut: None,
                command: Command::SelectCluster(Some(cluster.id.clone())),
            };
            Some((score, hit))
        })
        .collect();
    scored.sort_by_key(|(score, _)| Reverse(*score));
    scored.into_iter().map(|(_, hit)| hit).collect()
}

fn page_hits(snapshot: &Snapshot, current: Option<&ClusterId>, text: &str) -> Vec<Hit> {
    let fleet_pages = FLEET_NAV
        .iter()
        .flat_map(|section| section.pages.iter())
        .map(|page| (*page, FLEET.to_owned()));
    let cluster_name = current
        .and_then(|id| snapshot.cluster(id))
        .map(|cluster| cluster.name.clone());
    let cluster_pages = cluster_name.into_iter().flat_map(|name| {
        CLUSTER_NAV
            .iter()
            .flat_map(|section| section.pages.iter())
            .map(move |page| (*page, name.clone()))
    });
    let mut scored: Vec<(i64, Hit)> = fleet_pages
        .chain(cluster_pages)
        .filter_map(|(page, scope)| {
            let (score, highlights) = matched(page.label(), text)?;
            let hit = Hit {
                section: Section::Pages,
                kind: PAGE_KIND.into(),
                name: page.label().into(),
                highlights,
                namespace: String::new(),
                cluster: scope,
                status: None,
                detail: String::new(),
                shortcut: None,
                command: Command::SelectPage(page),
            };
            Some((score, hit))
        })
        .collect();
    if text.trim().is_empty() {
        return scored.into_iter().map(|(_, hit)| hit).collect();
    }
    scored.sort_by_key(|(score, _)| Reverse(*score));
    scored.into_iter().map(|(_, hit)| hit).collect()
}

fn resource_hits<'a>(snapshot: &'a Snapshot, query: &Query) -> Vec<(Hit, &'a ResourceSummary)> {
    if query.text.trim().is_empty() && !query.filters_resources() && query.cluster.is_none() {
        return Vec::new();
    }
    let cluster_order = |id: &ClusterId| {
        snapshot
            .clusters
            .iter()
            .position(|c| &c.id == id)
            .unwrap_or(snapshot.clusters.len())
    };
    let mut scored: Vec<_> = snapshot
        .resources
        .iter()
        .filter(|summary| {
            let cluster_name = snapshot
                .cluster(&summary.resource.cluster)
                .map_or(summary.resource.cluster.as_str(), |c| c.name.as_str());
            query.in_cluster(cluster_name) && query.keeps(summary)
        })
        .filter_map(|summary| {
            let (score, highlights) = matched(&summary.resource.name, &query.text)?;
            Some((score, highlights, summary))
        })
        .collect();
    scored.sort_by_key(|(score, _, summary)| {
        let resource = &summary.resource;
        (
            Reverse(*score),
            Reverse(summary.status),
            kind_rank(&resource.kind),
            cluster_order(&resource.cluster),
            resource.name.clone(),
        )
    });
    scored
        .into_iter()
        .take(MAX_RESOURCES)
        .map(|(_, highlights, summary)| (resource_hit(snapshot, summary, highlights), summary))
        .collect()
}

fn resource_hit(
    snapshot: &Snapshot,
    summary: &ResourceSummary,
    highlights: Vec<Range<usize>>,
) -> Hit {
    let resource = &summary.resource;
    let cluster = snapshot
        .cluster(&resource.cluster)
        .map_or_else(|| resource.cluster.to_string(), |c| c.name.clone());
    Hit {
        section: Section::Resources,
        kind: short_kind(&resource.kind),
        name: resource.name.clone(),
        highlights,
        namespace: resource.namespace.clone().unwrap_or_default(),
        cluster,
        status: summary.status,
        detail: summary.detail.clone(),
        shortcut: None,
        command: Command::OpenResource(resource.clone()),
    }
}

fn command_hit(label: String, shortcut: Option<&'static str>, command: Command) -> Hit {
    Hit {
        section: Section::Commands,
        kind: COMMAND_PREFIX.into(),
        name: label,
        highlights: Vec::new(),
        namespace: String::new(),
        cluster: String::new(),
        status: None,
        detail: String::new(),
        shortcut,
        command,
    }
}

fn contextual_commands(snapshot: &Snapshot, resources: &[(Hit, &ResourceSummary)]) -> Vec<Hit> {
    let mut commands = Vec::new();
    let workload = resources
        .iter()
        .find(|(_, summary)| kind_info(&summary.resource.kind).is_some_and(|info| info.has_pods));
    if let Some((hit, summary)) = workload {
        let resource = &summary.resource;
        let target = format!("{}/{}", hit.kind, resource.name);
        commands.push(command_hit(
            format!("Logs: follow all {} pods on {}", resource.name, hit.cluster),
            Some(LOGS_KEY),
            Command::FollowLogs(resource.clone()),
        ));
        if elsewhere(snapshot, resource) {
            commands.push(command_hit(
                format!("Compare {target} across clusters"),
                Some(COMPARE_KEY),
                Command::Compare(resource.clone()),
            ));
        }
        if let Some(owner) = &summary.owner {
            commands.push(command_hit(
                format!("Show Flux owner of {target}"),
                Some(OWNER_KEY),
                Command::OpenResource(owner.clone()),
            ));
        }
    }
    let service = resources.iter().find_map(|(hit, summary)| {
        let is_service = kind_info(&summary.resource.kind).is_some_and(|info| info.short == "svc");
        let port = summary.ports.first().copied().filter(|_| is_service)?;
        Some((hit, summary, port))
    });
    if let Some((hit, summary, remote)) = service {
        let local = local_port(remote);
        let resource = &summary.resource;
        commands.push(command_hit(
            format!(
                "Port-forward {}/{} {local} → {remote}",
                hit.kind, resource.name
            ),
            None,
            Command::PortForward {
                service: resource.clone(),
                local,
                remote,
            },
        ));
    }
    commands
}

fn elsewhere(snapshot: &Snapshot, resource: &ResourceRef) -> bool {
    snapshot.resources.iter().any(|other| {
        let other = &other.resource;
        other.cluster != resource.cluster
            && other.kind == resource.kind
            && other.namespace == resource.namespace
            && other.name == resource.name
    })
}

pub fn local_port(remote: u16) -> u16 {
    if remote < PRIVILEGED_PORTS {
        remote + LOCAL_PORT_OFFSET
    } else {
        remote
    }
}

fn global_commands(text: &str) -> Vec<Hit> {
    let mut scored: Vec<(i64, Hit)> = GLOBAL_COMMANDS
        .iter()
        .filter_map(|(label, key, command)| {
            let (score, highlights) = matched(label, text)?;
            let mut hit = command_hit((*label).to_owned(), Some(key), command.clone());
            hit.highlights = highlights;
            Some((score, hit))
        })
        .collect();
    scored.sort_by_key(|(score, _)| Reverse(*score));
    scored.into_iter().map(|(_, hit)| hit).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::fleet::tests::cluster;
    use crate::{Counts, Page};

    fn resource(cluster: &str, kind: &str, namespace: &str, name: &str) -> ResourceRef {
        ResourceRef {
            cluster: ClusterId::new(cluster),
            kind: kind.into(),
            namespace: Some(namespace.into()),
            name: name.into(),
        }
    }

    fn summary(resource: ResourceRef, status: Option<Status>) -> ResourceSummary {
        ResourceSummary {
            resource,
            status,
            detail: String::new(),
            labels: BTreeMap::new(),
            ports: Vec::new(),
            owner: None,
        }
    }

    fn snapshot() -> Snapshot {
        let owner = resource("a", "Kustomization", "flux-system", "apps");
        let mut deploy_a = summary(
            resource("a", "Deployment", "shop", "cart"),
            Some(Status::Err),
        );
        deploy_a.owner = Some(owner.clone());
        deploy_a.labels.insert("app".into(), "cart".into());
        let mut svc = summary(resource("a", "Service", "shop", "cart"), None);
        svc.ports = vec![80];
        Snapshot {
            clusters: vec![
                cluster("a", Status::Err, Counts::default()),
                cluster("b", Status::Ok, Counts::default()),
            ],
            resources: vec![
                summary(
                    resource("b", "Deployment", "shop", "cart"),
                    Some(Status::Ok),
                ),
                svc,
                summary(resource("a", "Pod", "shop", "cart-7f9c"), Some(Status::Err)),
                deploy_a,
                summary(owner, Some(Status::Err)),
            ],
            ..Snapshot::default()
        }
    }

    fn names(results: &Results) -> Vec<(Section, String, String)> {
        results
            .hits
            .iter()
            .map(|hit| {
                (
                    hit.section,
                    hit.kind.clone(),
                    format!("{} {}", hit.cluster, hit.name),
                )
            })
            .collect()
    }

    #[test]
    fn parses_scopes_out_of_the_query() {
        let query = Query::parse("@Prod ns:pay cart k:deploy l:app=cart");
        assert_eq!(query.text, "cart");
        assert_eq!(query.cluster.as_deref(), Some("prod"));
        assert_eq!(query.namespace.as_deref(), Some("pay"));
        assert_eq!(query.kind.as_deref(), Some("deploy"));
        assert_eq!(query.label, Some(("app".into(), Some("cart".into()))));
        assert!(!query.commands_only);
        let commands = Query::parse(">toggle res");
        assert!(commands.commands_only);
        assert_eq!(commands.text, "toggle res");
    }

    #[test]
    fn resources_rank_problems_then_owners_then_cluster_order() {
        let results = search(&snapshot(), None, "cart");
        let resources: Vec<_> = names(&results)
            .into_iter()
            .filter(|(section, ..)| *section == Section::Resources)
            .map(|(_, kind, name)| format!("{kind} {name}"))
            .collect();
        assert_eq!(
            resources,
            [
                "deploy a cart",
                "pod a cart-7f9c",
                "deploy b cart",
                "svc a cart"
            ]
        );
    }

    #[test]
    fn workload_and_service_hits_suggest_commands() {
        let results = search(&snapshot(), None, "cart");
        let commands: Vec<_> = results
            .hits
            .iter()
            .filter(|hit| hit.section == Section::Commands)
            .map(|hit| (hit.name.as_str(), hit.command.clone()))
            .collect();
        let deploy = resource("a", "Deployment", "shop", "cart");
        assert_eq!(
            commands,
            [
                (
                    "Logs: follow all cart pods on a",
                    Command::FollowLogs(deploy.clone())
                ),
                (
                    "Compare deploy/cart across clusters",
                    Command::Compare(deploy)
                ),
                (
                    "Show Flux owner of deploy/cart",
                    Command::OpenResource(resource("a", "Kustomization", "flux-system", "apps"))
                ),
                (
                    "Port-forward svc/cart 8080 → 80",
                    Command::PortForward {
                        service: resource("a", "Service", "shop", "cart"),
                        local: 8080,
                        remote: 80
                    }
                ),
            ]
        );
    }

    #[test]
    fn scopes_filter_resources() {
        let snapshot = snapshot();
        let in_b = search(&snapshot, None, "@b cart");
        assert_eq!(in_b.clusters, 1);
        let scoped = search(&snapshot, None, "@b");
        let sections: Vec<_> = scoped.hits.iter().map(|hit| hit.section).collect();
        assert_eq!(
            sections,
            [
                Section::Clusters,
                Section::Resources,
                Section::Commands,
                Section::Commands
            ]
        );
        assert!(
            in_b.hits
                .iter()
                .all(|hit| hit.section != Section::Resources || hit.cluster == "b")
        );
        let pods = search(&snapshot, None, "k:pod");
        let kinds: Vec<_> = pods.hits.iter().map(|hit| hit.kind.as_str()).collect();
        assert_eq!(kinds, ["pod"]);
        let labelled = search(&snapshot, None, "l:app=cart");
        assert_eq!(labelled.hits.len(), 4);
        let flux = search(&snapshot, None, "ns:flux");
        assert_eq!(flux.hits.first().map(|hit| hit.name.as_str()), Some("apps"));
    }

    #[test]
    fn empty_query_browses_clusters_pages_and_commands() {
        let snapshot = snapshot();
        let results = search(&snapshot, None, "");
        let sections: Vec<_> = results.hits.iter().map(|hit| hit.section).collect();
        assert!(!sections.contains(&Section::Resources));
        assert_eq!(sections.first(), Some(&Section::Clusters));
        let fleet_pages = FLEET_NAV.iter().map(|s| s.pages.len()).sum::<usize>();
        let pages = sections.iter().filter(|s| **s == Section::Pages).count();
        assert_eq!(pages, fleet_pages);
        let in_cluster = search(&snapshot, Some(&ClusterId::new("a")), "pods");
        let pods = in_cluster.hits.first().map(|hit| hit.command.clone());
        assert_eq!(pods, Some(Command::SelectPage(Page::Pods)));
    }

    #[test]
    fn command_scope_only_lists_commands() {
        let results = search(&snapshot(), None, ">res");
        let names: Vec<_> = results.hits.iter().map(|hit| hit.name.as_str()).collect();
        assert_eq!(names, ["Toggle resources panel"]);
    }

    #[test]
    fn summary_counts_matches_and_clusters() {
        let results = search(&snapshot(), None, "cart");
        assert_eq!(results.summary(), "8 matches · 2 clusters");
        let one = Results {
            hits: results.hits.into_iter().take(1).collect(),
            clusters: 1,
        };
        assert_eq!(one.summary(), "1 match · 1 cluster");
    }

    #[test]
    fn privileged_ports_forward_to_a_high_local_port() {
        assert_eq!(local_port(80), 8080);
        assert_eq!(local_port(8443), 8443);
    }
}
