use std::fs;
use std::path::{Path, PathBuf};

use crate::stage::run;

pub const BUNDLE: &str = "Fleet.app";
pub const ID: &str = "dev.fleet.app";
const EXECUTABLE: &str = "/usr/bin/true";

pub fn temp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"))
}

fn info_plist(identifier: &str, version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>fleet</string>
  <key>CFBundleIdentifier</key><string>{identifier}</string>
  <key>CFBundleName</key><string>Fleet</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundleVersion</key><string>{version}</string>
</dict>
</plist>
"#
    )
}

pub fn fake_bundle(dir: &Path, identifier: &str, version: &str) -> PathBuf {
    let bundle = dir.join(BUNDLE);
    let macos = bundle.join("Contents").join("MacOS");
    assert!(fs::create_dir_all(&macos).is_ok());
    let plist = bundle.join("Contents").join("Info.plist");
    assert!(fs::write(plist, info_plist(identifier, version)).is_ok());
    assert!(fs::copy(EXECUTABLE, macos.join("fleet")).is_ok());
    let signed = run(
        "codesign",
        &[
            "--force".as_ref(),
            "--deep".as_ref(),
            "-s".as_ref(),
            "-".as_ref(),
            bundle.as_os_str(),
        ],
    );
    assert!(signed.is_ok(), "codesign: {signed:?}");
    bundle
}

pub fn zip(bundle: &Path) -> Vec<u8> {
    let archive = bundle.with_extension("zip");
    let zipped = run(
        "ditto",
        &[
            "-c".as_ref(),
            "-k".as_ref(),
            "--keepParent".as_ref(),
            bundle.as_os_str(),
            archive.as_os_str(),
        ],
    );
    assert!(zipped.is_ok(), "ditto: {zipped:?}");
    fs::read(&archive).unwrap_or_else(|error| panic!("zip: {error}"))
}

pub fn bundle_version(bundle: &Path) -> String {
    let plist = bundle.join("Contents").join("Info.plist");
    run(
        "plutil",
        &[
            "-extract".as_ref(),
            "CFBundleShortVersionString".as_ref(),
            "raw".as_ref(),
            "-o".as_ref(),
            "-".as_ref(),
            plist.as_os_str(),
        ],
    )
    .unwrap_or_else(|error| panic!("plutil: {error}"))
}

pub fn quarantined(path: &Path) -> bool {
    run(
        "xattr",
        &[
            "-p".as_ref(),
            "com.apple.quarantine".as_ref(),
            path.as_os_str(),
        ],
    )
    .is_ok()
}

pub fn quarantine(path: &Path) {
    let marked = run(
        "xattr",
        &[
            "-w".as_ref(),
            "com.apple.quarantine".as_ref(),
            "0081;00000000;Safari;".as_ref(),
            path.as_os_str(),
        ],
    );
    assert!(marked.is_ok(), "xattr: {marked:?}");
}
