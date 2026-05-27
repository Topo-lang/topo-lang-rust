//! End-to-end acceptance for the topo-app Rust compile-time static path.
//!
//! Mirrors the Python vertical-slice acceptance
//! (`topo-lang-python/runtime/test/test_vertical_slice.py`) but through
//! the *static* producer: parse Rust with `syn` (no execution) ->
//! graph -> emit .topo -> read back via the real `topo` -> check via
//! the real `topo-check`.
//!
//! Requires the prebuilt toolchain. Set `TOPO_BIN_DIR` to the build
//! directory that contains `topo-core/tools/topo/topo` and
//! `topo-cli/tools/topo-check/topo-check`. When the binaries are not
//! resolvable (no `TOPO_BIN_DIR` set and neither path is present), the
//! tests that need the real toolchain SKIP cleanly with
//! `eprintln!` notice rather than hard-failing — mirroring the
//! lsp-integration-tests convention. Static-only tests (analyze /
//! emit / read_topo round-trip) always run.

use std::path::{Path, PathBuf};

use topo_app_static_rust::{analyze, check, emit_topo, read_topo};

/// Probe whether the bundled `topo` + `topo-check` binaries can be
/// resolved. Mirrors the lookup logic in
/// `topo-lang-rust/topo-build/static-frontend/src/check.rs`. Returns
/// the reason string when the toolchain is missing so the SKIP notice
/// names what was tried — per CLAUDE.md skip semantics, skips must
/// have a stated reason.
fn toolchain_available() -> Result<(), String> {
    let rel = "topo-cli/tools/topo-check/topo-check";
    if let Ok(dir) = std::env::var("TOPO_BIN_DIR") {
        let base = PathBuf::from(&dir);
        if base.join(rel).is_file() || base.join("topo-check").is_file() {
            return Ok(());
        }
        return Err(format!(
            "TOPO_BIN_DIR={dir:?} does not contain {rel} or topo-check"
        ));
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(root) = manifest
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    {
        let cand = root.join("build").join(rel);
        if cand.is_file() {
            return Ok(());
        }
        return Err(format!(
            "no TOPO_BIN_DIR set and {} not built", cand.display()
        ));
    }
    Err("could not determine workspace root; TOPO_BIN_DIR not set".to_string())
}

/// Macro: SKIP with a printed reason when the toolchain is missing.
/// Mirrors the rust-analyzer / clangd integration-test convention. The
/// `eprintln!` line surfaces under `cargo test` so the skip is visible,
/// not silent.
macro_rules! skip_unless_toolchain {
    () => {
        if let Err(reason) = toolchain_available() {
            eprintln!(
                "SKIPPED: real toolchain unavailable: {reason}. \
                 Build with `cmake --build build` or set TOPO_BIN_DIR=/path/to/build."
            );
            return;
        }
    };
}

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", p.display()))
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

// --- T1: registration produces an enumerable graph (static) ----------

#[test]
fn graph_enumerable_from_static_scan() {
    let g = analyze(&fixture("vertical_slice.rs")).expect("analyze");
    assert_eq!(g.namespace, "orders");
    let names: Vec<&str> = g.handlers.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["parse", "validate", "persist"]);

    let parse = g.handler("parse").expect("parse handler");
    assert_eq!(parse.in_type.as_ref().unwrap().topo(), "string");
    assert_eq!(
        parse.out_type.topo(),
        "record<id: i64, amount: f64>"
    );
    assert_eq!(g.handler("persist").unwrap().out_type.topo(), "bool");

    let flow = g.flow.as_ref().expect("flow");
    assert_eq!(flow.name, "order_pipeline");
    // parse->validate, validate->persist, persist->void
    assert_eq!(flow.edges.len(), 3);
}

#[test]
fn source_handler_has_no_input() {
    // A no-parameter handler is a legal source handler (spec §7a).
    let src = r#"
        fn seed() -> i64 { 0 }
        fn build_app() -> topo::App {
            let mut app = topo::App::new("src");
            app.handler(seed);
            app
        }
    "#;
    let g = analyze(src).expect("analyze");
    assert!(g.handler("seed").unwrap().in_type.is_none());
}

#[test]
fn multi_input_handler_is_rejected_not_silently_dropped() {
    // A handler is a pure Functor: at most one input. The static path
    // refuses the program rather than emit a graph that hides the
    // violation.
    let src = r#"
        fn bad(a: i64, b: i64) -> i64 { a + b }
        fn build_app() -> topo::App {
            let mut app = topo::App::new("x");
            app.handler(bad);
            app
        }
    "#;
    let err = analyze(src).expect_err("must reject multi-input handler");
    assert!(err.0.contains("at most one input"), "{}", err.0);
}

// --- T2: emitted .topo parses under the merged grammar ---------------

#[test]
fn emitted_topo_parses_and_has_expected_form() {
    skip_unless_toolchain!();
    let g = analyze(&fixture("vertical_slice.rs")).expect("analyze");
    let text = emit_topo(&g);
    assert!(
        text.contains(
            "handler parse(string in) -> record<id: i64, amount: f64>;"
        ),
        "emitted:\n{text}"
    );
    assert!(text.contains("flow order_pipeline {"), "emitted:\n{text}");
    // read_topo() errors if `topo` rejects the source — a successful
    // parse is itself the grammar-conformance proof.
    let g2 = read_topo(&text).expect("emitted .topo must parse under topo");
    assert_eq!(g2.namespace, "orders");
}

