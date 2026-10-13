use std::io::Read;

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use k8s_openapi::api::core::v1::Secret;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use similar::TextDiff;
use thiserror::Error;

pub const SECRET_TYPE: &str = "helm.sh/release.v1";
const SECRET_PREFIX: &str = "sh.helm.release.v1.";
const MAX_RELEASE_BYTES: u64 = 64 * 1024 * 1024;
const DIFF_CONTEXT_LINES: usize = 3;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Release {
    pub name: String,
    pub namespace: String,
    pub version: i64,
    pub status: String,
    pub chart_name: String,
    pub chart_version: String,
    pub app_version: Option<String>,
    pub first_deployed: Option<DateTime<Utc>>,
    pub last_deployed: Option<DateTime<Utc>>,
    pub description: String,
    pub values: Value,
    pub manifest: String,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("secret is not a Helm release storage secret")]
    InvalidSecret,
    #[error("secret has no release data")]
    MissingData,
    #[error("invalid release base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("invalid compressed release: {0}")]
    Compression(#[from] std::io::Error),
    #[error("release exceeds the decoded size limit")]
    TooLarge,
    #[error("invalid release JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("release metadata does not match the storage secret")]
    MetadataMismatch,
    #[error("history contains releases with different identities or duplicate revisions")]
    InvalidHistory,
}

#[derive(Deserialize)]
struct StoredRelease {
    name: String,
    namespace: String,
    version: i64,
    info: StoredInfo,
    chart: StoredChart,
    #[serde(default = "empty_values")]
    config: Value,
    manifest: String,
}

fn empty_values() -> Value {
    serde_json::json!({})
}

#[derive(Deserialize)]
struct StoredInfo {
    status: String,
    #[serde(default)]
    first_deployed: Option<DateTime<Utc>>,
    #[serde(default)]
    last_deployed: Option<DateTime<Utc>>,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct StoredChart {
    metadata: ChartMetadata,
}

#[derive(Deserialize)]
struct ChartMetadata {
    name: String,
    version: String,
    #[serde(default, rename = "appVersion")]
    app_version: Option<String>,
}

pub fn decode_secret(secret: &Secret) -> Result<Release, StorageError> {
    if secret.type_.as_deref() != Some(SECRET_TYPE) {
        return Err(StorageError::InvalidSecret);
    }
    let name = secret
        .metadata
        .name
        .as_deref()
        .ok_or(StorageError::InvalidSecret)?;
    let suffix = name
        .strip_prefix(SECRET_PREFIX)
        .ok_or(StorageError::InvalidSecret)?;
    let (release_name, revision) = suffix
        .rsplit_once(".v")
        .ok_or(StorageError::InvalidSecret)?;
    let revision: i64 = revision.parse().map_err(|_| StorageError::InvalidSecret)?;
    if release_name.is_empty() || revision <= 0 {
        return Err(StorageError::InvalidSecret);
    }
    let encoded = secret
        .data
        .as_ref()
        .and_then(|data| data.get("release"))
        .ok_or(StorageError::MissingData)?;
    let compressed = STANDARD.decode(&encoded.0)?;
    let decoder = GzDecoder::new(compressed.as_slice());
    let mut bounded = decoder.take(MAX_RELEASE_BYTES + 1);
    let mut json = Vec::new();
    bounded.read_to_end(&mut json)?;
    if json.len() as u64 > MAX_RELEASE_BYTES {
        return Err(StorageError::TooLarge);
    }
    let stored: StoredRelease = serde_json::from_slice(&json)?;
    if stored.name != release_name || stored.version != revision || stored.namespace.is_empty() {
        return Err(StorageError::MetadataMismatch);
    }
    let metadata = stored.chart.metadata;
    Ok(Release {
        name: stored.name,
        namespace: stored.namespace,
        version: stored.version,
        status: stored.info.status,
        chart_name: metadata.name,
        chart_version: metadata.version,
        app_version: metadata.app_version,
        first_deployed: stored.info.first_deployed,
        last_deployed: stored.info.last_deployed,
        description: stored.info.description,
        values: stored.config,
        manifest: stored.manifest,
    })
}

pub fn decode_secret_json(json: &str) -> Result<Release, StorageError> {
    let secret: Secret = serde_json::from_str(json)?;
    decode_secret(&secret)
}

pub fn revision_history<'a>(
    secrets: impl IntoIterator<Item = &'a Secret>,
) -> Result<Vec<Release>, StorageError> {
    let mut history = secrets
        .into_iter()
        .map(decode_secret)
        .collect::<Result<Vec<_>, _>>()?;
    history.sort_by_key(|release| release.version);
    if history.windows(2).any(|pair| {
        let [older, newer] = pair else {
            return false;
        };
        older.name != newer.name
            || older.namespace != newer.namespace
            || older.version == newer.version
    }) {
        return Err(StorageError::InvalidHistory);
    }
    Ok(history)
}

pub fn manifest_diff(older: &Release, newer: &Release) -> String {
    let diff = TextDiff::from_lines(&older.manifest, &newer.manifest);
    let old_label = format!("{}/v{}", older.name, older.version);
    let new_label = format!("{}/v{}", newer.name, newer.version);
    diff.unified_diff()
        .context_radius(DIFF_CONTEXT_LINES)
        .header(&old_label, &new_label)
        .to_string()
}
