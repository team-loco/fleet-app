use std::fmt;
use std::path::Path;

use kube::config::AuthInfo;

const MAX_EXEC_SUMMARY_ARGS: usize = 2;
const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];
const LOCAL_REGION: &str = "local";
const EKS_ARN_PREFIX: &str = "arn:aws:eks:";
const EKS_HOST_SUFFIX: &str = ".eks.amazonaws.com";
const AKS_HOST_SUFFIX: &str = ".azmk8s.io";
const GKE_CONTEXT_PREFIX: &str = "gke_";
const DOKS_CONTEXT_PREFIX: &str = "do-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    Eks,
    Gke,
    Aks,
    Doks,
    Kind,
    K3d,
    K3s,
    Minikube,
    OrbStack,
    DockerDesktop,
    Generic,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Eks => "EKS",
            Self::Gke => "GKE",
            Self::Aks => "AKS",
            Self::Doks => "DOKS",
            Self::Kind => "kind",
            Self::K3d => "k3d",
            Self::K3s => "k3s",
            Self::Minikube => "minikube",
            Self::OrbStack => "OrbStack",
            Self::DockerDesktop => "Docker Desktop",
            Self::Generic => "Kubernetes",
        }
    }

    pub fn is_local(self) -> bool {
        matches!(
            self,
            Self::Kind | Self::K3d | Self::Minikube | Self::OrbStack | Self::DockerDesktop
        )
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl ExecCommand {
    pub fn program(&self) -> &str {
        Path::new(&self.command)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.command)
    }

    pub fn summary(&self) -> String {
        let subcommands = self
            .args
            .iter()
            .take_while(|arg| !arg.starts_with('-'))
            .take(MAX_EXEC_SUMMARY_ARGS);
        std::iter::once(self.program())
            .chain(subcommands.map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn flag_value(&self, flag: &str) -> Option<&str> {
        let prefixed = format!("{flag}=");
        let mut args = self.args.iter();
        while let Some(arg) = args.next() {
            if arg == flag {
                return args.next().map(String::as_str);
            }
            if let Some(value) = arg.strip_prefix(&prefixed) {
                return Some(value);
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthKind {
    Exec(ExecCommand),
    ClientCertificate,
    Token,
    Oidc,
    AuthProvider(String),
    Basic,
    None,
}

impl AuthKind {
    pub fn exec(&self) -> Option<&ExecCommand> {
        match self {
            Self::Exec(exec) => Some(exec),
            _ => None,
        }
    }
}

impl fmt::Display for AuthKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exec(exec) => write!(f, "exec · {}", exec.summary()),
            Self::ClientCertificate => f.write_str("client certificate"),
            Self::Token => f.write_str("token"),
            Self::Oidc => f.write_str("oidc"),
            Self::AuthProvider(name) => write!(f, "auth-provider · {name}"),
            Self::Basic => f.write_str("basic"),
            Self::None => f.write_str("none"),
        }
    }
}

pub fn detect_auth(auth: Option<&AuthInfo>) -> AuthKind {
    let Some(auth) = auth else {
        return AuthKind::None;
    };
    if let Some(exec) = &auth.exec {
        let command = ExecCommand {
            command: exec.command.clone().unwrap_or_default(),
            args: exec.args.clone().unwrap_or_default(),
        };
        if command.args.iter().any(|arg| arg == "oidc-login") {
            return AuthKind::Oidc;
        }
        return AuthKind::Exec(command);
    }
    if let Some(provider) = &auth.auth_provider {
        if provider.name == "oidc" {
            return AuthKind::Oidc;
        }
        return AuthKind::AuthProvider(provider.name.clone());
    }
    if auth.client_certificate.is_some() || auth.client_certificate_data.is_some() {
        return AuthKind::ClientCertificate;
    }
    if auth.token.is_some() || auth.token_file.is_some() {
        return AuthKind::Token;
    }
    if auth.username.is_some() && auth.password.is_some() {
        return AuthKind::Basic;
    }
    AuthKind::None
}

fn server_host(server: Option<&str>) -> Option<String> {
    let uri: http::Uri = server?.parse().ok()?;
    uri.host()
        .map(|host| host.trim_matches(['[', ']']).to_ascii_lowercase())
}

fn provider_from_exec(exec: &ExecCommand) -> Option<Provider> {
    match exec.program() {
        "aws" if exec.args.iter().any(|arg| arg == "eks") => Some(Provider::Eks),
        "aws-iam-authenticator" => Some(Provider::Eks),
        "gke-gcloud-auth-plugin" => Some(Provider::Gke),
        "kubelogin" => Some(Provider::Aks),
        "doctl" => Some(Provider::Doks),
        _ => None,
    }
}

fn provider_from_host(host: &str) -> Option<Provider> {
    if host.ends_with(".eks.amazonaws.com") {
        return Some(Provider::Eks);
    }
    if host.ends_with(".azmk8s.io") {
        return Some(Provider::Aks);
    }
    if host.ends_with(".k8s.ondigitalocean.com") {
        return Some(Provider::Doks);
    }
    if host == "k8s.orb.local" {
        return Some(Provider::OrbStack);
    }
    if host == "kubernetes.docker.internal" {
        return Some(Provider::DockerDesktop);
    }
    None
}

fn provider_from_context(context: &str) -> Option<Provider> {
    if context.starts_with("kind-") {
        return Some(Provider::Kind);
    }
    if context.starts_with("k3d-") {
        return Some(Provider::K3d);
    }
    if context.starts_with("gke_") {
        return Some(Provider::Gke);
    }
    if context.starts_with("arn:aws:eks:") {
        return Some(Provider::Eks);
    }
    if context.starts_with("do-") {
        return Some(Provider::Doks);
    }
    match context {
        "minikube" => Some(Provider::Minikube),
        "orbstack" => Some(Provider::OrbStack),
        "docker-desktop" | "docker-for-desktop" => Some(Provider::DockerDesktop),
        _ => None,
    }
}

pub fn detect_provider(context: &str, server: Option<&str>, auth: &AuthKind) -> Provider {
    let host = server_host(server);
    let by_host = host.as_deref().and_then(provider_from_host);
    let by_exec = auth.exec().and_then(provider_from_exec);
    let by_auth_provider = match auth {
        AuthKind::AuthProvider(name) if name == "gcp" => Some(Provider::Gke),
        AuthKind::AuthProvider(name) if name == "azure" => Some(Provider::Aks),
        _ => None,
    };
    by_host
        .or(by_exec)
        .or(by_auth_provider)
        .or_else(|| provider_from_context(context))
        .unwrap_or(Provider::Generic)
}

pub fn is_loopback(server: Option<&str>) -> bool {
    server_host(server).is_some_and(|host| LOOPBACK_HOSTS.contains(&host.as_str()))
}

fn eks_region(context: &str, host: Option<&str>) -> Option<String> {
    if let Some(rest) = context.strip_prefix(EKS_ARN_PREFIX) {
        return rest.split(':').next().map(str::to_owned);
    }
    let stem = host?.strip_suffix(EKS_HOST_SUFFIX)?;
    stem.rsplit('.').next().map(str::to_owned)
}

pub fn region(provider: Provider, context: &str, server: Option<&str>) -> String {
    let host = server_host(server);
    let found = match provider {
        Provider::Eks => eks_region(context, host.as_deref()),
        Provider::Gke => context
            .strip_prefix(GKE_CONTEXT_PREFIX)
            .and_then(|rest| rest.split('_').nth(1))
            .map(str::to_owned),
        Provider::Aks => host
            .as_deref()
            .and_then(|host| host.strip_suffix(AKS_HOST_SUFFIX))
            .and_then(|stem| stem.rsplit('.').next())
            .map(str::to_owned),
        Provider::Doks => context
            .strip_prefix(DOKS_CONTEXT_PREFIX)
            .and_then(|rest| rest.split('-').next())
            .map(str::to_owned),
        _ if provider.is_local() || is_loopback(server) => Some(LOCAL_REGION.to_owned()),
        _ => None,
    };
    found.unwrap_or_default()
}

pub fn refine_provider(
    provider: Provider,
    git_version: Option<&str>,
    node_provider_id: Option<&str>,
) -> Provider {
    if provider != Provider::Generic {
        return provider;
    }
    if let Some(id) = node_provider_id {
        let by_node = [
            ("kind://", Provider::Kind),
            ("k3s://", Provider::K3s),
            ("aws://", Provider::Eks),
            ("gce://", Provider::Gke),
            ("azure://", Provider::Aks),
            ("digitalocean://", Provider::Doks),
        ]
        .into_iter()
        .find(|(prefix, _)| id.starts_with(prefix))
        .map(|(_, provider)| provider);
        if let Some(found) = by_node {
            return found;
        }
    }
    if let Some(version) = git_version {
        if version.contains("+k3s") {
            return Provider::K3s;
        }
        if version.contains("-eks-") {
            return Provider::Eks;
        }
        if version.contains("-gke.") {
            return Provider::Gke;
        }
    }
    provider
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(command: &str, args: &[&str]) -> AuthKind {
        AuthKind::Exec(ExecCommand {
            command: command.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        })
    }

    #[test]
    fn eks_from_server_and_exec() {
        let auth = exec(
            "aws",
            &[
                "--region",
                "us-west-2",
                "eks",
                "get-token",
                "--cluster-name",
                "sandbox",
            ],
        );
        assert_eq!(
            detect_provider(
                "sandbox",
                Some("https://ABC.gr7.us-west-2.eks.amazonaws.com"),
                &AuthKind::Token
            ),
            Provider::Eks
        );
        assert_eq!(
            detect_provider("sandbox", Some("https://10.0.0.1"), &auth),
            Provider::Eks
        );
    }

    #[test]
    fn gke_aks_doks() {
        let gke = exec("/opt/homebrew/bin/gke-gcloud-auth-plugin", &[]);
        assert_eq!(
            detect_provider("staging", Some("https://34.118.0.1"), &gke),
            Provider::Gke
        );
        assert_eq!(
            detect_provider(
                "analytics-aks",
                Some("https://x.hcp.westus2.azmk8s.io:443"),
                &AuthKind::Token
            ),
            Provider::Aks
        );
        assert_eq!(
            detect_provider("ctx", None, &exec("kubelogin", &["get-token"])),
            Provider::Aks
        );
        assert_eq!(
            detect_provider(
                "do-nyc1-x",
                Some("https://abc.k8s.ondigitalocean.com"),
                &exec(
                    "doctl",
                    &["kubernetes", "cluster", "kubeconfig", "exec-credential"]
                )
            ),
            Provider::Doks
        );
    }

    #[test]
    fn local_distros() {
        let cert = AuthKind::ClientCertificate;
        assert_eq!(
            detect_provider("kind-fleet-a", Some("https://127.0.0.1:52814"), &cert),
            Provider::Kind
        );
        assert_eq!(
            detect_provider("k3d-dev", Some("https://0.0.0.0:6550"), &cert),
            Provider::K3d
        );
        assert_eq!(
            detect_provider("minikube", Some("https://127.0.0.1:52011"), &cert),
            Provider::Minikube
        );
        assert_eq!(
            detect_provider("orbstack", Some("https://127.0.0.1:26443"), &cert),
            Provider::OrbStack
        );
        assert_eq!(
            detect_provider(
                "desktop",
                Some("https://kubernetes.docker.internal:6443"),
                &cert
            ),
            Provider::DockerDesktop
        );
        assert_eq!(
            detect_provider("edge", Some("https://10.20.0.4:6443"), &AuthKind::Token),
            Provider::Generic
        );
    }

    #[test]
    fn refine_uses_nodes_then_version() {
        assert_eq!(
            refine_provider(
                Provider::Generic,
                None,
                Some("kind://docker/fleet/fleet-control-plane")
            ),
            Provider::Kind
        );
        assert_eq!(
            refine_provider(Provider::Generic, Some("v1.29.8+k3s1"), None),
            Provider::K3s
        );
        assert_eq!(
            refine_provider(Provider::Generic, Some("v1.30.5-gke.1014"), None),
            Provider::Gke
        );
        assert_eq!(
            refine_provider(Provider::K3d, Some("v1.29.8+k3s1"), None),
            Provider::K3d
        );
        assert_eq!(
            refine_provider(Provider::Generic, Some("v1.31.0"), None),
            Provider::Generic
        );
    }

    #[test]
    fn regions() {
        assert_eq!(
            region(
                Provider::Eks,
                "arn:aws:eks:us-west-2:123:cluster/sandbox",
                None
            ),
            "us-west-2"
        );
        assert_eq!(
            region(
                Provider::Eks,
                "prod",
                Some("https://ABC.gr7.eu-west-1.eks.amazonaws.com")
            ),
            "eu-west-1"
        );
        assert_eq!(
            region(Provider::Gke, "gke_acme-data_us-central1_analytics", None),
            "us-central1"
        );
        assert_eq!(
            region(
                Provider::Aks,
                "analytics-aks",
                Some("https://x.hcp.westus2.azmk8s.io:443")
            ),
            "westus2"
        );
        assert_eq!(region(Provider::Doks, "do-nyc1-example", None), "nyc1");
        assert_eq!(region(Provider::Kind, "kind-fleet-a", None), "local");
        assert_eq!(
            region(Provider::Generic, "edge", Some("https://127.0.0.1:6443")),
            "local"
        );
        assert_eq!(
            region(Provider::Generic, "edge", Some("https://10.0.0.4")),
            ""
        );
    }

    #[test]
    fn auth_labels() {
        let aws = exec(
            "aws",
            &[
                "eks",
                "get-token",
                "--cluster-name",
                "prod",
                "--profile",
                "prod",
            ],
        );
        assert_eq!(aws.to_string(), "exec · aws eks get-token");
        assert_eq!(
            exec("gke-gcloud-auth-plugin", &[]).to_string(),
            "exec · gke-gcloud-auth-plugin"
        );
        assert_eq!(
            AuthKind::ClientCertificate.to_string(),
            "client certificate"
        );
        let Some(command) = aws.exec() else {
            panic!("exec auth expected");
        };
        assert_eq!(command.flag_value("--profile"), Some("prod"));
        assert_eq!(command.flag_value("--region"), None);
    }

    #[test]
    fn detect_auth_kinds() {
        let yaml = r"
users:
- name: cert
  user:
    client-certificate-data: AAAA
    client-key-data: AAAA
- name: token
  user:
    token: abc
- name: oidc
  user:
    exec:
      apiVersion: client.authentication.k8s.io/v1beta1
      command: kubectl
      args: [oidc-login, get-token]
- name: provider
  user:
    auth-provider:
      name: gcp
";
        let Ok(config) = kube::config::Kubeconfig::from_yaml(yaml) else {
            panic!("fixture parses");
        };
        let kinds: Vec<AuthKind> = config
            .auth_infos
            .iter()
            .map(|named| detect_auth(named.auth_info.as_ref()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                AuthKind::ClientCertificate,
                AuthKind::Token,
                AuthKind::Oidc,
                AuthKind::AuthProvider("gcp".to_owned())
            ]
        );
        assert!(is_loopback(Some("https://127.0.0.1:6443")));
        assert!(!is_loopback(Some("https://10.0.0.1:6443")));
    }
}
