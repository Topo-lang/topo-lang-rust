//! Zero-declaration check: hand the emitted `.topo` to the existing
//! `topo-check`.
//!
//! The third-scenario product value is "use the framework, get `topo check` for free":
//! the user writes no `.topo` by hand. We materialise a throwaway
//! project (Topo.toml + emitted .topo + the user's Rust sources), run
//! the *existing* `topo-check` binary against it, and surface the
//! verdict. No checking logic is reimplemented here — pure
//! orchestration, mirroring the Python `check.py`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::emit::emit_topo;
use crate::graph::Graph;

#[derive(Debug)]
pub struct CheckResult {
    pub passed: bool,
    pub returncode: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct CheckError(pub String);
impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for CheckError {}

fn topo_check_bin() -> Result<PathBuf, CheckError> {
    let rel = "topo-cli/tools/topo-check/topo-check";
    if let Ok(dir) = std::env::var("TOPO_BIN_DIR") {
        let base = PathBuf::from(&dir);
        let nested = base.join(rel);
        if nested.is_file() {
            return Ok(nested);
        }
        let flat = base.join("topo-check");
        if flat.is_file() {
            return Ok(flat);
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(root) = manifest
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    {
        let cand = root.join("build").join(rel);
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(CheckError(
        "could not locate `topo-check`; set TOPO_BIN_DIR".to_string(),
    ))
}

// The Rust host purity stress relies on the same forced PurityCheck the
// Python slice uses. `language = "rust"` routes the existing Rust
// analysis provider; `[purity] mode = "force"` mirrors the Python
// vertical slice's Topo.toml so the compliant-vs-violating parity is
// measured under the identical check regime.
//
// `ignore_patterns = ["build_app"]` is the Rust-host counterpart of the
// Python slice's module-level setup: a Rust topo-app needs an explicit
// registration scaffold fn (`build_app`), which is wiring, not a logic
// entry, so it is excluded from completeness exactly as constructors /
// `main` are. The handler fns themselves are real public logic entries
// and so must be `pub fn` in the host source — that visibility match is
// part of the contract, not something the orchestrator papers over.
const TOPO_TOML: &str = r#"[project]
name = "{name}"

[topo]
root = "topo/app.topo"

[build]
language = "rust"
sources = ["src/*.rs"]

[purity]
mode = "force"

[completeness]
ignore_constructors = true
ignore_main = true
ignore_patterns = ["build_app"]
"#;

// Distinct scratch directories per check() call: tests run concurrently
// and several share a namespace, so a PID+namespace path alone collides
// (one run's sources leak into another's `src/*.rs` glob).
static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn check(graph: &Graph, rust_sources: &[&Path]) -> Result<CheckResult, CheckError> {
    let bin = topo_check_bin()?;
    let name = if graph.namespace.is_empty() {
        "topo_app".to_string()
    } else {
        graph.namespace.clone()
    };

    let seq = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "topo-app-rust-check-{}-{}-{}",
        std::process::id(),
        seq,
        name
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("topo"))
        .and_then(|_| std::fs::create_dir_all(root.join("src")))
        .map_err(|e| CheckError(format!("scratch project mkdir failed: {e}")))?;

    std::fs::write(
        root.join("Topo.toml"),
        TOPO_TOML.replace("{name}", &name),
    )
    .map_err(|e| CheckError(format!("Topo.toml write failed: {e}")))?;
    std::fs::write(root.join("topo").join("app.topo"), emit_topo(graph))
        .map_err(|e| CheckError(format!(".topo write failed: {e}")))?;
    for src in rust_sources {
        let fname = src
            .file_name()
            .ok_or_else(|| CheckError("source has no file name".to_string()))?;
        std::fs::copy(src, root.join("src").join(fname))
            .map_err(|e| CheckError(format!("source copy failed: {e}")))?;
    }

    let out = Command::new(&bin)
        .arg("--project")
        .arg(&root)
        .output()
        .map_err(|e| CheckError(format!("failed to run topo-check: {e}")))?;
    let _ = std::fs::remove_dir_all(&root);

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    // topo-check's textual verdict is the source of truth (exit codes
    // are not always non-zero on a logical FAIL) — same rule as the
    // Python orchestrator.
    let passed = stdout.contains("Result: PASS");
    Ok(CheckResult {
        passed,
        returncode: out.status.code().unwrap_or(-1),
        stdout,
        stderr,
    })
}
