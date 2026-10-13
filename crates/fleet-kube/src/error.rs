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
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
