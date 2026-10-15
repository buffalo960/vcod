// Build script shared by the `vcod` and `vcod-server` binaries: sets
// `VCOD_VERSION` to `<package version>-<git short sha>`. `VCOD_GIT_SHA`
// overrides the sha for builds outside a checkout; without either it reads
// `unknown`.
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !s.is_empty()).then_some(s)
}

fn main() {
    println!("cargo:rerun-if-env-changed=VCOD_GIT_SHA");
    // A commit moves HEAD's ref (or HEAD itself when detached); a gc moves
    // refs into packed-refs. Worktrees keep HEAD apart from the refs.
    if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Some(path) =
        git(&["symbolic-ref", "-q", "HEAD"]).and_then(|r| git(&["rev-parse", "--git-path", &r]))
    {
        println!("cargo:rerun-if-changed={path}");
    }
    if let Some(packed) = git(&["rev-parse", "--git-path", "packed-refs"]) {
        println!("cargo:rerun-if-changed={packed}");
    }
    let sha = std::env::var("VCOD_GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| git(&["rev-parse", "--short=8", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    let version = env!("CARGO_PKG_VERSION");
    println!("cargo:rustc-env=VCOD_VERSION={version}-{sha}");
}
