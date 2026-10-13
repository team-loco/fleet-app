use fleet_core::settings::{Auth, ClusterSettings, Connection, MetricsSource};
use fleet_core::{ClusterCatalog, ClusterId, Preferences};

const KUBECONFIG_SOURCES: usize = 3;
const PROMETHEUS: &str = "monitoring/prometheus-k8s:9090";
const DEFAULT_NAMESPACE: &str = "payments";
const AWS: &str = "aws eks get-token";
const GKE: &str = "gke-gcloud-auth-plugin";

struct Seed {
    name: &'static str,
    kubeconfig: &'static str,
    context: &'static str,
    server: &'static str,
    auth: Auth,
    auth_expired: bool,
    prometheus: bool,
    labels: &'static [&'static str],
}

fn settings(seed: Seed) -> ClusterSettings {
    let preferences = Preferences {
        default_namespace: Some(DEFAULT_NAMESPACE.into()),
        metrics: MetricsSource::Auto,
        confirm_destructive: true,
        notify: true,
        labels: seed.labels.iter().map(|l| (*l).to_owned()).collect(),
    };
    ClusterSettings {
        cluster: ClusterId::new(seed.name),
        connection: Connection {
            context: seed.context.into(),
            server: seed.server.into(),
            auth: seed.auth,
            kubeconfig: seed.kubeconfig.into(),
        },
        auth_expired: seed.auth_expired,
        prometheus: seed.prometheus.then(|| PROMETHEUS.into()),
        preferences,
    }
}

pub fn catalog() -> ClusterCatalog {
    let seeds = [
        Seed {
            name: "prod-us-east",
            kubeconfig: "~/.kube/eks/prod.yaml",
            context: "arn:aws:eks:us-east-1:[ACCOUNT]:cluster/prod",
            server: "https://[ID].gr7.us-east-1.eks.amazonaws.com",
            auth: Auth::Exec(AWS.into()),
            auth_expired: false,
            prometheus: true,
            labels: &["env=prod", "region=us-east-1", "team=platform"],
        },
        Seed {
            name: "prod-eu-west",
            kubeconfig: "~/.kube/eks/prod-eu.yaml",
            context: "arn:aws:eks:eu-west-1:[ACCOUNT]:cluster/prod-eu",
            server: "https://[ID].gr7.eu-west-1.eks.amazonaws.com",
            auth: Auth::Exec(AWS.into()),
            auth_expired: false,
            prometheus: true,
            labels: &["env=prod", "region=eu-west-1"],
        },
        Seed {
            name: "staging",
            kubeconfig: "~/.kube/config",
            context: "gke_[PROJECT]_us-central1_staging",
            server: "https://[IP]",
            auth: Auth::Exec(GKE.into()),
            auth_expired: false,
            prometheus: true,
            labels: &["env=staging"],
        },
        Seed {
            name: "dev-kind",
            kubeconfig: "~/.kube/kind-dev.yaml",
            context: "kind-dev",
            server: "https://127.0.0.1:52814",
            auth: Auth::Other("client certificate".into()),
            auth_expired: false,
            prometheus: false,
            labels: &["env=dev", "local"],
        },
        Seed {
            name: "qa-gke",
            kubeconfig: "~/.kube/config",
            context: "gke_[PROJECT]_us-central1_qa",
            server: "https://[IP]",
            auth: Auth::Exec(GKE.into()),
            auth_expired: true,
            prometheus: false,
            labels: &["env=qa"],
        },
        Seed {
            name: "edge-k3s",
            kubeconfig: "~/.kube/config",
            context: "edge-k3s",
            server: "https://10.20.0.4:6443",
            auth: Auth::Other("token".into()),
            auth_expired: false,
            prometheus: false,
            labels: &["env=edge"],
        },
    ];
    ClusterCatalog {
        sources: KUBECONFIG_SOURCES,
        clusters: seeds.into_iter().map(settings).collect(),
    }
}
