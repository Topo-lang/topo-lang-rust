//! Zero-declaration check: hand the existing `topo-check` the
//! framework-emitted `.topo`.
//!
//! The user writes no `.topo` by hand. We materialise a throwaway
//! project (Topo.toml + emitted `.topo` + the caller's Rust sources),
//! run the *existing fresh* `topo-check` against it, and surface the
//! verdict. No checking logic is reimplemented here; this is pure
//! orchestration — port of `check.py`.
//!
//! The Rust check path shells out to `topo-extract-rust` (the L1
//! symbol-access analyser the purity check consumes). `topo-check`
//! resolves it on `PATH`; this bridge prepends the extractor's release
//! directory so a zero-config caller still gets purity analysis. (A
//! compliant flow passes; a parallel handler that writes a `static mut`
//! global is flagged by core `PurityCheck`. Note: the Rust L1 extractor
//! detects the write only in the multi-line `unsafe { \n NAME ... \n }`
//! idiom — see the parity note in the integration suite.)

use std::io::Write;
use std::path::Path;
use std::process::Command;

use crate::app::App;
use crate::emit::emit_topo;
use crate::toolchain::{rust_extractor_dir, topo_check_bin};

/// The verdict of a zero-declaration check run.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub passed: bool,
    pub returncode: i32,
    pub stdout: String,
    pub stderr: String,
}

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
"#;

/// Run `topo-check` on the framework-emitted `.topo` against the given
/// Rust source files. No hand-written `.topo` anywhere in the flow.
pub fn check(app: &App, rust_sources: &[&Path]) -> std::io::Result<CheckResult> {
    let name = if app.graph().namespace.is_empty() {
        "topo_app".to_string()
    } else {
        app.graph().namespace.clone()
    };

    let dir = scratch_dir()?;
    let root = &dir.path;
    std::fs::create_dir_all(root.join("topo"))?;
    std::fs::create_dir_all(root.join("src"))?;

    {
        let mut f = std::fs::File::create(root.join("Topo.toml"))?;
        f.write_all(TOPO_TOML.replace("{name}", &name).as_bytes())?;
    }
    {
        let mut f = std::fs::File::create(root.join("topo").join("app.topo"))?;
        f.write_all(emit_topo(app.graph()).as_bytes())?;
    }
    for src in rust_sources {
        let leaf = src
            .file_name()
            .map(|s| s.to_owned())
            .unwrap_or_else(|| std::ffi::OsString::from("src.rs"));
        std::fs::copy(src, root.join("src").join(leaf))?;
    }

    // Prepend the Rust extractor dir to PATH so topo-check can shell out
    // to topo-extract-rust without the caller pre-configuring it. The
    // separator must be platform-aware: `:` on Unix, `;` on Windows.
    let mut path_var = rust_extractor_dir().as_os_str().to_owned();
    if let Some(existing) = std::env::var_os("PATH") {
        path_var.push(if cfg!(windows) { ";" } else { ":" });
        path_var.push(existing);
    }

    let out = Command::new(topo_check_bin())
        .arg("--project")
        .arg(root)
        .env("PATH", path_var)
        .output()?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    // topo-check's textual verdict is the source of truth: exit codes
    // are not always non-zero on a logical FAIL (same contract as
    // `check.py`).
    let passed = stdout.contains("Result: PASS");

    Ok(CheckResult {
        passed,
        returncode: out.status.code().unwrap_or(-1),
        stdout,
        stderr,
    })
}

// --- scratch dir guard (shared shape with readback, kept local so the
// modules stay independently readable) -------------------------------

struct ScratchDir {
    path: std::path::PathBuf,
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn scratch_dir() -> std::io::Result<ScratchDir> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "topo-app-rust-check-{}-{}",
        std::process::id(),
        n
    ));
    std::fs::create_dir_all(&base)?;
    Ok(ScratchDir { path: base })
}
