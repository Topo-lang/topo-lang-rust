//! In-memory logic graph — the single source of truth a topo-app
//! program builds by registration.
//!
//! Mirrors `topo-lang-python/runtime/topo/_graph.py`. The graph is a
//! plain data model with no behaviour beyond structural equality.
//! Emission, read-back and checking are separate concerns that consume
//! this model, so the round-trip can be reasoned about as data, not as
//! side effects.

use crate::schema::TypeRef;

/// A registered logic unit. `in_type` is `None` for a source handler
/// (no input — the zero-input form is valid per the handler/flow spec).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handler {
    pub name: String,
    pub in_type: Option<TypeRef>,
    pub out_type: TypeRef,
}

impl Handler {
    /// The `handler` declaration line. The single input parameter is
    /// conventionally named `in_` (Rust reserves `in`); the Rust
    /// handler_flow fixture uses an arbitrary single param name, so the
    /// spelling of the name is free as long as it is a valid identifier.
    /// A source handler has no parameter.
    pub fn signature(&self) -> String {
        let param = match &self.in_type {
            None => String::new(),
            Some(t) => format!("{} in_", t.topo()),
        };
        format!(
            "handler {}({}) -> {};",
            self.name,
            param,
            self.out_type.topo()
        )
    }
}

/// A pipeline edge inside a flow. `target == None` is a terminal edge
/// (`source -> void;`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub source: String,
    pub target: Option<String>,
}

impl Edge {
    pub fn is_terminal(&self) -> bool {
        self.target.is_none()
    }
}

/// A flow: a named DAG of handler edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flow {
    pub name: String,
    pub edges: Vec<Edge>,
}

/// The whole program: namespace, handlers, one flow.
///
/// A single namespace + single flow keeps the slice minimal while still
/// exercising every mapping rule (linear chain, parallel fan-out,
/// terminal edge, source handler).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    pub namespace: String,
    pub handlers: Vec<Handler>,
    pub flow: Option<Flow>,
}

impl Graph {
    pub fn new(namespace: impl Into<String>) -> Self {
        Graph {
            namespace: namespace.into(),
            handlers: Vec::new(),
            flow: None,
        }
    }

    pub fn handler(&self, name: &str) -> Option<&Handler> {
        self.handlers.iter().find(|h| h.name == name)
    }

    /// A canonical, order-insensitive description of the graph's
    /// meaning. Two graphs are semantically equivalent iff their keys
    /// are equal. Handler order and edge order do not change meaning
    /// (the stage topology is derived from the edge set), so both are
    /// sorted — identical contract to `_graph.py::semantic_key`.
    pub fn semantic_key(&self) -> SemanticKey {
        let mut handlers: Vec<(String, Option<String>, String)> = self
            .handlers
            .iter()
            .map(|h| {
                (
                    h.name.clone(),
                    h.in_type.as_ref().map(|t| t.topo()),
                    h.out_type.topo(),
                )
            })
            .collect();
        handlers.sort();

        let flow_name = self.flow.as_ref().map(|f| f.name.clone());
        let mut edges: Vec<(String, Option<String>)> = self
            .flow
            .as_ref()
            .map(|f| {
                f.edges
                    .iter()
                    .map(|e| (e.source.clone(), e.target.clone()))
                    .collect()
            })
            .unwrap_or_default();
        edges.sort();

        SemanticKey {
            namespace: self.namespace.clone(),
            flow_name,
            handlers,
            edges,
        }
    }

    pub fn equivalent_to(&self, other: &Graph) -> bool {
        self.semantic_key() == other.semantic_key()
    }
}

/// Opaque canonical key — equality is the round-trip's headline
/// acceptance. Field order is fixed and all collections are pre-sorted
/// by [`Graph::semantic_key`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticKey {
    namespace: String,
    flow_name: Option<String>,
    handlers: Vec<(String, Option<String>, String)>,
    edges: Vec<(String, Option<String>)>,
}
