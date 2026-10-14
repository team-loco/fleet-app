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
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| Self::Io { path, source }
    }
}
