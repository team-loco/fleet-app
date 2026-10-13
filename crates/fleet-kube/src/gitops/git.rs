use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fleet_core::changes::{FilePatch, LineChange, PatchLine};
use fleet_flux::redact;
use jiff::Timestamp;
use tokio::process::Command;
use tokio::sync::watch;

use crate::manager::lock;
use crate::task::TaskGuard;

const CACHE_DIR: &str = "Library/Caches/dev.fleet.app/git";
const FETCH_INTERVAL: Duration = Duration::from_mins(5);
const COMMIT_LIMIT: usize = 30;
const MAX_FILES: usize = 50;
const MAX_FILE_LINES: usize = 400;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const RECORD: char = '\u{1e}';
const FIELD: char = '\u{1f}';
const LOG_FORMAT: &str = "--format=%x1e%H%x1f%an%x1f%at%x1f%s";
const GIT: &str = "git";
const NO_HOME: &str = "no home directory for the git cache";
const STDERR_FALLBACK: &str = "git exited unsuccessfully";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RepoKey {
    pub url: String,
    pub branch: String,
}

impl RepoKey {
    pub fn name(&self) -> String {
        repo_name(&self.url)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommit {
    pub sha: String,
    pub author: String,
    pub time: Timestamp,
    pub subject: String,
    pub files: Vec<FilePatch>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MirrorState {
    Fetching,
    Ready {
        commits: Vec<GitCommit>,
        fetched: Timestamp,
    },
    Unavailable(String),
}

type Wanted = BTreeMap<RepoKey, BTreeSet<String>>;

pub struct Mirrors {
    states: Arc<Mutex<BTreeMap<RepoKey, MirrorState>>>,
    wanted: watch::Sender<Wanted>,
    _task: TaskGuard,
}

impl Mirrors {
    pub fn start(timeout: Duration, changed: async_channel::Sender<()>) -> Self {
        let states = Arc::new(Mutex::new(BTreeMap::new()));
        let (wanted, receiver) = watch::channel(Wanted::new());
        let cache = std::env::home_dir().map(|home| home.join(CACHE_DIR));
        let task = TaskGuard::spawn(mirror(cache, timeout, receiver, states.clone(), changed));
        Self {
            states,
            wanted,
            _task: task,
        }
    }

    pub fn want(&self, wanted: Wanted) {
        self.wanted.send_if_modified(|current| {
            let differs = *current != wanted;
            if differs {
                *current = wanted;
            }
            differs
        });
    }

    pub fn states(&self) -> BTreeMap<RepoKey, MirrorState> {
        lock(&self.states).clone()
    }
}

async fn mirror(
    cache: Option<PathBuf>,
    timeout: Duration,
    mut wanted: watch::Receiver<Wanted>,
    states: Arc<Mutex<BTreeMap<RepoKey, MirrorState>>>,
    changed: async_channel::Sender<()>,
) {
    let mut fetched: BTreeMap<RepoKey, tokio::time::Instant> = BTreeMap::new();
    loop {
        let current = wanted.borrow_and_update().clone();
        lock(&states).retain(|key, _| current.contains_key(key));
        fetched.retain(|key, _| current.contains_key(key));
        for (key, revisions) in &current {
            let state = lock(&states).get(key).cloned();
            let expired = fetched
                .get(key)
                .is_none_or(|at| at.elapsed() >= FETCH_INTERVAL);
            if !needs_fetch(state.as_ref(), revisions, expired) {
                continue;
            }
            if state.is_none() {
                lock(&states).insert(key.clone(), MirrorState::Fetching);
                notify(&changed);
            }
            let next = match &cache {
                Some(cache) => sync(cache, key, timeout).await,
                None => MirrorState::Unavailable(NO_HOME.to_owned()),
            };
            fetched.insert(key.clone(), tokio::time::Instant::now());
            lock(&states).insert(key.clone(), next);
            notify(&changed);
        }
        tokio::select! {
            result = wanted.changed() => {
                if result.is_err() {
                    return;
                }
            }
            () = tokio::time::sleep(FETCH_INTERVAL) => {}
        }
    }
}

fn notify(changed: &async_channel::Sender<()>) {
    if changed.try_send(()).is_err() {
        tracing::trace!("GitOps change already pending");
    }
}

pub fn needs_fetch(
    current: Option<&MirrorState>,
    revisions: &BTreeSet<String>,
    expired: bool,
) -> bool {
    match current {
        None => true,
        Some(MirrorState::Fetching) => true,
        Some(MirrorState::Unavailable(_)) => expired,
        Some(MirrorState::Ready { commits, .. }) => {
            let known = |sha: &String| commits.iter().any(|commit| &commit.sha == sha);
            expired || !revisions.iter().all(known)
        }
    }
}

async fn sync(cache: &Path, key: &RepoKey, timeout: Duration) -> MirrorState {
    let dir = mirror_dir(cache, &key.url);
    match fetch_log(&dir, key, timeout).await {
        Ok(commits) => MirrorState::Ready {
            commits,
            fetched: Timestamp::now(),
        },
        Err(reason) => {
            tracing::info!(url = %redact::credentials(&key.url), %reason, "git mirror unavailable");
            MirrorState::Unavailable(reason)
        }
    }
}

async fn fetch_log(dir: &Path, key: &RepoKey, timeout: Duration) -> Result<Vec<GitCommit>, String> {
    if !dir.exists() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|error| error.to_string())?;
        git(dir, &["init", "--bare", "--quiet"], timeout).await?;
    }
    let refspec = format!("+refs/heads/{0}:refs/heads/{0}", key.branch);
    git(
        dir,
        &["fetch", "--quiet", "--no-tags", &key.url, &refspec],
        timeout,
    )
    .await?;
    let reference = format!("refs/heads/{}", key.branch);
    let limit = format!("-n{COMMIT_LIMIT}");
    let log = git(
        dir,
        &[
            "log",
            "-p",
            "--first-parent",
            "--diff-merges=first-parent",
            "--no-color",
            "--no-ext-diff",
            LOG_FORMAT,
            &limit,
            &reference,
            "--",
        ],
        timeout,
    )
    .await?;
    Ok(parse_log(&log))
}

async fn git(dir: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut command = Command::new(GIT);
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| {
            format!(
                "git {} timed out after {}s",
                args.first().copied().unwrap_or_default(),
                timeout.as_secs()
            )
        })?
        .map_err(|error| format!("git could not run: {error}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(redact::credentials(&first_line(&stderr)))
}

pub fn first_line(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map_or_else(|| STDERR_FALLBACK.to_owned(), str::to_owned)
}

pub fn repo_name(url: &str) -> String {
    let clean = redact::credentials(url);
    let path = clean.split(['?', '#']).next().unwrap_or_default();
    let trimmed = path.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or(trimmed);
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    })
}

