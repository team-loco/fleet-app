use crate::{ClusterId, Status};

const DEFAULT_NAMESPACE: &str = "default";
const EXEC: &str = "exec";
const SEPARATOR: &str = " · ";
const AUTO: &str = "auto";
const METRICS_SERVER: &str = "metrics-server";
const PROMETHEUS: &str = "prometheus";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClusterCatalog {
    pub sources: usize,
    pub clusters: Vec<ClusterSettings>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClusterSettings {
    pub cluster: ClusterId,
    pub connection: Connection,
    pub auth_expired: bool,
    pub prometheus: Option<String>,
    pub preferences: Preferences,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub context: String,
    pub server: String,
    pub auth: Auth,
    pub kubeconfig: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Auth {
    Exec(String),
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub namespace_selection: Option<NamespaceSelection>,
    pub default_namespace: Option<String>,
    pub metrics: MetricsSource,
    pub confirm_destructive: bool,
    pub notify: bool,
    pub labels: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MetricsSource {
    #[default]
    Auto,
    MetricsServer,
    Prometheus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterAction {
    Open,
    Reauthenticate,
    Connect,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            namespace_selection: None,
            default_namespace: None,
            metrics: MetricsSource::default(),
            confirm_destructive: true,
            notify: true,
            labels: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NamespaceSelection {
    All,
    Named(String),
}

impl NamespaceSelection {
    pub fn namespace(&self) -> Option<&str> {
        match self {
            Self::All => None,
            Self::Named(namespace) => Some(namespace),
        }
    }
}

impl Preferences {
    pub fn namespace_selection(&self) -> NamespaceSelection {
        self.namespace_selection.clone().unwrap_or_else(|| {
            let namespace = self.default_namespace.as_deref().and_then(parse_namespace);
            NamespaceSelection::Named(namespace.unwrap_or_else(|| DEFAULT_NAMESPACE.into()))
        })
    }
}

impl Auth {
    pub fn label(&self) -> String {
        match self {
            Self::Exec(command) => format!("{EXEC}{SEPARATOR}{command}"),
            Self::Other(label) => label.clone(),
        }
    }

    pub fn program(&self) -> Option<&str> {
        match self {
            Self::Exec(command) => command.split_whitespace().next(),
            Self::Other(_) => None,
        }
    }
}

impl MetricsSource {
    pub const ALL: [Self; 3] = [Self::Auto, Self::MetricsServer, Self::Prometheus];

    pub fn label(self, detected: Option<&str>) -> String {
        let name = match self {
            Self::Auto => AUTO,
            Self::MetricsServer => METRICS_SERVER,
            Self::Prometheus => PROMETHEUS,
        };
        match (self, detected) {
            (Self::Auto, Some(endpoint)) => format!("{name}{SEPARATOR}{endpoint}"),
            (Self::Prometheus, Some(endpoint)) => format!("{name}{SEPARATOR}{endpoint}"),
            _ => name.into(),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let name = text
            .split(SEPARATOR.trim())
            .next()
            .unwrap_or_default()
            .trim();
        if name.is_empty() {
            return Some(Self::Auto);
        }
        Self::ALL.into_iter().find(|source| {
            let label = source.label(None);
            label.eq_ignore_ascii_case(name)
        })
    }
}

impl ClusterAction {
    pub fn for_cluster(status: Status, auth_expired: bool) -> Self {
        if auth_expired {
            Self::Reauthenticate
        } else if status == Status::Offline {
            Self::Connect
        } else {
            Self::Open
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open cluster",
            Self::Reauthenticate => "Re-authenticate",
            Self::Connect => "Connect",
        }
    }
}

impl ClusterSettings {
    pub fn auth_sentence(&self) -> Option<String> {
        if !self.auth_expired {
            return None;
        }
        let sentence = match self.connection.auth.program() {
            Some(program) => format!(
                "Credentials from {program} expired. Re-run your login and Fleet reconnects on its own."
            ),
            None => {
                "Credentials expired. Update the kubeconfig and Fleet reconnects on its own.".into()
            }
        };
        Some(sentence)
    }
}

impl ClusterCatalog {
    pub fn get(&self, cluster: &ClusterId) -> Option<&ClusterSettings> {
        self.clusters.iter().find(|c| &c.cluster == cluster)
    }

    pub fn get_mut(&mut self, cluster: &ClusterId) -> Option<&mut ClusterSettings> {
        self.clusters.iter_mut().find(|c| &c.cluster == cluster)
    }

    pub fn sentence(&self) -> String {
        let sources = match self.sources {
            1 => "1 kubeconfig source".to_owned(),
            n => format!("{n} kubeconfig sources"),
        };
        format!(
            "Discovered from {sources}. Files are watched; new contexts appear here automatically."
        )
    }
}

pub fn parse_labels(text: &str) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    let words = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty());
    for word in words {
        if !labels.iter().any(|l| l == word) {
            labels.push(word.to_owned());
        }
    }
    labels
}

pub fn format_labels(labels: &[String]) -> String {
    labels.join(" ")
}

pub fn parse_namespace(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(auth: Auth, auth_expired: bool) -> ClusterSettings {
        ClusterSettings {
            cluster: ClusterId::new("qa-gke"),
            connection: Connection {
                context: "gke_qa".into(),
                server: "https://[IP]".into(),
                auth,
                kubeconfig: "~/.kube/config".into(),
            },
            auth_expired,
            prometheus: None,
            preferences: Preferences::default(),
        }
    }

    #[test]
    fn namespace_memory_overrides_preferred_default_and_keeps_all_explicit() {
        let mut preferences = Preferences::default();
        assert_eq!(
            preferences.namespace_selection().namespace(),
            Some("default")
        );
        preferences.default_namespace = Some("payments".into());
        assert_eq!(
            preferences.namespace_selection().namespace(),
            Some("payments")
        );
        preferences.namespace_selection = Some(NamespaceSelection::All);
        assert_eq!(preferences.namespace_selection().namespace(), None);
        preferences.namespace_selection = Some(NamespaceSelection::Named("web".into()));
        assert_eq!(preferences.namespace_selection().namespace(), Some("web"));
    }

    #[test]
    fn auth_labels_follow_the_design() {
        let exec = Auth::Exec("aws eks get-token".into());
        assert_eq!(exec.label(), "exec · aws eks get-token");
        assert_eq!(exec.program(), Some("aws"));
        let certificate = Auth::Other("client certificate".into());
        assert_eq!(certificate.label(), "client certificate");
        assert_eq!(certificate.program(), None);
    }

    #[test]
    fn expired_auth_names_the_plugin() {
        let expired = settings(Auth::Exec("gke-gcloud-auth-plugin".into()), true);
        assert_eq!(
            expired.auth_sentence().as_deref(),
            Some(
                "Credentials from gke-gcloud-auth-plugin expired. Re-run your login and Fleet reconnects on its own."
            )
        );
        let fine = settings(Auth::Other("token".into()), false);
        assert_eq!(fine.auth_sentence(), None);
        assert!(
            settings(Auth::Other("token".into()), true)
                .auth_sentence()
                .is_some()
        );
    }

    #[test]
    fn primary_action_depends_on_connection() {
        assert_eq!(
            ClusterAction::for_cluster(Status::Err, true),
            ClusterAction::Reauthenticate
        );
        assert_eq!(
            ClusterAction::for_cluster(Status::Offline, false),
            ClusterAction::Connect
        );
        assert_eq!(
            ClusterAction::for_cluster(Status::Warn, false),
            ClusterAction::Open
        );
    }

    #[test]
    fn metrics_source_parses_its_own_labels() {
        let endpoint = Some("monitoring/prometheus-k8s:9090");
        for source in MetricsSource::ALL {
            assert_eq!(MetricsSource::parse(&source.label(endpoint)), Some(source));
        }
        assert_eq!(
            MetricsSource::parse(" Metrics-Server "),
            Some(MetricsSource::MetricsServer)
        );
        assert_eq!(MetricsSource::parse(""), Some(MetricsSource::Auto));
        assert_eq!(MetricsSource::parse("graphite"), None);
    }

    #[test]
    fn metrics_labels_show_the_detected_endpoint() {
        let endpoint = Some("monitoring/prometheus-k8s:9090");
        assert_eq!(
            MetricsSource::Auto.label(endpoint),
            "auto · monitoring/prometheus-k8s:9090"
        );
        assert_eq!(MetricsSource::Auto.label(None), "auto");
        assert_eq!(
            MetricsSource::MetricsServer.label(endpoint),
            "metrics-server"
        );
    }

    #[test]
    fn labels_parse_from_spaces_or_commas_without_duplicates() {
        assert_eq!(
            parse_labels(" env=prod, region=us-east-1  env=prod local"),
            ["env=prod", "region=us-east-1", "local"]
        );
        assert_eq!(parse_labels("  "), Vec::<String>::new());
        let labels = parse_labels("env=dev local");
        assert_eq!(format_labels(&labels), "env=dev local");
    }

    #[test]
    fn blank_namespace_means_none() {
        assert_eq!(parse_namespace("  "), None);
        assert_eq!(parse_namespace(" payments "), Some("payments".into()));
    }

    #[test]
    fn catalog_sentence_counts_sources() {
        let catalog = ClusterCatalog {
            sources: 3,
            clusters: Vec::new(),
        };
        assert_eq!(
            catalog.sentence(),
            "Discovered from 3 kubeconfig sources. Files are watched; new contexts appear here automatically."
        );
    }
}
