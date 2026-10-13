use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("kubeconfig {path}: {source}")]
    Kubeconfig {
        path: PathBuf,
        #[source]
        source: kube::config::KubeconfigError,
    },
    #[error(transparent)]
    Kube(#[from] kube::Error),
    #[error("watching kubeconfig files: {0}")]
    Notify(#[from] notify::Error),
    #[error("cluster is not connected")]
    NotConnected,
    #[error("{api_version} {kind} is not served by this cluster")]
    UnknownKind { api_version: String, kind: String },
    #[error("YAML: {0}")]
    Yaml(#[from] serde_norway::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("object has no {0}")]
    MissingField(&'static str),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("exec: {0}")]
    Exec(String),
    #[error("Prometheus: {0}")]
    Prometheus(String),
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),
    #[error("svc/{service} has no port {port}")]
    NoServicePort { service: String, port: u16 },
    #[error("svc/{0} has no pod selector")]
    NoSelector(String),
    #[error("no ready pod backs svc/{0}")]
    NoReadyPod(String),
    #[error("pod {pod} has no container port named {name}")]
    NoNamedPort { pod: String, name: String },
    #[error("{action} is not supported for {kind}")]
    Unsupported { action: &'static str, kind: String },
    #[error("revision {0} not found")]
    RevisionNotFound(u64),
    #[error("deployment {0} is paused; resume it before rolling back")]
    Paused(String),
    #[error("Flux object: {0}")]
    Flux(#[from] fleet_flux::ObjectError),
    #[error("Not a kubeconfig: {0}")]
    InvalidKubeconfig(String),
    #[error("The name can only use letters, digits, - _ and ., and cannot start with a dot.")]
    InvalidKubeconfigName,
    #[error("A pasted kubeconfig named {0} already exists.")]
    KubeconfigExists(String),
    #[error("{0} is not a folder.")]
    NotAFolder(PathBuf),
    #[error("debug container {name} did not start: {reason}")]
    DebugContainer { name: String, reason: String },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
