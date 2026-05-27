//! topo-app Rust surface: idiomatic registration, not a macro DSL.
//!
//! The proposal fixes the philosophy (pure In->Out Functor + flow DAG)
//! and leaves each topo-lang to project it onto its own idioms. Python
//! projects it onto a decorator reading `__annotations__`. Rust has no
//! decorators and no runtime reflection, so the Rust projection is an
//! explicit registration call whose In/Out are supplied through the
//! type system ([`crate::schema::TopoType`] / [`crate::record!`]) rather
//! than re-declared in hand-written `.topo` (the proposal's
//! "no duplicate declaration" rule: the In/Out live in the registration,
//! and the `.topo` file is generated from it).
//!
//! A handler stays a plain `fn` after registration — it is registered by
//! *describing* it, never wrapped — so it remains independently
//! invocable and unit-testable with zero framework bootstrap.

use crate::graph::{Edge, Flow, Graph, Handler};
use crate::schema::TypeRef;

/// A topo-app program: the in-memory logic graph plus the callables.
///
/// One `App` owns one namespace and one flow — enough to exercise every
/// proposal mapping rule without productionizing.
pub struct App {
    graph: Graph,
}

impl App {
    pub fn new(namespace: impl Into<String>) -> Self {
        App {
            graph: Graph::new(namespace),
        }
    }

    /// Register a logic unit by describing it. `in_type == None` is a
    /// legal source handler (no input). The actual callable is *not*
    /// taken or wrapped here: registration records the contract, the fn
    /// stays an ordinary, independently callable Rust function. This is
    /// the deliberate parity with Python's `@app.handler` returning the
    /// function unchanged.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        in_type: Option<TypeRef>,
        out_type: TypeRef,
    ) {
        self.graph.handlers.push(Handler {
            name: name.into(),
            in_type,
            out_type,
        });
    }

    /// Declare a linear logic chain: `flow("p", [a, b, c])` becomes
    /// edges a->b->c->void. A [`Stage::Parallel`] member fans in/out
    /// from the same neighbours (same-source / same-sink == same-stage
    /// parallel candidates, per the proposal mapping table) — identical
    /// edge construction to `app.py::flow`.
    pub fn flow(&mut self, name: impl Into<String>, stages: Vec<Stage>) {
        let mut edges: Vec<Edge> = Vec::new();

        for i in 0..stages.len().saturating_sub(1) {
            for src in stages[i].names() {
                for tgt in stages[i + 1].names() {
                    edges.push(Edge {
                        source: src.clone(),
                        target: Some(tgt.clone()),
                    });
                }
            }
        }
        if let Some(last) = stages.last() {
            for src in last.names() {
                // Terminal edge -> void.
                edges.push(Edge {
                    source: src,
                    target: None,
                });
            }
        }

        self.graph.flow = Some(Flow {
            name: name.into(),
            edges,
        });
    }

    /// The in-memory graph (the round-trip's left-hand side).
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The graph snapshot + emit entry point.
    ///
    /// Rust analogue of Python's `topo.config(app)`: one place to read
    /// the whole graph, emit the `.topo` view, and run the round-trip.
    /// (Named `app_config` to stay clear of the unrelated product
    /// runtime [`crate::config`] bridge, which shares the Python name
    /// only by coincidence — different concern entirely.)
    pub fn app_config(&self) -> AppConfig<'_> {
        AppConfig { app: self }
    }
}

/// A read-only graph snapshot view (proposal "config(app)"): the whole
/// graph in one place, the `.topo` emitter, and the round-trip.
pub struct AppConfig<'a> {
    app: &'a App,
}

impl AppConfig<'_> {
    /// The whole graph as a structured snapshot — handlers with their
    /// In/Out spellings and the flow edges, mirroring
    /// `config(app).snapshot()`.
    pub fn snapshot(&self) -> Snapshot {
        let g = self.app.graph();
        Snapshot {
            namespace: g.namespace.clone(),
            handlers: g
                .handlers
                .iter()
                .map(|h| HandlerSnapshot {
                    name: h.name.clone(),
                    in_type: h.in_type.as_ref().map(|t| t.topo()),
                    out_type: h.out_type.topo(),
                })
                .collect(),
            flow: g.flow.as_ref().map(|f| FlowSnapshot {
                name: f.name.clone(),
                edges: f
                    .edges
                    .iter()
                    .map(|e| {
                        (
                            e.source.clone(),
                            e.target.clone().unwrap_or_else(|| "void".to_string()),
                        )
                    })
                    .collect(),
            }),
        }
    }

    /// The round-trippable `.topo` view — identical bytes to
    /// [`crate::emit::emit_topo`] on the graph.
    pub fn emit_topo(&self) -> String {
        crate::emit::emit_topo(self.app.graph())
    }

    /// graph -> .topo -> graph': emit, then read back through the fresh
    /// `topo --ast-dump`. The returned graph is the round-trip's
    /// right-hand side for [`Graph::equivalent_to`].
    pub fn roundtrip(&self) -> Result<Graph, crate::readback::ReadbackError> {
        crate::readback::read_topo(&self.emit_topo())
    }
}

/// Structured whole-graph snapshot (the `config(app).snapshot()` shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub namespace: String,
    pub handlers: Vec<HandlerSnapshot>,
    pub flow: Option<FlowSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerSnapshot {
    pub name: String,
    pub in_type: Option<String>,
    pub out_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowSnapshot {
    pub name: String,
    /// `(source, target)` pairs; a terminal target is the literal
    /// `"void"` (the `.topo` spelling), matching the Python snapshot.
    pub edges: Vec<(String, String)>,
}

/// One position in a flow chain: either a single handler or a set of
/// independent handlers that are same-stage parallel candidates.
pub enum Stage {
    One(String),
    Parallel(Vec<String>),
}

impl Stage {
    fn names(&self) -> Vec<String> {
        match self {
            Stage::One(n) => vec![n.clone()],
            Stage::Parallel(ns) => ns.clone(),
        }
    }
}

impl From<&str> for Stage {
    fn from(s: &str) -> Self {
        Stage::One(s.to_string())
    }
}

impl From<String> for Stage {
    fn from(s: String) -> Self {
        Stage::One(s)
    }
}

/// Independent units on the same input == same-stage parallel
/// candidates (proposal mapping rule). Purity of these is enforced by
/// core `PurityCheck` after emission, never self-asserted here — exactly
/// `app.py::parallel`'s contract.
pub fn parallel<I, S>(members: I) -> Stage
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Stage::Parallel(members.into_iter().map(Into::into).collect())
}

/// Register a handler and keep it an ordinary callable.
///
/// `handler!(app, parse, in: String, out: i64)` records the contract on
/// `app` and leaves `parse` untouched and directly callable. A source
/// handler omits `in:`. In/Out spellings come from the type system:
/// `in`/`out` accept either a `T: TopoType` via `scalar::<T>()` sugar or
/// a `record!(...)` `TypeRef`. The fn name is written once (the macro
/// does not, and cannot, recover it by reflection) — the same single
/// statement Python's `@app.handler` is.
#[macro_export]
macro_rules! handler {
    // Source handler (no input), record/expr Out.
    ($app:expr, $name:ident, out: $out:expr) => {{
        $app.register(::std::stringify!($name), ::std::option::Option::None, $out);
    }};
    // In + Out, both arbitrary TypeRef expressions.
    ($app:expr, $name:ident, in: $in:expr, out: $out:expr) => {{
        $app.register(
            ::std::stringify!($name),
            ::std::option::Option::Some($in),
            $out,
        );
    }};
}
