//! Integration tests for ``topo-extract-rust``'s wire protocol contract.
//!
//! Spawns the built binary, drives it through stdin/stdout, and asserts
//! the structured exit codes and JSON error envelopes documented in
//! ``main.rs``. Pins two hardening guarantees:
//! - malformed stdin produces no panic — a JSON error envelope and a
//!   stable non-zero exit instead;
//! - the per-file size cap path: an oversize file becomes a per-file
//!   unsupported entry rather than an OOM.
//!
//! ``cargo test`` rebuilds the binary on demand via
//! ``env!("CARGO_BIN_EXE_topo-extract-rust")``.

use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_topo-extract-rust")
}

fn run_with_stdin(input: &str) -> (i32, String, String) {
    let mut child = Command::new(bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn topo-extract-rust");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait child");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn malformed_json_exits_with_structured_envelope() {
    // Pre-fix this panicked with ``thread 'main' panicked at 'failed to
    // parse JSON request from stdin'`` and a SIGABRT-shape exit; the
    // caller saw no parseable diagnostic. The fix emits an envelope on
    // stdout and uses ``EXIT_REQUEST_PARSE = 3`` so the caller can
    // route the error structurally.
    let (code, stdout, stderr) = run_with_stdin("{this is not json}");
    assert_eq!(code, 3, "expected EXIT_REQUEST_PARSE; got {}", code);
    let env: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("stdout should be JSON envelope");
    assert_eq!(env["kind"], "request-parse");
    assert!(env["error"].as_str().unwrap().contains("parse"),
            "error should mention parse failure: {}", env);
    assert!(stderr.contains("request-parse"),
            "stderr should mirror the kind: {}", stderr);
}

#[test]
fn empty_stdin_parses_as_missing_required_fields() {
    // Empty stdin is invalid JSON for the request shape (serde fails
    // on missing required fields); same envelope contract applies.
    let (code, stdout, _stderr) = run_with_stdin("");
    assert_eq!(code, 3, "empty input should be request-parse");
    let env: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("stdout should be JSON envelope");
    assert_eq!(env["kind"], "request-parse");
}

#[test]
fn valid_empty_request_succeeds_with_empty_module() {
    // Sanity: the success path still yields an EXIT_OK and the TranspileModule
    // JSON shape.
    let req = r#"{"files":[], "functions":[]}"#;
    let (code, stdout, _stderr) = run_with_stdin(req);
    assert_eq!(code, 0, "empty files request should succeed");
    let module: serde_json::Value = serde_json::from_str(stdout.trim())
        .expect("stdout should be TranspileModule JSON");
    assert!(module["functions"].is_array());
    assert!(module["types"].is_array());
}

#[test]
fn oversize_file_becomes_unsupported_entry_not_oom() {
    // Drive the size cap path: write a 16 KiB Rust source under a temp
    // dir, set ``TOPO_EXTRACT_RUST_MAX_FILE_BYTES=4096`` so the file is
    // ~4x over cap. The extractor should reject the read and continue
    // (the module's functions/types arrays come back empty) rather than
    // OOM on a pathologically large input.
    let tmp = std::env::temp_dir().join(format!(
        "topo-extract-rust-oversize-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)));
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("big.rs");
    std::fs::write(&path, "fn placeholder() {}\n".repeat(800)).unwrap();
    let req = serde_json::json!({
        "files": [path.file_name().unwrap().to_string_lossy()],
        "functions": [],
    })
    .to_string();

    let mut child = Command::new(bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("TOPO_EXTRACT_ROOT", &tmp)
        .env("TOPO_EXTRACT_RUST_MAX_FILE_BYTES", "4096")
        .spawn()
        .expect("spawn topo-extract-rust");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(req.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait child");
    let _ = std::fs::remove_dir_all(&tmp);

    assert_eq!(out.status.code().unwrap_or(-1), 0,
               "oversize file is a per-file reject, not a fatal exit");
    let module: serde_json::Value = serde_json::from_str(
        &String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert!(module["functions"].as_array().unwrap().is_empty(),
            "no functions should be lifted from the rejected file");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("exceeds cap"),
            "stderr should explain the per-file rejection: {}", stderr);
}
