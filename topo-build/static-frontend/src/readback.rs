//! Read `.topo` back into a `Graph` by parsing it with the *real*
//! toolchain.
//!
//! Round-trip fidelity is the topo-app design's decisive constraint. To prove
//! it honestly, read-back must go through the actual Topo parser, not a
//! Rust re-implementation of the grammar (which could agree with the
//! emitter by accident). We invoke `topo --ast-dump` and reconstruct
//! the graph from the parser's own structured dump — the same strategy
//! the Python `_readback.py` uses. A non-zero exit is itself the
//! grammar-conformance failure signal (no silent degradation).

use std::path::PathBuf;
use std::process::Command;

use crate::graph::{Edge, Flow, Graph, Handler, TypeRef};

#[derive(Debug)]
pub struct ReadbackError(pub String);

impl std::fmt::Display for ReadbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ReadbackError {}

/// Locate the prebuilt `topo` binary. Resolution: `TOPO_BIN_DIR` env
/// (used by the tests / CI), then a sibling `build` tree. A clear error
/// is raised if neither yields the binary — silently degrading a
/// correctness tool would defeat the point (same policy as the Python
/// `_toolchain.py`).
fn topo_bin() -> Result<PathBuf, ReadbackError> {
    let rel = "topo-core/tools/topo/topo";
    if let Ok(dir) = std::env::var("TOPO_BIN_DIR") {
        let base = PathBuf::from(&dir);
        let nested = base.join(rel);
        if nested.is_file() {
            return Ok(nested);
        }
        let flat = base.join("topo");
        if flat.is_file() {
            return Ok(flat);
        }
    }
    // Sibling build tree: this crate lives at
    // topo-lang-rust/topo-build/static-frontend; the repo root is three
    // parents up from CARGO_MANIFEST_DIR.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(root) = manifest.parent().and_then(|p| p.parent()).and_then(|p| p.parent())
    {
        let cand = root.join("build").join(rel);
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(ReadbackError(
        "could not locate `topo` binary; set TOPO_BIN_DIR to the build \
         directory containing topo-core/tools/topo/topo"
            .to_string(),
    ))
}

/// Parse one `record<f: T, ...>` / scalar type spelling out of an
/// `--ast-dump` type fragment. Record nesting is one level in the
/// vertical slice (matching the topo-app design's order example), so a
/// top-level comma split is sufficient.
fn parse_type(spec: &str) -> TypeRef {
    let spec = spec.trim();
    if let Some(inner) = spec
        .strip_prefix("record<")
        .and_then(|s| s.strip_suffix('>'))
    {
        let mut fields = Vec::new();
        for part in inner.split(',') {
            let (name, ty) = part.split_once(':').unwrap_or((part, ""));
            fields.push((
                name.trim().to_string(),
                TypeRef::Scalar(ty.trim().to_string()),
            ));
        }
        return TypeRef::Record(fields);
    }
    TypeRef::Scalar(spec.to_string())
}

/// Parse `.topo` source text into a `Graph` via `topo --ast-dump`.
/// Returns an error if the toolchain rejects the source — that
/// rejection is the grammar-conformance proof.
pub fn read_topo(text: &str) -> Result<Graph, ReadbackError> {
    let bin = topo_bin()?;
    let tmp = std::env::temp_dir().join(format!(
        "topo-app-rust-roundtrip-{}.topo",
        std::process::id()
    ));
    std::fs::write(&tmp, text)
        .map_err(|e| ReadbackError(format!("temp write failed: {e}")))?;

    let out = Command::new(&bin)
        .arg("--ast-dump")
        .arg(&tmp)
        .output()
        .map_err(|e| ReadbackError(format!("failed to run topo: {e}")))?;
    let _ = std::fs::remove_file(&tmp);

    if !out.status.success() {
        return Err(ReadbackError(format!(
            "topo --ast-dump rejected the emitted .topo (grammar \
             non-conformance):\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    let dump = String::from_utf8_lossy(&out.stdout);
    let mut namespace = String::new();
    let mut handlers: Vec<Handler> = Vec::new();
    let mut flow: Option<Flow> = None;

    for line in dump.lines() {
        let s = line.trim();
        if let Some(rest) = s.strip_prefix("NamespaceDecl '") {
            if let Some(name) = rest.strip_suffix('\'') {
                namespace = name.to_string();
            }
        } else if let Some(rest) = s.strip_prefix("HandlerDecl '") {
            // `name(<params>) -> <ret>'`
            let body = rest.strip_suffix('\'').unwrap_or(rest);
            if let Some((sig, ret)) = body.split_once("->") {
                let sig = sig.trim();
                let ret = ret.trim();
                let name = sig.split('(').next().unwrap_or("").trim().to_string();
                let params = sig
                    .split_once('(')
                    .and_then(|(_, r)| r.strip_suffix(')'))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let in_type = if params.is_empty() {
                    None
                } else {
                    // "<Type> in" — strip the conventional param name.
                    let tspec = params
                        .rsplit_once(' ')
                        .map(|(t, _)| t)
                        .unwrap_or(&params);
                    Some(parse_type(tspec))
                };
                handlers.push(Handler {
                    name,
                    in_type,
                    out_type: parse_type(ret),
                });
            }
        } else if let Some(rest) = s.strip_prefix("FlowBlock '") {
            if let Some(name) = rest.strip_suffix('\'') {
                flow = Some(Flow {
                    name: name.to_string(),
                    edges: Vec::new(),
                });
            }
        } else if let Some(rest) = s.strip_prefix("Edge ") {
            if let Some(f) = flow.as_mut() {
                // `src -> tgt` optionally ` [terminal]`
                let core = rest.split('[').next().unwrap_or(rest).trim();
                if let Some((src, tgt)) = core.split_once("->") {
                    let tgt = tgt.trim();
                    f.edges.push(Edge {
                        source: src.trim().to_string(),
                        target: if tgt == "void" {
                            None
                        } else {
                            Some(tgt.to_string())
                        },
                    });
                }
            }
        }
    }

    Ok(Graph {
        namespace,
        handlers,
        flow,
    })
}
