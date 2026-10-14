use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::process::Command;

const PATH_VAR: &str = "PATH";
const SHELL_VAR: &str = "SHELL";
const DEFAULT_SHELL: &str = "/bin/zsh";
const MARKER: &str = "__FLOCK_ENV_7c1e9b__";
const FALLBACK_ABSOLUTE_DIRS: [&str; 4] = [
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
];
const FALLBACK_HOME_DIRS: [&str; 6] = [
    ".local/bin",
    "google-cloud-sdk/bin",
    ".local/share/mise/shims",
    ".asdf/shims",
    ".krew/bin",
    "bin",
];
const UNSHARED_VARS: [&str; 5] = ["PWD", "OLDPWD", "SHLVL", "_", "TERM_SESSION_ID"];
pub const AUTH_BINARIES: [&str; 8] = [
    "aws",
    "aws-iam-authenticator",
    "gcloud",
    "gke-gcloud-auth-plugin",
    "az",
    "kubelogin",
    "doctl",
    "kubectl",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvOrigin {
    LoginShell { shell: PathBuf, took: Duration },
    Fallback { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathOrigin {
    LoginShell,
    Process,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathEntry {
    pub dir: PathBuf,
    pub origin: PathOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryResolution {
    pub name: String,
    pub resolved: Option<PathEntry>,
}

impl BinaryResolution {
    pub fn path(&self) -> Option<PathBuf> {
        self.resolved
            .as_ref()
            .map(|entry| entry.dir.join(&self.name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellEnv {
    pub vars: BTreeMap<String, String>,
    pub path: Vec<PathEntry>,
    pub origin: EnvOrigin,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShellEnvError {
    #[error("no environment markers in shell output")]
    MissingMarkers,
    #[error("environment dump is not UTF-8")]
    NotUtf8,
    #[error("environment dump has no PATH")]
    NoPath,
}

pub fn parse_env_dump(output: &[u8]) -> Result<BTreeMap<String, String>, ShellEnvError> {
    let text = std::str::from_utf8(output).map_err(|_| ShellEnvError::NotUtf8)?;
    let mut parts = text.splitn(3, MARKER);
    let (Some(_), Some(body), Some(_)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(ShellEnvError::MissingMarkers);
    };
    let vars: BTreeMap<String, String> = body
        .split('\0')
        .filter_map(|entry| entry.split_once('='))
        .filter(|(name, _)| !name.is_empty() && !name.contains('\n'))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    if !vars.contains_key(PATH_VAR) {
        return Err(ShellEnvError::NoPath);
    }
    Ok(vars)
}

fn fallback_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let absolute = FALLBACK_ABSOLUTE_DIRS.iter().map(PathBuf::from);
    let in_home = home
        .into_iter()
        .flat_map(|home| FALLBACK_HOME_DIRS.iter().map(move |dir| home.join(dir)));
    absolute.chain(in_home).collect()
}

fn split_path(value: &str) -> Vec<PathBuf> {
    std::env::split_paths(value)
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect()
}

pub fn merge_path(primary: &[PathBuf], origin: PathOrigin, home: Option<&Path>) -> Vec<PathEntry> {
    let mut entries: Vec<PathEntry> = Vec::new();
    let primary_entries = primary.iter().map(|dir| (dir.clone(), origin));
    let fallback_entries = fallback_dirs(home)
        .into_iter()
        .map(|dir| (dir, PathOrigin::Fallback));
    for (dir, origin) in primary_entries.chain(fallback_entries) {
        if !entries.iter().any(|entry| entry.dir == dir) {
            entries.push(PathEntry { dir, origin });
        }
    }
    entries
}

impl ShellEnv {
    pub fn from_vars(
        mut vars: BTreeMap<String, String>,
        origin: EnvOrigin,
        home: Option<&Path>,
    ) -> Self {
        let path_origin = match origin {
            EnvOrigin::LoginShell { .. } => PathOrigin::LoginShell,
            EnvOrigin::Fallback { .. } => PathOrigin::Process,
        };
        let primary = vars
            .get(PATH_VAR)
            .map(|value| split_path(value))
            .unwrap_or_default();
        let path = merge_path(&primary, path_origin, home);
        for name in UNSHARED_VARS {
            vars.remove(name);
        }
        let joined = std::env::join_paths(path.iter().map(|entry| &entry.dir))
            .ok()
            .and_then(|value| value.into_string().ok());
        if let Some(joined) = joined {
            vars.insert(PATH_VAR.to_owned(), joined);
        }
        Self { vars, path, origin }
    }

    pub fn from_process(reason: String) -> Self {
        let vars = std::env::vars().collect();
        Self::from_vars(
            vars,
            EnvOrigin::Fallback { reason },
            std::env::home_dir().as_deref(),
        )
    }

    pub fn path_var(&self) -> Option<&str> {
        self.vars.get(PATH_VAR).map(String::as_str)
    }

    pub fn resolve(&self, program: &str) -> Option<PathBuf> {
        if program.contains(std::path::MAIN_SEPARATOR) {
            return Some(PathBuf::from(program));
        }
        self.resolve_entry(program)
            .map(|entry| entry.dir.join(program))
    }

    fn resolve_entry(&self, program: &str) -> Option<&PathEntry> {
        self.path
            .iter()
            .find(|entry| is_executable(&entry.dir.join(program)))
    }

    pub fn diagnostics(&self, binaries: &[&str]) -> Vec<BinaryResolution> {
        binaries
            .iter()
            .map(|name| BinaryResolution {
                name: (*name).to_owned(),
                resolved: self.resolve_entry(name).cloned(),
            })
            .collect()
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    const EXECUTE_BITS: u32 = 0o111;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & EXECUTE_BITS != 0)
}

pub fn user_shell() -> PathBuf {
    std::env::var_os(SHELL_VAR)
        .filter(|shell| !shell.is_empty())
        .map_or_else(|| PathBuf::from(DEFAULT_SHELL), PathBuf::from)
}

async fn run_login_shell(shell: &Path, timeout: Duration) -> Result<Vec<u8>, String> {
    let script = format!("printf '%s' '{MARKER}'; env -0; printf '%s' '{MARKER}'");
    let child = Command::new(shell)
        .args(["-l", "-i", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("starting {}: {err}", shell.display()))?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("{} did not finish within {timeout:?}", shell.display()))?
        .map_err(|err| format!("running {}: {err}", shell.display()))?;
    Ok(output.stdout)
}

pub async fn capture(shell: &Path, timeout: Duration) -> ShellEnv {
    let started = Instant::now();
    let home = std::env::home_dir();
    let captured = run_login_shell(shell, timeout)
        .await
        .and_then(|stdout| parse_env_dump(&stdout).map_err(|err| err.to_string()));
    match captured {
        Ok(vars) => {
            let origin = EnvOrigin::LoginShell {
                shell: shell.to_path_buf(),
                took: started.elapsed(),
            };
            ShellEnv::from_vars(vars, origin, home.as_deref())
        }
        Err(reason) => {
            tracing::warn!(%reason, "login shell environment capture failed");
            ShellEnv::from_process(reason)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn dump(body: &str) -> Vec<u8> {
        format!("Last login: today\n\u{1b}[1mprompt\u{1b}[0m{MARKER}{body}{MARKER}bye\n")
            .into_bytes()
    }

    #[test]
    fn parses_between_markers_ignoring_noise() {
        let parsed = parse_env_dump(&dump(
            "PATH=/opt/homebrew/bin:/usr/bin\0AWS_PROFILE=prod\0MULTI=a\nb=c\0\0",
        ));
        let Ok(vars) = parsed else {
            panic!("parse failed: {parsed:?}");
        };
        assert_eq!(
            vars.get("PATH").map(String::as_str),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(vars.get("AWS_PROFILE").map(String::as_str), Some("prod"));
        assert_eq!(vars.get("MULTI").map(String::as_str), Some("a\nb=c"));
    }

    #[test]
    fn rejects_missing_markers_and_path() {
        assert_eq!(
            parse_env_dump(b"PATH=/usr/bin\0"),
            Err(ShellEnvError::MissingMarkers)
        );
        assert_eq!(
            parse_env_dump(&dump("HOME=/x\0")),
            Err(ShellEnvError::NoPath)
        );
    }

    #[test]
    fn fallback_dirs_appended_without_duplicates() {
        let home = PathBuf::from("/Users/me");
        let primary = vec![
            PathBuf::from("/usr/bin"),
            PathBuf::from("/opt/homebrew/bin"),
        ];
        let merged = merge_path(&primary, PathOrigin::LoginShell, Some(&home));
        let dirs: Vec<&Path> = merged.iter().map(|entry| entry.dir.as_path()).collect();
        assert_eq!(dirs.first(), Some(&Path::new("/usr/bin")));
        assert_eq!(
            dirs.iter()
                .filter(|d| **d == Path::new("/opt/homebrew/bin"))
                .count(),
            1
        );
        assert!(dirs.contains(&Path::new("/Users/me/google-cloud-sdk/bin")));
        assert!(dirs.contains(&Path::new("/Users/me/.local/share/mise/shims")));
        let homebrew = merged
            .iter()
            .find(|entry| entry.dir == Path::new("/opt/homebrew/bin"));
        assert_eq!(
            homebrew.map(|entry| entry.origin),
            Some(PathOrigin::LoginShell)
        );
    }

    #[test]
    fn resolves_binaries_and_reports_origin() {
        use std::os::unix::fs::PermissionsExt;
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let plugin = dir.path().join("gke-gcloud-auth-plugin");
        assert!(fs::write(&plugin, "#!/bin/sh\n").is_ok());
        assert!(fs::set_permissions(&plugin, fs::Permissions::from_mode(0o755)).is_ok());
        let not_exec = dir.path().join("flock-test-not-executable");
        assert!(fs::write(&not_exec, "").is_ok());
        let mut vars = BTreeMap::new();
        vars.insert("PATH".to_owned(), dir.path().display().to_string());
        vars.insert("SHLVL".to_owned(), "2".to_owned());
        let origin = EnvOrigin::LoginShell {
            shell: PathBuf::from("/bin/zsh"),
            took: Duration::ZERO,
        };
        let env = ShellEnv::from_vars(vars, origin, None);

        assert_eq!(env.resolve("gke-gcloud-auth-plugin"), Some(plugin.clone()));
        assert_eq!(env.resolve("/abs/aws"), Some(PathBuf::from("/abs/aws")));
        assert!(!env.vars.contains_key("SHLVL"));
        let report = env.diagnostics(&["gke-gcloud-auth-plugin", "flock-test-not-executable"]);
        assert_eq!(
            report.first().and_then(BinaryResolution::path),
            Some(plugin)
        );
        assert_eq!(
            report
                .first()
                .and_then(|r| r.resolved.as_ref().map(|e| e.origin)),
            Some(PathOrigin::LoginShell)
        );
        assert_eq!(report.get(1).and_then(|r| r.resolved.clone()), None);
    }

    #[tokio::test]
    async fn captures_from_a_real_shell() {
        let env = capture(Path::new("/bin/sh"), Duration::from_secs(5)).await;
        assert!(
            matches!(env.origin, EnvOrigin::LoginShell { .. }),
            "{:?}",
            env.origin
        );
        assert!(env.resolve("sh").is_some());
    }

    #[tokio::test]
    async fn falls_back_when_shell_hangs() {
        use std::os::unix::fs::PermissionsExt;
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let shell = dir.path().join("slow-shell");
        assert!(fs::write(&shell, "#!/bin/sh\nsleep 30\n").is_ok());
        assert!(fs::set_permissions(&shell, fs::Permissions::from_mode(0o755)).is_ok());
        let env = capture(&shell, Duration::from_millis(200)).await;
        assert!(matches!(env.origin, EnvOrigin::Fallback { .. }));
        assert!(
            env.path
                .iter()
                .any(|entry| entry.origin == PathOrigin::Fallback)
        );
    }
}
