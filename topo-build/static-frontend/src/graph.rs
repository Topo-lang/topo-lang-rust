//! In-memory logic graph: the single source of truth a topo-app program
//! builds by registration.
//!
//! This is a structural port of the Python runtime bridge's `_graph.py`
//! data model. Keeping the same shape — and especially the same
//! order-insensitive `semantic_key` — is deliberate: it is what makes
//! the static path's graph directly comparable to the runtime bridge's
//! graph for the topo-app design's headline round-trip / graph==graph'
//! acceptance.

use std::fmt::Write as _;

/// A topo type as it will be spelled in `.topo`.
///
/// Exactly one variant is used at a use site; absence of a `TypeRef`
/// (a `None`) represents `void` / no-input — never a sentinel instance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypeRef {
    /// A stdlib scalar alias (`int` / `float` / `bool` / `str`).
    Scalar(String),
    /// An ordered `record<...>` field list (name, type).
    Record(Vec<(String, TypeRef)>),
}

impl TypeRef {
    /// The `.topo` spelling of this type.
    pub fn topo(&self) -> String {
        match self {
            TypeRef::Scalar(s) => s.clone(),
            TypeRef::Record(fields) => {
                let mut inner = String::new();
                for (i, (n, t)) in fields.iter().enumerate() {
                    if i > 0 {
                        inner.push_str(", ");
                    }
                    let _ = write!(inner, "{}: {}", n, t.topo());
                }
                format!("record<{inner}>")
            }
        }
    }
}

/// A registered logic unit. `in_type` is `None` for a source handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handler {
    pub name: String,
    pub in_type: Option<TypeRef>,
    pub out_type: TypeRef,
}

impl Handler {
    /// The `handler ...;` declaration line. The single input parameter is
    /// conventionally named `in` to match the handler/flow spec's
    /// HandlerInput form; a source handler has no parameter.
    pub fn signature(&self) -> String {
        let param = match &self.in_type {
            None => String::new(),
            Some(t) => format!("{} in", t.topo()),
        };
        format!("handler {}({}) -> {};", self.name, param, self.out_type.topo())
    }
}

/// A pipeline edge inside a flow. `target` is `None` for a terminal
/// edge (`source -> void;`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub source: String,
    pub target: Option<String>,
}

impl Edge {
    pub fn is_terminal(&self) -> bool {
        self.target.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flow {
    pub name: String,
    pub edges: Vec<Edge>,
}

/// The whole program: one namespace, handlers, one flow. A single
/// namespace + single flow keeps the vertical slice minimal while still
/// exercising every mapping rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    pub namespace: String,
    pub handlers: Vec<Handler>,
    pub flow: Option<Flow>,
}

/// Canonical, order-insensitive description of a graph's meaning. Two
/// graphs are semantically equivalent iff their keys are equal. Handler
/// order and edge order do not change meaning (stage topology is derived
/// from the edge set), so both are sorted — mirrors the Python
/// `Graph.semantic_key`.
pub type SemanticKey = (
    String,
    Option<String>,
    Vec<(String, Option<String>, String)>,
    Vec<(String, Option<String>)>,
);

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

        (self.namespace.clone(), flow_name, handlers, edges)
    }

    pub fn equivalent_to(&self, other: &Graph) -> bool {
        self.semantic_key() == other.semantic_key()
    }
}
