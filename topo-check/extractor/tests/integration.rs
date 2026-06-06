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

/// Run the extractor over an in-memory Rust source and return the parsed
/// TranspileModule JSON. The source is written under a private temp root and
/// `TOPO_EXTRACT_ROOT` is pinned to it so the path sanitiser accepts the file.
fn extract_source(source: &str) -> serde_json::Value {
    let tmp = std::env::temp_dir().join(format!(
        "topo-extract-rust-wire-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)));
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("input.rs");
    std::fs::write(&path, source).unwrap();
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
    assert_eq!(out.status.code().unwrap_or(-1), 0, "extractor should exit 0");
    serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
        .expect("stdout should be TranspileModule JSON")
}

/// Find the single statement in `read_u8`-style single-function modules.
fn only_fn_body(module: &serde_json::Value) -> &Vec<serde_json::Value> {
    let fns = module["functions"].as_array().expect("functions array");
    assert_eq!(fns.len(), 1, "expected exactly one function: {}", module);
    fns[0]["body"].as_array().expect("body array")
}

/// Regression for the extractor -> C++ deserializer wire contract
/// (extractor-to-cpp-deserializer-contract-mismatch). The C++
/// `deserializeModule` discriminates on word-form operators, reads `litKind`
/// with value `"boolean"`, requires `vardecl.type` to be a TypeNode (it does
/// `j.at("type")` then `typeNodeFromJson` -> `j.at("nameParts")`), and follows
/// an omit-when-absent convention for `init` / `value`. Emitting symbolic ops,
/// `"bool"`, or an explicit `null` type/init/value either silently mis-maps to
/// the wrong C++ enum or throws and drops the whole module. This asserts the
/// corrected encodings directly on extractor output.
#[test]
fn compound_assign_emits_word_form_op() {
    // `x += 1` must serialize as `op:"add"`, NOT the symbolic `"+"` (which
    // the C++ BinaryOp deserializer maps to Shr -> `x >>= 1`).
    let module = extract_source("pub fn f(mut x: i32) { x += 1; }\n");
    let body = only_fn_body(&module);
    let stmt = &body[0];
    assert_eq!(stmt["kind"], "compoundassign");
    assert_eq!(stmt["op"], "add",
               "compound-assign op must be word-form, not symbolic: {}", stmt);
}

#[test]
fn untyped_local_emits_empty_type_node_not_null() {
    // `let x = b;` must emit `type: {"nameParts": []}`, never `type: null`
    // (the C++ VarDecl deserializer unconditionally feeds `type` to
    // typeNodeFromJson, which reads `nameParts` and throws on null).
    let module = extract_source("pub fn f(b: u8) -> u8 { let x = b; x }\n");
    let body = only_fn_body(&module);
    let vardecl = &body[0];
    assert_eq!(vardecl["kind"], "vardecl");
    assert!(!vardecl["type"].is_null(),
            "untyped local must not emit `type: null`: {}", vardecl);
    assert!(vardecl["type"]["nameParts"].is_array(),
            "untyped local type must be a TypeNode with nameParts: {}", vardecl);
}

#[test]
fn uninitialised_local_omits_init_key() {
    // `let x;` must drop the `init` key entirely (the C++ side guards with
    // `j.contains("init")` and would call deserializeExpr(null) on an
    // explicit `"init": null`, throwing).
    let module = extract_source("pub fn f() { let x; let _ = x; }\n");
    let body = only_fn_body(&module);
    let vardecl = &body[0];
    assert_eq!(vardecl["kind"], "vardecl");
    assert!(vardecl.get("init").is_none(),
            "uninitialised local must omit `init`, not emit null: {}", vardecl);
}

#[test]
fn bare_return_omits_value_key() {
    // `return;` must drop the `value` key (same contains-then-at("kind")
    // throw on the C++ side as init above).
    let module = extract_source("pub fn f() { return; }\n");
    let body = only_fn_body(&module);
    let ret = &body[0];
    assert_eq!(ret["kind"], "return");
    assert!(ret.get("value").is_none(),
            "bare return must omit `value`, not emit null: {}", ret);
}

#[test]
fn bool_literal_uses_recognised_kind() {
    // `true` must emit `litKind:"boolean"` (the C++ LiteralKind deserializer
    // maps the unknown `"bool"` to Integer).
    let module = extract_source("pub fn f() -> bool { true }\n");
    let body = only_fn_body(&module);
    let ret = &body[0];
    assert_eq!(ret["kind"], "return");
    assert_eq!(ret["value"]["litKind"], "boolean",
               "bool literal must use the recognised `boolean` kind: {}", ret);
}

#[test]
fn borrow_and_deref_surface_as_unsupported() {
    // `&x` / `&mut x` / `*p` have no faithful UnaryOp; mapping them to a
    // recognised op silently corrupts semantics (borrow -> negate, deref ->
    // logical-not). They must surface as `unsupported`.
    let module = extract_source(
        "pub fn f(x: i32, p: i32) { let _r = &x; let _m = &mut x; let _d = *p; }\n");
    let body = only_fn_body(&module);
    for (i, name) in ["&", "&mut", "*"].iter().enumerate() {
        let init = &body[i]["init"];
        assert_eq!(init["kind"], "unsupported",
                   "{} should surface as unsupported, not a mis-mapped op: {}",
                   name, init);
    }
}
