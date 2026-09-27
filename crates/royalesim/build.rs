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

    // Rebuild the stamp when HEAD moves. The files are found through git, not at `../../.git`:
    // in a linked worktree `.git` is a FILE naming the worktree's own directory, so a fixed
    // `../../.git/HEAD` does not exist there and a commit never re-ran this script. On
    // 2026-09-26 a wheel built in a worktree right after a commit reported the PARENT commit
    // and "dirty" (the stamp left by a `cargo check` run before the commit) while holding the
    // committed code. HEAD and the index live in the worktree's directory (`--git-dir`); the
    // branch a commit moves lives in the shared one (`--git-common-dir`), as a loose ref or in
    // packed-refs. Without git (a fresh archive) none of these exist and the stamp keeps its
    // last value, which is why `provenance()` is documented as best-effort rather than as a
    // guarantee.
    let dir = |flag: &str| git(&["rev-parse", "--path-format=absolute", flag]).map(std::path::PathBuf::from);
    let mut watched = Vec::new();
    if let Some(d) = dir("--git-dir") {
        watched.push(d.join("HEAD"));
        watched.push(d.join("index"));
    }
    if let Some(c) = dir("--git-common-dir") {
        watched.push(c.join("packed-refs"));
        if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
            watched.push(c.join(branch));
        }
    }
    for path in watched {
        // A path that does not exist would re-run this script on every build.
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
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
