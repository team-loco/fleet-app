use std::process::Command;

#[test]
fn version_reports_workspace_version_and_git_revision_without_launching() -> anyhow::Result<()> {
    let revision = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()?;
    assert!(revision.status.success());
    let sha = String::from_utf8(revision.stdout)?;
    let output = Command::new(env!("CARGO_BIN_EXE_fleet"))
        .arg("--version")
        .output()?;
    assert!(output.status.success());
    let expected = format!("fleet {} ({})\n", env!("CARGO_PKG_VERSION"), sha.trim());
    assert_eq!(output.stdout, expected.as_bytes());
    Ok(())
}
