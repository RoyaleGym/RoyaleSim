//! Stamp the extension with the source it was built from.
//!
//! WHY. The compiled engine is loaded from the venv at runtime, with no checkout around it, so
//! nothing could answer "was this code the code in the last commit". The data side was already
//! answerable (the ledger is embedded and compared against the file on disk), and the code side
//! was not, so a rebuild from a dirty tree was invisible to every guard in the family. On
//! 2026-09-22 the engine was rebuilt twice from trees with uncommitted changes and no tool could
//! have said so.
//!
//! WHAT IT DOES NOT PROVE. The commit is what `git` reported in the crate directory at build
//! time. `dirty` covers tracked, modified files under the repo, which is what makes a build
//! unreproducible in practice; an untracked file that the build reads would not show. When git is
//! not available at all, both come out "unknown", which a consumer must treat as unknown rather
//! than as clean.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    // Tracked files only. `--porcelain` prints one line per changed path, so empty means clean.
    let dirty = match git(&["status", "--porcelain", "--untracked-files=no"]) {
        None => "unknown",
        Some(s) if s.is_empty() => "clean",
        Some(_) => "dirty",
    };
    println!("cargo:rustc-env=ROYALESIM_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=ROYALESIM_BUILD_TREE={dirty}");

    // Rebuild the stamp when HEAD moves. A ref file is the cheapest signal; if the path is not
    // there (a worktree, a fresh archive) the stamp simply keeps its last value, which is why
    // `provenance()` is documented as best-effort rather than as a guarantee.
    for path in ["../../.git/HEAD", "../../.git/index"] {
        if std::path::Path::new(path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    // AND WHEN ANY BUILD INPUT CHANGES. Declaring even one `rerun-if-changed` switches OFF
    // cargo's default of re-running this script on any change in the package, so with only
    // the two git files above an EDITED-BUT-UNSTAGED source file recompiled the crate and left
    // this stamp at its previous value. On 2026-09-24 that shipped a wheel containing
    // uncommitted changes to four files while `provenance()` reported ("a44ef09...", "clean"):
    // the lie this file exists to prevent, told by the file itself. The sources, the manifest
    // and the data the crate `include_str!`s all re-stamp now; a directory is scanned whole.
    for path in ["src", "Cargo.toml", "../../data"] {
        println!("cargo:rerun-if-changed={path}");
    }
}
