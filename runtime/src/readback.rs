//! Read `.topo` back into a [`Graph`] by parsing it with the real
//! toolchain.
//!
//! Round-trip fidelity is the topo-app design's decisive constraint. To prove
//! it honestly, read-back goes through the *actual* Topo parser, not a
//! Rust re-implementation of the grammar (which could agree with the
//! emitter by accident). We invoke the fresh `topo --ast-dump` and
//! reconstruct the graph from the parser's own structured dump. This
//! simultaneously proves "emitted .topo parses under the grammar" (the
//! dump only succeeds if the parser accepts it) and yields graph' for
//! the equivalence check. Port of `_readback.py`.
//!
//! No regex crate is pulled in: the AST dump lines have a fixed,
//! single-quote-delimited shape, so hand-written slicing is both
//! sufficient and dependency-free (the config port set the precedent of
//! adding crate deps only when a first-class capability is needed).

use std::io::Write;
use std::process::Command;

use crate::graph::{Edge, Flow, Graph, Handler};
use crate::schema::TypeRef;
use crate::toolchain::topo_bin;

/// Error surface for read-back: either the toolchain rejected the
/// source (the grammar-conformance signal) or the dump was unreadable.
#[derive(Debug)]
pub enum ReadbackError {
    /// `topo --ast-dump` exited non-zero — the emitted `.topo` did not
    /// parse. Carries the captured stdout+stderr for the assertion
    /// message.
    ParseRejected { code: i32, output: String },
    Io(std::io::Error),
}

impl std::fmt::Display for ReadbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadbackError::ParseRejected { code, output } => {
                write!(f, "topo --ast-dump rejected source (exit {code}):\n{output}")
            }
            ReadbackError::Io(e) => write!(f, "readback io error: {e}"),
        }
    }
}

impl std::error::Error for ReadbackError {}

impl From<std::io::Error> for ReadbackError {
    fn from(e: std::io::Error) -> Self {
        ReadbackError::Io(e)
    }
}

/// Parse a record `.topo` spelling (`record<a: T, b: U>`) or a scalar.
///
/// Record fields in the slice are scalar-typed (one nesting level,
/// matching the topo-app design's order example), so a top-level comma split is
/// sufficient — identical assumption to `_readback.py::_parse_type`.
fn parse_type(spec: &str) -> TypeRef {
    let spec = spec.trim();
    if let Some(inner) = spec
        .strip_prefix("record<")
        .and_then(|s| s.strip_suffix('>'))
    {
        let fields = inner
            .split(',')
            .map(|part| {
                let (name, ftype) = part.split_once(':').unwrap_or((part, ""));
                (
                    name.trim().to_string(),
                    // Field types here are scalars; the spelling is
                    // carried through verbatim so emit/readback agree.
                    TypeRef::Scalar(leak(ftype.trim())),
                )
            })
            .collect();
        return TypeRef::Record(fields);
    }
    TypeRef::Scalar(leak(spec))
}

