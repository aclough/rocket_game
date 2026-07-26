//! Stamps the build with the git commit it came from, so a bug report
//! identifies the exact code rather than just "0.1.0" — which every
//! release will be for a while.
//!
//! Degrades quietly: a source tarball with no `.git` yields "unknown",
//! which is a worse report but not a failed build.

use std::process::Command;

fn main() {
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let stamp = match hash {
        Some(h) if dirty => format!("{h}-modified"),
        Some(h) => h,
        None => "unknown".to_string(),
    };
    println!("cargo:rustc-env=ROCKET_TYCOON_GIT={stamp}");

    // Re-run when the checked-out commit changes. Not perfect for the
    // dirty flag (that would mean re-running on every file change), and
    // deliberately so — "-modified" is a hint for dev builds, and
    // release builds are made from a clean tree.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
}