// --- T3: round-trip graph == graph' (headline) -----------------------

#[test]
fn static_roundtrip_is_semantically_equivalent() {
    skip_unless_toolchain!();
    let g1 = analyze(&fixture("vertical_slice.rs")).expect("analyze");
    let text = emit_topo(&g1);
    let g2 = read_topo(&text).expect("read back");
    assert!(
        g1.equivalent_to(&g2),
        "graph != graph'\n  g1 = {:?}\n  g2 = {:?}",
        g1.semantic_key(),
        g2.semantic_key()
    );
}

#[test]
fn hand_edit_survives_readback() {
    skip_unless_toolchain!();
    // The .topo is a view, not an opaque IR: reorder edges by hand,
    // read back, still semantically equivalent.
    let g = analyze(&fixture("vertical_slice.rs")).expect("analyze");
    let text = emit_topo(&g);
    let edited = text.replace(
        "      parse -> validate;\n      validate -> persist;",
        "      validate -> persist;\n      parse -> validate;",
    );
    assert_ne!(edited, text, "the hand-edit must actually change the text");
    let g2 = read_topo(&edited).expect("edited .topo must still parse");
    assert!(g.equivalent_to(&g2));
}

// --- T3b: static path == runtime-bridge form (cross-producer parity) -

#[test]
fn static_output_matches_runtime_bridge_emitter_form() {
    // The runtime bridge (Python `_emit.py`) and the static path emit
    // the same structural surface; only the host-scalar preamble differs
    // (Rust binds std::rust::*, Python std::python::*). Assert the body
    // is identical line-for-line after dropping the comment header and
    // the 4 preamble lines, so the "just another `.topo` producer"
    // claim is verified, not assumed.
    let g = analyze(&fixture("vertical_slice.rs")).expect("analyze");
    let text = emit_topo(&g);
    let body: Vec<&str> = text
        .lines()
        .skip_while(|l| l.starts_with("//") || l.is_empty())
        .skip_while(|l| l.starts_with("using ") || l.is_empty())
        .collect();
    let expected_body = [
        "namespace orders {",
        "  public:",
        "    handler parse(string in) -> record<id: i64, amount: f64>;",
        "    handler validate(record<id: i64, amount: f64> in) -> record<id: i64, amount: f64>;",
        "    handler persist(record<id: i64, amount: f64> in) -> bool;",
        "",
        "    flow order_pipeline {",
        "      parse -> validate;",
        "      validate -> persist;",
        "      persist -> void;",
        "    }",
        "}",
        // `str::lines()` does not yield a trailing element for the final
        // newline, so the body ends at the closing brace.
    ];
    assert_eq!(body, expected_body, "full emitted text:\n{text}");
}

// --- T4: zero-declaration check via the existing topo-check ----------

#[test]
fn compliant_app_passes_zero_declaration_check() {
    skip_unless_toolchain!();
    let g = analyze(&fixture("compliant.rs")).expect("analyze");
    let src = fixture_path("compliant.rs");
    let srcs: Vec<&Path> = vec![src.as_path()];
    let r = check(&g, &srcs).expect("run topo-check");
    assert!(
        r.passed,
        "compliant app must pass zero-declaration check\n--- stdout ---\n{}\n--- stderr ---\n{}",
        r.stdout, r.stderr
    );
}

#[test]
fn violating_app_emits_same_surface_as_compliant() {
    // The purity violation lives only in the handler body; the
    // registration surface and therefore the emitted .topo are
    // byte-identical to the compliant case. This half always holds and
    // proves the static path faithfully reflects the program structure.
    let gc = analyze(&fixture("compliant.rs")).expect("analyze compliant");
    let gv = analyze(&fixture("violating.rs")).expect("analyze violating");
    assert_eq!(emit_topo(&gc), emit_topo(&gv));
}

#[test]
fn violating_app_is_flagged_by_zero_declaration_check() {
    skip_unless_toolchain!();
    // Headline T4 acceptance for the Rust static path: with zero
    // hand-written .topo, the hidden parallel-stage global write in
    // `audit` is FLAGGED by the existing forced PurityCheck.
    //
    // The Python vertical slice gates the equivalent assertion behind a
    // known issue where the *Python* host symbol-access extractor reports
    // bare names that never match the flow's namespace-qualified
    // `calledFunctions`. Verified by tool-backed repro that the **Rust**
    // host does NOT exhibit that mismatch (the Rust symbol-access path
    // produces name-matchable records), so for the static path the
    // violation is genuinely detected — full parity, asserted as a real
    // pass, not gated and not faked. Should the Rust host ever regress
    // into the same false-negative, this test fails loudly rather than
    // silently certifying unsafe code.
    let g = analyze(&fixture("violating.rs")).expect("analyze");
    let src = fixture_path("violating.rs");
    let srcs: Vec<&Path> = vec![src.as_path()];
    let r = check(&g, &srcs).expect("run topo-check");
    assert!(
        !r.passed,
        "violating handler's hidden global write must be flagged by \
         topo-check\n--- stdout ---\n{}\n--- stderr ---\n{}",
        r.stdout, r.stderr
    );
    assert!(
        r.stdout.contains("parallel stage")
            && r.stdout.contains("audit"),
        "expected the purity violation on `audit` to be reported\n\
         --- stdout ---\n{}",
        r.stdout
    );
}
