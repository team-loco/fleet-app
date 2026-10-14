use std::io;
use std::process::Command;

fn git(args: &[&str]) -> io::Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!("git {args:?}: {stderr}")));
    }
    let stdout = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let value = stdout.trim();
    if value.is_empty() {
        return Err(io::Error::other(format!("git {args:?} returned no value")));
    }
    Ok(value.to_owned())
}

fn watch_git_path(path: &str) -> io::Result<()> {
    let resolved = git(&["rev-parse", "--git-path", path])?;
    println!("cargo:rerun-if-changed={resolved}");
    Ok(())
}

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    watch_git_path("HEAD")?;
    watch_git_path("packed-refs")?;
    let reference = git(&["rev-parse", "--symbolic-full-name", "HEAD"])?;
    if reference != "HEAD" {
        watch_git_path(&reference)?;
    }
    let sha = git(&["rev-parse", "--short", "HEAD"])?;
    if !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(io::Error::other("git returned an invalid revision"));
    }
    println!("cargo:rustc-env=FLOCK_GIT_SHA={sha}");
    Ok(())
}