pub fn mirror_dir(cache: &Path, url: &str) -> PathBuf {
    let safe: String = repo_name(url)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    cache.join(format!("{safe}-{:016x}.git", fnv1a(url)))
}

pub fn parse_log(text: &str) -> Vec<GitCommit> {
    text.split(RECORD)
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let (header, patch) = record.split_once('\n').unwrap_or((record, ""));
            let mut fields = header.split(FIELD);
            let sha = fields.next()?.trim().to_owned();
            let author = fields.next()?.to_owned();
            let seconds: i64 = fields.next()?.parse().ok()?;
            let subject = fields.next().unwrap_or_default().to_owned();
            let time = Timestamp::from_second(seconds).ok()?;
            let valid = !sha.is_empty() && sha.chars().all(|c| c.is_ascii_hexdigit());
            valid.then(|| GitCommit {
                sha,
                author,
                time,
                subject,
                files: parse_patch(patch),
            })
        })
        .collect()
}

struct Cursor {
    old: u32,
    new: u32,
}

fn hunk_start(line: &str) -> Option<Cursor> {
    let body = line.strip_prefix("@@ -")?;
    let (old, rest) = body.split_once(" +")?;
    let new = rest.split_once(" @@").map_or(rest, |(new, _)| new);
    let start = |range: &str| range.split(',').next()?.parse::<u32>().ok();
    Some(Cursor {
        old: start(old)?,
        new: start(new)?,
    })
}

