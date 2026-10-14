use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use flock_core::ClusterId;
use jiff::Timestamp;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::store::{APP_ID, CACHES};
use crate::usage::UsageRing;

pub const SAVE_INTERVAL: Duration = Duration::from_secs(60);
const DIRECTORY: &str = "metrics";
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub fn cache_dir(home: &Path) -> PathBuf {
    home.join(CACHES).join(APP_ID).join(DIRECTORY)
}

pub fn path(dir: &Path, id: &ClusterId) -> PathBuf {
    let digest = Sha256::digest(id.as_str().as_bytes());
    let mut name = String::new();
    for byte in digest {
        let _ = write!(name, "{byte:02x}");
    }
    name.push_str(".json");
    dir.join(name)
}

pub fn load(path: &Path, now: Timestamp) -> UsageRing {
    match read(path, now) {
        Ok(ring) => ring,
        Err(error) => {
            if !matches!(&error, Error::Io(error) if error.kind() == std::io::ErrorKind::NotFound) {
                tracing::warn!(%error, path = %path.display(), "ignoring corrupt metrics history");
            }
            UsageRing::default()
        }
    }
}

fn read(path: &Path, now: Timestamp) -> Result<UsageRing> {
    if std::fs::metadata(path)?.len() > MAX_FILE_BYTES {
        return Err(Error::MetricsHistory("file exceeds the size limit"));
    }
    let bytes = std::fs::read(path)?;
    let mut ring: UsageRing = serde_json::from_slice(&bytes)?;
    if !ring.valid() {
        return Err(Error::MetricsHistory("invalid samples"));
    }
    ring.prune(now);
    Ok(ring)
}

pub fn save(path: &Path, ring: &UsageRing) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec(ring)?;
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::Sample;

    #[test]
    fn persistence_round_trip_and_corrupt_files() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = path(directory.path(), &ClusterId::new("../../cluster"));
        assert_eq!(path.parent(), Some(directory.path()));
        let now = Timestamp::now();
        let mut ring = UsageRing::default();
        ring.push(
            now,
            Sample {
                cpu: 12.,
                memory: 34.,
            },
        );
        save(&path, &ring)?;
        assert_eq!(load(&path, now), ring);
        std::fs::write(&path, b"broken")?;
        assert_eq!(load(&path, now).latest(), None);
        std::fs::write(&path, br#"[{"at":"2026-10-14T16:00:00Z","sample":{"cpu":1,"memory":1}},{"at":"2026-10-14T15:00:00Z","sample":{"cpu":1,"memory":1}}]"#)?;
        assert_eq!(load(&path, now).latest(), None);
        Ok(())
    }
}
