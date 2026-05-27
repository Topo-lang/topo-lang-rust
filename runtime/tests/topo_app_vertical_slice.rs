//! End-to-end acceptance for the topo-app Rust vertical slice.
//!
//! Mirrors `topo-lang-python/runtime/test/test_vertical_slice.py`
//! T1–T5. Round-trip / check cases invoke the *fresh* toolchain
//! binaries (`build/topo-core/tools/...`) by absolute path through the
//! crate's `toolchain` resolver; `TOPO_BIN_DIR` overrides if set.
//!
//! Run: `cargo test --test topo_app_vertical_slice`
//!
//! T4 violating case: an impure same-stage parallel handler (a `static
//! mut` write) is flagged by core PurityCheck — full parity with the
//! Python reference. The earlier Rust false negative (the L1
//! `RustSymbolAccessExtractor` leaked `inFunction` across a single-line
//! fn body and misattributed the write to the first single-line fn) is
//! fixed.

use std::io::Write;
use std::path::PathBuf;

use topo::app::{parallel, App};
use topo::emit::emit_topo;
use topo::record;
use topo::readback::read_topo;
use topo::schema::scalar;

/// The `OrderRec` used across the slice — parity with the Python
/// `topo.Record[("id", int), ("amount", float)]`. Field names are
/// written here because that is the only place they exist as tokens.
fn order_rec() -> topo::schema::TypeRef {
    record!(id: scalar::<i64>(), amount: scalar::<f64>())
}

/// Build the canonical sample app: parse -> validate -> persist.
fn build_app() -> App {
    let mut app = App::new("orders");

    // `parse`, `validate`, `persist` stay ordinary callables — declared
    // here purely to demonstrate the no-wrap contract (T1c uses them).
    fn parse(raw: String) -> (i64, f64) {
        (raw.len() as i64, 1.0)
    }
    let _ = parse; // ordinary fn; registration does not consume it

    topo::handler!(app, parse, in: scalar::<String>(), out: order_rec());
    topo::handler!(app, validate, in: order_rec(), out: order_rec());
    topo::handler!(app, persist, in: order_rec(), out: scalar::<bool>());

    app.flow("order_pipeline", vec!["parse".into(), "validate".into(), "persist".into()]);
    app
}

// --- T1: registration produces an enumerable graph -------------------

#[test]
fn t1_graph_enumerable() {
    let app = build_app();
    let g = app.graph();
    assert_eq!(g.namespace, "orders");
    assert_eq!(
        g.handlers.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
        vec!["parse", "validate", "persist"]
    );
    assert_eq!(
        g.handler("parse").unwrap().in_type.as_ref().unwrap().topo(),
        "string"
    );
    assert_eq!(
        g.handler("parse").unwrap().out_type.topo(),
        "record<id: i64, amount: f64>"
    );
    assert_eq!(g.handler("persist").unwrap().out_type.topo(), "bool");
    let flow = g.flow.as_ref().expect("flow registered");
    // parse->validate, validate->persist, persist->void
    assert_eq!(flow.edges.len(), 3);
}

#[test]
fn t1_source_handler_has_no_input() {
    // A no-input handler is a legal source handler (handler/flow spec).
    let mut app = App::new("src");
    topo::handler!(app, seed, out: scalar::<i64>());
    assert!(app.graph().handler("seed").unwrap().in_type.is_none());
}

#[test]
fn t1_handler_stays_independently_callable() {
    // The fn is never wrapped by registration: it stays a plain Rust
    // fn, callable with zero framework bootstrap.
    let mut app = App::new("x");
    fn double(n: i64) -> i64 {
        n * 2
    }
    topo::handler!(app, double, in: scalar::<i64>(), out: scalar::<i64>());
    assert_eq!(double(21), 42); // no bootstrap needed
}

// --- T2: emitted .topo parses under the fresh grammar ----------------

#[test]
fn t2_emitted_topo_parses() {
    let app = build_app();
    let text = app.app_config().emit_topo();
    assert!(text.contains("handler parse(string in_) -> record<id: i64, amount: f64>;"));
    assert!(text.contains("flow order_pipeline {"));
    // read_topo invokes the fresh `topo --ast-dump`; an Err is the
    // grammar-rejection signal. Ok proves the emitter output parses.
    let g2 = read_topo(&text).expect("emitted .topo must parse under fresh grammar");
    assert_eq!(g2.namespace, "orders");
}

// --- T3: graph -> .topo -> graph' semantic equivalence ---------------

#[test]
fn t3_semantic_equivalence() {
    let app = build_app();
    let g1 = app.graph();
    let g2 = app
        .app_config()
        .roundtrip()
        .expect("round-trip through fresh topo");
    assert!(
        g1.equivalent_to(&g2),
        "{:?} != {:?}",
        g1.semantic_key(),
        g2.semantic_key()
    );
}