pub fn parse_patch(text: &str) -> Vec<FilePatch> {
    let mut files: Vec<FilePatch> = Vec::new();
    let mut cursor: Option<Cursor> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            cursor = None;
            let path = rest
                .split_once(" b/")
                .map_or(rest, |(_, new)| new)
                .to_owned();
            files.push(FilePatch {
                path,
                lines: Vec::new(),
            });
            continue;
        }
        if let Some(start) = hunk_start(line) {
            cursor = Some(start);
            continue;
        }
        let (Some(at), Some(file)) = (cursor.as_mut(), files.last_mut()) else {
            continue;
        };
        let (change, text, number) = if let Some(text) = line.strip_prefix('+') {
            at.new += 1;
            (LineChange::Added, text, at.new - 1)
        } else if let Some(text) = line.strip_prefix('-') {
            at.old += 1;
            (LineChange::Removed, text, at.old - 1)
        } else if let Some(text) = line.strip_prefix(' ') {
            at.old += 1;
            at.new += 1;
            (LineChange::Context, text, at.new - 1)
        } else {
            continue;
        };
        if file.lines.len() < MAX_FILE_LINES {
            file.lines.push(PatchLine {
                change,
                number,
                text: text.to_owned(),
                note: None,
            });
        }
    }
    files.truncate(MAX_FILES);
    files
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    const PLANTED_USER: &str = "planted-user-token";
    const PLANTED_QUERY: &str = "planted-query-token";
    const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            lock(&self.0).extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[tokio::test]
    async fn failed_fetches_never_log_or_report_url_credentials() {
        let Ok(cache) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let key = RepoKey {
            url: format!(
                "https://x-access-token:{PLANTED_USER}@127.0.0.1:9/acme/repo.git?access_token={PLANTED_QUERY}"
            ),
            branch: "main".to_owned(),
        };
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_ansi(false)
            .finish();
        let state = {
            let _guard = tracing::subscriber::set_default(subscriber);
            sync(cache.path(), &key, FETCH_TIMEOUT).await
        };
        let MirrorState::Unavailable(reason) = state else {
            panic!("fetch from a closed port succeeded");
        };
        let logged = String::from_utf8_lossy(&lock(&captured.0)).into_owned();
        assert!(logged.contains("git mirror unavailable"), "{logged}");
        for output in [&logged, &reason, &key.name()] {
            assert!(!output.contains(PLANTED_USER), "{output}");
            assert!(!output.contains(PLANTED_QUERY), "{output}");
        }
    }

    const LOG: &str = "\u{1e}3e0ff8ae123b\u{1f}Stefan Prodan\u{1f}1791870000\u{1f}Release v6.15.0\n\ndiff --git a/kustomize/deployment.yaml b/kustomize/deployment.yaml\nindex 1..2 100644\n--- a/kustomize/deployment.yaml\n+++ b/kustomize/deployment.yaml\n@@ -21,3 +21,4 @@ spec:\n       containers:\n-        image: podinfo:6.14.0\n+        image: podinfo:6.15.0\n+        args: []\n       ports:\n\u{1e}aaaa111\u{1f}Bot\u{1f}1791860000\u{1f}docs: typo\n\ndiff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-colour\n+color\n";

    #[test]
    fn log_parses_commits_and_numbered_patch_lines() {
        let commits = parse_log(LOG);
        assert_eq!(commits.len(), 2);
        let release = &commits[0];
        assert_eq!(release.sha, "3e0ff8ae123b");
        assert_eq!(release.author, "Stefan Prodan");
        assert_eq!(release.subject, "Release v6.15.0");
        assert_eq!(release.time.as_second(), 1_791_870_000);
        let file = &release.files[0];
        assert_eq!(file.path, "kustomize/deployment.yaml");
        let lines: Vec<(LineChange, u32, &str)> = file
            .lines
            .iter()
            .map(|l| (l.change, l.number, l.text.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (LineChange::Context, 21, "      containers:"),
                (LineChange::Removed, 22, "        image: podinfo:6.14.0"),
                (LineChange::Added, 22, "        image: podinfo:6.15.0"),
                (LineChange::Added, 23, "        args: []"),
                (LineChange::Context, 24, "      ports:"),
            ]
        );
        assert_eq!(commits[1].files[0].added(), 1);
        assert_eq!(commits[1].files[0].removed(), 1);
    }

    #[test]
    fn mirror_dirs_are_stable_safe_and_distinct() {
        let cache = Path::new("/c");
        let https = mirror_dir(cache, "https://github.com/stefanprodan/podinfo");
        let ssh = mirror_dir(cache, "ssh://git@github.com/stefanprodan/podinfo.git");
        assert_eq!(
            https,
            mirror_dir(cache, "https://github.com/stefanprodan/podinfo")
        );
        assert_ne!(https, ssh);
        let name = https
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        assert!(name.starts_with("podinfo-"), "{name}");
        assert_eq!(https.extension().and_then(|e| e.to_str()), Some("git"));
        assert_eq!(https.parent(), Some(cache));
        let odd = mirror_dir(cache, "git@host:team/../we ird.git");
        let odd_name = odd.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        assert!(odd_name.starts_with("we_ird-"), "{odd_name}");
    }

    #[test]
    fn repo_names_drop_suffixes() {
        assert_eq!(
            repo_name("https://github.com/acme/fleet-infra.git/"),
            "fleet-infra"
        );
        assert_eq!(
            repo_name("git@github.com:acme/fleet-infra.git"),
            "fleet-infra"
        );
    }

    #[test]
    fn fetch_is_needed_for_new_stale_or_unknown_revisions() {
        let commit = GitCommit {
            sha: "abc".to_owned(),
            author: String::new(),
            time: Timestamp::UNIX_EPOCH,
            subject: String::new(),
            files: Vec::new(),
        };
        let ready = MirrorState::Ready {
            commits: vec![commit],
            fetched: Timestamp::UNIX_EPOCH,
        };
        let known = BTreeSet::from(["abc".to_owned()]);
        let unknown = BTreeSet::from(["def".to_owned()]);
        assert!(needs_fetch(None, &known, false));
        assert!(!needs_fetch(Some(&ready), &known, false));
        assert!(needs_fetch(Some(&ready), &unknown, false));
        assert!(needs_fetch(Some(&ready), &known, true));
        let failed = MirrorState::Unavailable("auth".to_owned());
        assert!(!needs_fetch(Some(&failed), &unknown, false));
        assert!(needs_fetch(Some(&failed), &unknown, true));
    }

    #[test]
    fn stderr_reduces_to_its_first_line() {
        assert_eq!(
            first_line("\nfatal: could not read Username\nmore"),
            "fatal: could not read Username"
        );
        assert_eq!(first_line(""), STDERR_FALLBACK);
    }

    #[test]
    fn repo_names_ignore_credentials_and_queries() {
        assert_eq!(
            repo_name("https://user:secret@host/acme/repo.git?token=secret"),
            "repo"
        );
        assert_eq!(repo_name("https://user:secret@host"), "host");
    }
}