/// Intern a type-spelling string into a `&'static str`.
///
/// `TypeRef::Scalar` holds `&'static str` (the emit side uses string
/// literals). Read-back spellings come from parser output at runtime;
/// interning them keeps `TypeRef` uniform without widening its type.
///
/// Each distinct spelling is leaked **at most once** — subsequent
/// observations of the same spelling re-use the previously-leaked
/// pointer rather than allocating a new heap block. The earlier
/// `Box::leak(s.to_string().into_boxed_str())` per call was unbounded
/// under pathological input (a test loop over distinct record shapes
/// would leak N strings per N readbacks); the bounded form here caps
/// the leak at the count of distinct spellings the program ever sees.
fn leak(s: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let pool = POOL.get_or_init(|| Mutex::new(HashSet::new()));
    // Hold the lock across both the get and the insert so two
    // concurrent readbacks never leak the same spelling twice.
    let mut guard = pool.lock().expect("readback intern pool poisoned");
    if let Some(&existing) = guard.get(s) {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    guard.insert(leaked);
    leaked
}

/// Extract the text between the first `'` and the last `'` on a line.
fn quoted(s: &str) -> Option<&str> {
    let start = s.find('\'')? + 1;
    let end = s.rfind('\'')?;
    if end > start {
        Some(&s[start..end])
    } else {
        None
    }
}

/// Parse `.topo` source text into a [`Graph`] via the fresh
/// `topo --ast-dump`.
///
/// Returns [`ReadbackError::ParseRejected`] if the toolchain rejects the
/// source — that rejection *is* the grammar-conformance signal.
pub fn read_topo(text: &str) -> Result<Graph, ReadbackError> {
    let dir = tempdir()?;
    let path = dir.path.join("roundtrip.topo");
    {
        let mut f = std::fs::File::create(&path)?;
        f.write_all(text.as_bytes())?;
    }

    let out = Command::new(topo_bin())
        .arg("--ast-dump")
        .arg(&path)
        .output()?;

    if !out.status.success() {
        let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
        combined.push_str(&String::from_utf8_lossy(&out.stderr));
        return Err(ReadbackError::ParseRejected {
            code: out.status.code().unwrap_or(-1),
            output: combined,
        });
    }

    let dump = String::from_utf8_lossy(&out.stdout);
    let mut namespace = String::new();
    let mut handlers: Vec<Handler> = Vec::new();
    let mut flow: Option<Flow> = None;

    for line in dump.lines() {
        let s = line.trim();
        if let Some(rest) = s.strip_prefix("NamespaceDecl ") {
            if let Some(n) = quoted(rest) {
                namespace = n.to_string();
            }
            continue;
        }
        if let Some(rest) = s.strip_prefix("HandlerDecl ") {
            // `name(params) -> ret`
            if let Some(decl) = quoted(rest) {
                let (sig, ret) = match decl.split_once("->") {
                    Some((a, b)) => (a.trim(), b.trim()),
                    None => continue,
                };
                let open = sig.find('(');
                let close = sig.rfind(')');
                if let (Some(o), Some(c)) = (open, close) {
                    let name = sig[..o].trim().to_string();
                    let params = sig[o + 1..c].trim();
                    let in_type = if params.is_empty() {
                        None
                    } else {
                        // "<Type> <paramName>" — strip the trailing
                        // identifier (record<...> may contain spaces, so
                        // split on the *last* whitespace).
                        let type_spec = match params.rfind(char::is_whitespace) {
                            Some(idx) => params[..idx].trim(),
                            None => params,
                        };
                        Some(parse_type(type_spec))
                    };
                    handlers.push(Handler {
                        name,
                        in_type,
                        out_type: parse_type(ret),
                    });
                }
            }
            continue;
        }
        if let Some(rest) = s.strip_prefix("FlowBlock ") {
            if let Some(n) = quoted(rest) {
                flow = Some(Flow {
                    name: n.to_string(),
                    edges: Vec::new(),
                });
            }
            continue;
        }
        if let Some(rest) = s.strip_prefix("Edge ") {
            if let Some(f) = flow.as_mut() {
                // `src -> tgt` optionally followed by `[terminal]`.
                let core = rest.split('[').next().unwrap_or(rest).trim();
                if let Some((src, tgt)) = core.split_once("->") {
                    let src = src.trim().to_string();
                    let tgt = tgt.trim();
                    f.edges.push(Edge {
                        source: src,
                        target: if tgt == "void" {
                            None
                        } else {
                            Some(tgt.to_string())
                        },
                    });
                }
            }
            continue;
        }
    }

    Ok(Graph {
        namespace,
        handlers,
        flow,
    })
}

// --- minimal temp-dir guard (no tempfile crate dependency) -----------

struct TempDir {
    path: std::path::PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best-effort cleanup; a leaked temp dir is harmless and never
        // worth panicking in a destructor over.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn tempdir() -> std::io::Result<TempDir> {
    // A process-unique directory under the OS temp root. PID + a
    // monotonic counter avoids collisions across concurrent test
    // binaries without pulling in a uuid/tempfile dependency.
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "topo-app-rust-{}-{}",
        std::process::id(),
        n
    ));
    std::fs::create_dir_all(&base)?;
    Ok(TempDir { path: base })
}

#[cfg(test)]
mod tests {
    use super::leak;

    /// Same spelling twice → same `&'static str` pointer (no double leak).
    #[test]
    fn leak_returns_identical_pointer_for_repeated_spellings() {
        let a = leak("i64");
        let b = leak("i64");
        assert_eq!(a.as_ptr(), b.as_ptr(),
            "interner must return the same pointer for repeated spellings");
    }

    /// Distinct spellings → distinct pointers (the interner does not collide).
    #[test]
    fn leak_returns_distinct_pointers_for_distinct_spellings() {
        let a = leak("u64");
        let b = leak("f64");
        assert_ne!(a.as_ptr(), b.as_ptr(),
            "distinct spellings must get distinct interned pointers");
    }
}
