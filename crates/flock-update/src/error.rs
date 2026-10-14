use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the key is not a base64 ed25519 key")]
    Key,
    #[error("the signature file is malformed")]
    SignatureFormat,
    #[error("the signature is invalid")]
    Signature,
    #[error("the checksum file is malformed")]
    ChecksumFormat,
    #[error("the checksum does not match")]
    Checksum,
    #[error("no random bytes for a key: {0}")]
    Random(String),
    #[error("{} is missing", .0)]
    MissingEnv(&'static str),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("usage: {0}")]
    Usage(&'static str),
    #[error("not a version: {0}")]
    Version(String),
    #[error("the release list could not be read: {0}")]
    Json(String),
    #[error("{0} is not a GitHub repository URL")]
    Repository(String),
    #[error("could not reach GitHub: {0}")]
    Network(String),
    #[error("GitHub answered HTTP {0}")]
    Http(u16),
    #[error("the download is larger than {0} bytes")]
    TooLarge(u64),
    #[error("release {version} has no {name}")]
    MissingAsset { version: String, name: String },
    #[error("{} is not an app bundle", .0.display())]
    NotABundle(PathBuf),
    #[error("the update is {0}, not Flock")]
    BundleId(String),
    #[error("the update is version {found}, the release says {expected}")]
    BundleVersion { found: String, expected: String },
    #[error("the update ({found}) is not newer than {current}")]
    NotNewer { found: String, current: String },
    #[error("{tool} failed: {detail}")]
    Tool { tool: &'static str, detail: String },
    #[error("the update could not be moved into place: {0}")]
    Swap(String),
    #[error("the update failed ({swap}) and the old copy could not be restored ({rollback}); it is at {}", previous.display())]
    Rollback {
        swap: String,
        rollback: String,
        previous: PathBuf,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn json(error: &serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }

    pub fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| Self::Io { path, source }
    }
}
