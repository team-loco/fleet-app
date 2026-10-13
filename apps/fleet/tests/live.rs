#![cfg(target_os = "macos")]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};

const E2E_VAR: &str = "FLEET_E2E";
const KUBECONFIG_VAR: &str = "KUBECONFIG";
const LOG_SNAPSHOTS_VAR: &str = "FLEET_LOG_SNAPSHOTS";
const CLUSTERS: [(&str, &str); 2] = [("fleet-a", "kind-fleet-a"), ("fleet-b", "kind-fleet-b")];
const SHARED_PROBLEMS: [&str; 3] = ["unschedulable", "broken-kustomization", "stripe-webhook"];
const FLEET_A_PROBLEMS: [&str; 1] = ["checkout-api"];
const SNAPSHOT_MARKER: &str = "snapshot clusters=";
const PROBLEM_MARKER: &str = "problem ";
const CONNECTED_FIELD: &str = "connected=";
const WAIT: Duration = Duration::from_secs(180);

struct App(Child);

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn kubeconfig_for(context: &str) -> anyhow::Result<PathBuf> {
    let value = std::env::var_os(KUBECONFIG_VAR).context("KUBECONFIG must be set")?;
    for file in std::env::split_paths(&value) {
        let body = std::fs::read_to_string(&file)?;
        if body.contains(context) {
            return Ok(file);
        }
    }
    bail!("no kubeconfig file in KUBECONFIG defines {context}")
}

fn seed_store(dir: &Path) -> anyhow::Result<()> {
    let mut clusters = Vec::new();
    for (name, context) in CLUSTERS {
        clusters.push(serde_json::json!({
            "file": kubeconfig_for(context)?,
            "context": context,
            "name": name,
        }));
    }
    let body = serde_json::json!({ "clusters": clusters });
    std::fs::write(dir.join("clusters.json"), serde_json::to_vec_pretty(&body)?)?;
    Ok(())
}

fn connected_count(line: &str) -> Option<usize> {
    let rest = line.split(CONNECTED_FIELD).nth(1)?;
    rest.split_whitespace().next()?.parse().ok()
}

fn missing_problems(block: &[String]) -> Vec<String> {
    let mut missing = Vec::new();
    for (name, _) in CLUSTERS {
        let cluster_field = format!("cluster={name}");
        let expected = SHARED_PROBLEMS.iter().chain(if name == "fleet-a" {
            FLEET_A_PROBLEMS.iter()
        } else {
            [].iter()
        });
        for problem in expected {
            let found = block
                .iter()
                .any(|line| line.contains(&cluster_field) && line.contains(problem));
            if !found {
                missing.push(format!("{name}/{problem}"));
            }
        }
    }
    missing
}

#[test]
fn live_app_connects_both_kind_clusters_and_shows_the_scenarios() -> anyhow::Result<()> {
    if std::env::var(E2E_VAR).as_deref() != Ok("1") {
        return Ok(());
    }
    let data_dir = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    seed_store(data_dir.path())?;
    let child = Command::new(env!("CARGO_BIN_EXE_fleet"))
        .arg("--data-dir")
        .arg(data_dir.path())
        .env("HOME", home.path())
        .env(LOG_SNAPSHOTS_VAR, "1")
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut app = App(child);
    let stdout = app.0.stdout.take().context("stdout piped")?;
    let (lines_tx, lines_rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + WAIT;
    let mut block: Vec<String> = Vec::new();
    let mut connected = 0;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match lines_rx.recv_timeout(remaining) {
            Ok(line) if line.contains(SNAPSHOT_MARKER) => {
                connected = connected_count(&line).unwrap_or_default();
                block.clear();
            }
            Ok(line) if line.contains(PROBLEM_MARKER) => block.push(line),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {
                bail!(
                    "connected={connected}, missing {:?}, saw {block:#?}",
                    missing_problems(&block)
                )
            }
            Err(RecvTimeoutError::Disconnected) => {
                bail!("fleet exited before the scenarios appeared")
            }
        }
        if connected == CLUSTERS.len() && missing_problems(&block).is_empty() {
            return Ok(());
        }
    }
}
