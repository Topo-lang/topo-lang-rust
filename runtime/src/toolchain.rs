//! Locate the built Topo toolchain binaries.
//!
//! topo-app is a product layer that *consumes* the existing toolchain;
//! it never reimplements parsing or checking. Resolution order mirrors
//! `_toolchain.py`:
//!
//!   1. explicit env var (`TOPO_BIN_DIR`) — used by tests and CI;
//!   2. a sibling build tree of this checkout.
//!
//! Only the LLVM-enabled `build/` tree is consulted, never
//! `build-no-llvm/`: that stale tree mis-resolves and produces spurious
//! parse failures (a tracked environment issue). A clear panic is raised
//! if neither yields the binary — silently degrading a correctness tool
//! would defeat the point.

use std::path::PathBuf;

/// This file lives at `topo-lang-rust/runtime/src/toolchain.rs`; the
/// repository root is four parents up from the file.
fn repo_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // .../topo-lang-rust/runtime
    p.pop(); // topo-lang-rust
    p.pop(); // repo root
    p
}

/// The main checkout's root when this crate is compiled from a git
/// worktree.
///
/// Worktrees live at `<main>/.claude/worktrees/<name>`; the fresh
/// toolchain `build/` tree is produced once in the main checkout and
/// shared (worktree agents reuse it rather than rebuilding). Stripping a
/// trailing `.claude/worktrees/<name>` recovers the main root so the
/// binaries resolve from a worktree without a per-worktree build.
/// Returns `None` when not inside a worktree (paths already main-rooted).
fn main_checkout_root(root: &std::path::Path) -> Option<PathBuf> {
    let mut comps: Vec<_> = root.components().collect();
    if comps.len() >= 3
        && comps[comps.len() - 2].as_os_str() == "worktrees"
        && comps[comps.len() - 3].as_os_str() == ".claude"
    {
        comps.truncate(comps.len() - 3);
        return Some(comps.iter().collect());
    }
    None
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(env) = std::env::var("TOPO_BIN_DIR") {
        if !env.is_empty() {
            dirs.push(PathBuf::from(env));
        }
    }
    // Fresh, LLVM-enabled build tree only. `build-no-llvm` is excluded
    // deliberately (see module docs).
    let root = repo_root();
    if let Some(main) = main_checkout_root(&root) {
        // Prefer the shared main-checkout build tree (where the fresh
        // binaries actually live) before the worktree-local one.
        dirs.push(main.join("build"));
    }
    dirs.push(root.join("build"));
    dirs
}

fn find(rel: &str) -> PathBuf {
    let leaf = std::path::Path::new(rel)
        .file_name()
        .map(|s| s.to_owned())
        .unwrap_or_default();
    for base in candidate_dirs() {
        let cand = base.join(rel);
        if cand.is_file() {
            return cand;
        }
        // `TOPO_BIN_DIR` may point straight at a bin directory.
        let flat = base.join(&leaf);
        if flat.is_file() {
            return flat;
        }
    }
    panic!(
        "could not locate '{rel}'. Build the toolchain \
         (cmake --preset default && cmake --build build --target \
         topo topo-check) or set TOPO_BIN_DIR."
    );
}

/// Absolute path to the fresh `topo` compiler front-end.
pub fn topo_bin() -> PathBuf {
    find("topo-core/tools/topo/topo")
}

/// Absolute path to the fresh `topo-check` consistency checker.
pub fn topo_check_bin() -> PathBuf {
    find("topo-cli/tools/topo-check/topo-check")
}

/// Directory holding `topo-extract-rust`, the Rust source extractor the
/// Rust check path shells out to. `topo-check` resolves it via `PATH`;
/// the check bridge prepends this directory so a zero-config caller
/// still gets the L1 symbol-access analysis the purity check needs.
pub fn rust_extractor_dir() -> PathBuf {
    let rel = std::path::Path::new("topo-lang-rust")
        .join("topo-check")
        .join("extractor")
        .join("target")
        .join("release");
    let root = repo_root();
    // Same shared-build rationale as the binaries: the extractor is
    // built once in the main checkout. Prefer that copy from a worktree.
    if let Some(main) = main_checkout_root(&root) {
        let in_main = main.join(&rel);
        if in_main.is_dir() {
            return in_main;
        }
    }
    root.join(rel)
}