#[test]
fn t3_hand_edit_survives_readback() {
    // The .topo is a view, not an opaque IR: reorder edges by hand,
    // read back, still semantically equivalent.
    let app = build_app();
    let text = app.app_config().emit_topo();
    let edited = text.replace(
        "      parse -> validate;\n      validate -> persist;",
        "      validate -> persist;\n      parse -> validate;",
    );
    assert_ne!(edited, text, "edit must actually change the text");
    let g2 = read_topo(&edited).expect("hand-edited .topo still parses");
    assert!(app.graph().equivalent_to(&g2));
}

// --- T4: zero-declaration check via the fresh topo-check -------------

const COMPLIANT_SRC: &str = r#"
pub fn parse(raw: i64) -> i64 { raw + 1 }
pub fn enrich(v: i64) -> i64 { v * 2 }
pub fn audit(v: i64) -> i64 { v }
pub fn total(v: i64) -> f64 { v as f64 + 0.5 }
"#;

// `audit` is a same-stage parallel candidate (sibling of `enrich`,
// `parse->{enrich,audit}->total`) and hides a module-global write —
// the impurity core PurityCheck must flag.
const VIOLATING_SRC: &str = r#"
static mut LOG: i64 = 0;

pub fn parse(raw: i64) -> i64 { raw + 1 }
pub fn enrich(v: i64) -> i64 { v * 2 }
pub fn audit(v: i64) -> i64 {
    unsafe {
        LOG += v;
    }
    v
}
pub fn total(v: i64) -> f64 { v as f64 + 0.5 }
"#;

/// The flow shared by both T4 cases: parse -> {enrich || audit} -> total.
fn build_parallel_app() -> App {
    let mut app = App::new("orders");
    topo::handler!(app, parse, in: scalar::<i64>(), out: scalar::<i64>());
    topo::handler!(app, enrich, in: scalar::<i64>(), out: scalar::<i64>());
    topo::handler!(app, audit, in: scalar::<i64>(), out: scalar::<i64>());
    topo::handler!(app, total, in: scalar::<i64>(), out: scalar::<f64>());
    app.flow(
        "pipeline",
        vec![
            "parse".into(),
            parallel(["enrich", "audit"]),
            "total".into(),
        ],
    );
    app
}

struct SrcFile {
    path: PathBuf,
}

impl Drop for SrcFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(self.path.parent().unwrap());
    }
}

fn write_src(name: &str, body: &str) -> SrcFile {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let n = C.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "topo-app-rust-t4-{}-{}",
        std::process::id(),
        n
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    SrcFile { path }
}

#[test]
fn t4_compliant_app_passes() {
    let app = build_parallel_app();
    let src = write_src("app.rs", COMPLIANT_SRC);
    let r = topo::check::check(&app, &[src.path.as_path()]).expect("check runs");
    assert!(r.passed, "stdout:\n{}\nstderr:\n{}", r.stdout, r.stderr);
}

#[test]
fn t4_violating_handler_is_flagged() {
    // Parity with the Python reference's T4-violating case: an impure
    // same-stage parallel handler in a flow is flagged by core
    // PurityCheck with zero hand-written .topo. The framework emits the
    // flow shape (parse -> {enrich || audit} -> total, terminal -> void)
    // and runs the real `topo-check`; `audit` hides a `static mut` write
    // and is a same-stage parallel candidate of `enrich`.
    let app = build_parallel_app();
    let src = write_src("app.rs", VIOLATING_SRC);
    let r = topo::check::check(&app, &[src.path.as_path()]).expect("check runs");

    // The framework emitted a well-formed, parseable flow with the
    // violating handler as a same-stage parallel candidate.
    let emitted = app.app_config().emit_topo();
    assert!(emitted.contains("parse -> enrich;"));
    assert!(emitted.contains("parse -> audit;"));
    assert!(emitted.contains("audit -> total;"));
    let g = read_topo(&emitted).expect("violating flow still parses under fresh grammar");
    assert!(g.flow.is_some());

    // The impure parallel handler must fail the check.
    assert!(
        !r.passed,
        "impure parallel handler must fail topo-check\nstdout:\n{}\nstderr:\n{}",
        r.stdout, r.stderr
    );
    assert!(
        r.stdout.contains("in parallel stage writes to global symbol"),
        "expected the parallel-stage purity diagnostic\nstdout:\n{}",
        r.stdout
    );
}

// --- T5: config(app) snapshot + emit ---------------------------------

#[test]
fn t5_snapshot_lists_full_graph() {
    let app = build_app();
    let snap = app.app_config().snapshot();
    assert_eq!(snap.namespace, "orders");
    assert_eq!(snap.handlers.len(), 3);
    let flow = snap.flow.expect("flow present");
    assert_eq!(flow.name, "order_pipeline");
    assert_eq!(flow.edges.len(), 3);
}

#[test]
fn t5_config_emit_equals_emitter_output() {
    let app = build_app();
    assert_eq!(app.app_config().emit_topo(), emit_topo(app.graph()));
}
