//! The Rust ecosystem bridge for the product runtime configuration.
//!
//! The layered model ([`crate::config_model`]) is language-agnostic and
//! never touches files. This is the Rust ecosystem's bridge: it decodes
//! `topo-app.toml` and serialises writes back, and exposes the same
//! semantics through idiomatic Rust accessors. It is a 1:1 port of the
//! Python `topo/config.py` bridge half (`ProductConfig`,
//! `declare_inlined_toml`/`restore_inlined_toml`, `declare_internal`,
//! `dev_browse`).
//!
//! ## TOML library choice
//!
//! The bridge uses the [`toml`] crate (v0.8). Rationale, recorded here
//! per the spec's "document the choice":
//!
//! * It is *the* idiomatic, ubiquitous TOML library in the Rust
//!   ecosystem — the same role `tomllib` plays in Python's stdlib. The
//!   Python bridge could lean on a stdlib parser; Rust has no stdlib
//!   TOML, and `toml` is the de-facto standard with a small, vendored
//!   dependency tree (`serde`, `toml_edit`, `toml_datetime`, `winnow`),
//!   all already present in the offline cargo cache. Hand-rolling a
//!   parser would be a worse engineering choice for no benefit.
//! * `toml::Value` exposes a first-class `Datetime` variant, so a TOML
//!   date/time decoded from the file is *detectable* and can be refused
//!   by the model's type contract — exact parity with the Python suite's
//!   datetime-rejection cases, rather than the parser silently coercing.
//!
//! The bridge still ships its *own* minimal deterministic emitter (sorted
//! keys, stable table nesting) rather than `toml::to_string`, mirroring
//! the Python bridge's decision: the round-trip contract is "re-parse of
//! the restored text == the original decoded data", and a deterministic
//! emitter over the flat→nested transform makes that an identity for the
//! scalar/array/table config vocabulary regardless of original key order.
//! The pure-semantics core stays free of `toml` entirely; only this
//! bridge file depends on it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config_model::{
    BrowseEntry, ConfigError, ConfigStore, ConfigValue, DevInternalRegistry, ItemPolicy,
    LayeredConfig, ResolvedValue,
};

// --- toml::Value <-> ConfigValue normalisation --------------------------
//
// The model only ever sees the plain [`ConfigValue`] vocabulary; this is
// the only place the ecosystem parser type appears. A TOML datetime is
// carried as ConfigValue::Datetime (not dropped) so the model's type
// contract can reject it with the stdlib-bridging-types gap message —
// the same reason the Python bridge lets tomllib's datetime reach the
// model.

fn toml_to_config(value: &toml::Value) -> ConfigValue {
    match value {
        toml::Value::Boolean(b) => ConfigValue::Bool(*b),
        toml::Value::Integer(i) => ConfigValue::Int(*i),
        toml::Value::Float(f) => ConfigValue::Float(*f),
        toml::Value::String(s) => ConfigValue::Str(s.clone()),
        toml::Value::Datetime(dt) => ConfigValue::Datetime(dt.to_string()),
        toml::Value::Array(items) => {
            ConfigValue::Array(items.iter().map(toml_to_config).collect())
        }
        toml::Value::Table(map) => {
            // A table reached as a *value* (a record) is one leaf in the
            // dotted-key addressing; the flatten step decides whether a
            // table is nesting or a stored record. Here every table is
            // carried structurally and `flatten_nested` recurses it.
            let mut record = BTreeMap::new();
            for (k, v) in map {
                record.insert(k.clone(), toml_to_config(v));
            }
            ConfigValue::Record(record)
        }
    }
}

/// Decoded TOML document -> the model's flat dotted-key map.
///
/// A table is treated as a nesting table and recursed; a scalar/array is
/// a leaf. Mirrors the Python `_flatten_nested`: the config vocabulary
/// keys are addressed by dotted path, and a stored table *value* only
/// appears as a value, never recursed into for *addressing* — but on the
/// decode path every nested table is structural, so it is flattened
/// (matching the Python bridge, where a record only arises from a
/// `set(...)` of a dict value, not from file decode).
fn flatten_nested(value: &toml::Value, prefix: &str) -> BTreeMap<String, ConfigValue> {
    let mut flat = BTreeMap::new();
    if let toml::Value::Table(map) = value {
        for (name, v) in map {
            let key = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            match v {
                toml::Value::Table(_) => {
                    flat.extend(flatten_nested(v, &key));
                }
                _ => {
                    flat.insert(key, toml_to_config(v));
                }
            }
        }
    }
    flat
}

// --- deterministic emitter ----------------------------------------------
//
// Mirrors the Python bridge's minimal writer: split dotted keys into a
// nested structure, then emit scalars-before-subtables with sorted keys
// and stable nesting. A stored table *value* (a record) is written inline
// so it round-trips as one value, not a sub-section.

#[derive(Debug)]
enum Nested {
    Leaf(ConfigValue),
    Table(BTreeMap<String, Nested>),
}

fn split_nested(flat: &BTreeMap<String, ConfigValue>) -> BTreeMap<String, Nested> {
    let mut root: BTreeMap<String, Nested> = BTreeMap::new();
    // Iterating a BTreeMap is already sorted, matching `sorted(flat)`.
    for (dotted, value) in flat {
        let parts: Vec<&str> = dotted.split('.').collect();
        let mut cursor = &mut root;
        for part in &parts[..parts.len() - 1] {
            let entry = cursor
                .entry((*part).to_string())
                .or_insert_with(|| Nested::Table(BTreeMap::new()));
            cursor = match entry {
                Nested::Table(t) => t,
                // A dotted path re-using a leaf segment is not part of
                // the flat config vocabulary; overwrite to a table so
                // emission stays total rather than panicking.
                leaf @ Nested::Leaf(_) => {
                    *leaf = Nested::Table(BTreeMap::new());
                    match leaf {
                        Nested::Table(t) => t,
                        _ => unreachable!(),
                    }
                }
            };
        }
        cursor.insert(
            parts[parts.len() - 1].to_string(),
            Nested::Leaf(value.clone()),
        );
    }
    root
}

fn toml_scalar(value: &ConfigValue) -> Result<String, ConfigError> {
    match value {
        ConfigValue::Bool(b) => Ok(if *b { "true".into() } else { "false".into() }),
        ConfigValue::Int(i) => Ok(i.to_string()),
        // `{:?}` on f64 yields a round-trippable literal (e.g. `0.5`,
        // `1.0`) the TOML parser reads back as the same float.
        ConfigValue::Float(f) => Ok(format!("{f:?}")),
        ConfigValue::Str(s) => {
            let escaped = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\t', "\\t");
            Ok(format!("\"{escaped}\""))
        }
        ConfigValue::Array(items) => {
            let parts: Result<Vec<String>, _> = items.iter().map(toml_scalar).collect();
            Ok(format!("[{}]", parts?.join(", ")))
        }
        ConfigValue::Record(fields) => {
            // An inline table value: round-trips as one value, sorted.
            let mut parts = Vec::new();
            for (k, v) in fields {
                parts.push(format!("{k} = {}", toml_scalar(v)?));
            }
            Ok(format!("{{{}}}", parts.join(", ")))
        }
        // The model rejects datetimes before a write reaches the bridge;
        // reaching this branch is a contract violation worth surfacing.
        ConfigValue::Datetime(_) => Err(ConfigError::UnbridgedValue(
            "datetime values have no stdlib bridge and must be rejected \
             by the model before serialisation"
                .to_string(),
        )),
    }
}

fn emit_toml(nested: &BTreeMap<String, Nested>, path: &[String]) -> Result<String, ConfigError> {
    let mut scalars: Vec<(&String, &ConfigValue)> = Vec::new();
    let mut subtables: Vec<(&String, &BTreeMap<String, Nested>)> = Vec::new();
    for (name, node) in nested {
        match node {
            Nested::Leaf(v) => scalars.push((name, v)),
            Nested::Table(t) => subtables.push((name, t)),
        }
    }
    // BTreeMap iteration is sorted; the explicit collects above preserve
    // that order, matching the Python `sorted(...)` passes.
    let mut out: Vec<String> = Vec::new();
    for (name, value) in &scalars {
        out.push(format!("{name} = {}", toml_scalar(value)?));
    }
    for (name, table) in &subtables {
        let mut section_path = path.to_vec();
        section_path.push((*name).clone());
        let section = section_path.join(".");
        let body = emit_toml(table, &section_path)?;
        out.push(format!("\n[{section}]"));
        if !body.is_empty() {
            out.push(body);
        }
    }
    Ok(out
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}

/// A flat map serialised to deterministic TOML text — the exact bytes
/// `set` writes and `restore_inlined_toml` returns. Empty map -> `""`.
fn serialize_flat(flat: &BTreeMap<String, ConfigValue>) -> Result<String, ConfigError> {
    if flat.is_empty() {
        return Ok(String::new());
    }
    let nested = split_nested(flat);
    let body = emit_toml(&nested, &[])?;
    Ok(format!("{}\n", body.trim()))
}

// --- ProductConfig ------------------------------------------------------

/// One declared pure-internal (`d`) record as the dev-phase listing
/// surfaces it — a plain shape, deliberately distinct from a runtime
/// [`BrowseEntry`] so the two ranges never blur.
#[derive(Debug, Clone, PartialEq)]
pub struct DevBrowseRecord {
    pub name: String,
    pub value: ConfigValue,
    pub tags: std::collections::BTreeSet<String>,
}

/// Rust projection of the product runtime config entry.
///
/// Wraps a language-agnostic [`ConfigStore`]; this type only adds the
/// Rust ecosystem's file I/O (`toml` read + the deterministic writer
/// above). `set` updates the external layer via the model and
/// re-serialises the user-managed file so a write is immediately
/// reflected on disk and in the next `get`.
pub struct ProductConfig {
    path: Option<PathBuf>,
    store: ConfigStore,
    // The pure-internal (d) catalogue is created lazily and kept on the
    // side: it is *not* wired into the ConfigStore, so the runtime
    // read/merge path provably cannot reach a d item. A production
    // projection could skip building it entirely. `Option` so "no d
    // declared" is structurally observable (the registry is never even
    // constructed by browsing), matching the Python `_dev_internal`.
    dev_internal: Option<DevInternalRegistry>,
}

/// Builder for [`ProductConfig`], mirroring the Python constructor's
/// keyword arguments (`path`, `inlined`, `injected`, `policies`).
#[derive(Default)]
pub struct ProductConfigBuilder {
    path: Option<PathBuf>,
    inlined: BTreeMap<String, ConfigValue>,
    injected: BTreeMap<String, ConfigValue>,
    policies: BTreeMap<String, ItemPolicy>,
}

impl ProductConfigBuilder {
    pub fn path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn inlined(mut self, inlined: BTreeMap<String, ConfigValue>) -> Self {
        self.inlined = inlined;
        self
    }

    pub fn injected(mut self, injected: BTreeMap<String, ConfigValue>) -> Self {
        self.injected = injected;
        self
    }

    pub fn policies(mut self, policies: BTreeMap<String, ItemPolicy>) -> Self {
        self.policies = policies;
        self
    }

    pub fn build(self) -> Result<ProductConfig, ConfigError> {
        // A file-backed config reads the external (a) layer from disk;
        // a missing file is an empty external layer, not an error
        // (mirrors the Python FileNotFoundError -> {} branch).
        let external = match &self.path {
            Some(p) => match fs::read_to_string(p) {
                Ok(text) => {
                    let doc: toml::Value = text.parse().map_err(|e| {
                        ConfigError::UnbridgedValue(format!(
                            "failed to parse {}: {e}",
                            p.display()
                        ))
                    })?;
                    flatten_nested(&doc, "")
                }
                Err(_) => BTreeMap::new(),
            },
            None => BTreeMap::new(),
        };
        let layered = LayeredConfig {
            inlined: self.inlined,
            external,
            injected: self.injected,
        };
        Ok(ProductConfig {
            path: self.path,
            store: ConfigStore::new(layered, self.policies),
            dev_internal: None,
        })
    }
}

impl ProductConfig {
    /// A builder mirroring the Python keyword constructor.
    pub fn builder() -> ProductConfigBuilder {
        ProductConfigBuilder::default()
    }

    /// An in-memory (pathless) config — the common test/embed entry.
    pub fn in_memory() -> Self {
        // Pathless build cannot fail (no file to parse).
        ProductConfigBuilder::default().build().unwrap()
    }

    /// Whether this config has a file path (a pathless config never
    /// persists).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn store(&self) -> &ConfigStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut ConfigStore {
        &mut self.store
    }

    pub fn declare(&mut self, key: &str, policy: ItemPolicy) -> Result<(), ConfigError> {
        self.store.declare(key, policy)
    }

    // -- code-layer inline / hidden TOML (layer b) ----------------------
    //
    // An explicit code-level call (not a TOML directive, not automatic
    // build behaviour) that says "this config block ships *inside* the
    // artifact" so it no longer needs to sit as a scattered external
    // file. The model only ever sees decoded data; this bridge owns the
    // toml decode and the symmetric restore back to TOML text.

    /// Embed a TOML config block (text) as the inlined (`b`) default.
    ///
    /// After this call the product needs no external file for these
    /// defaults, yet every embedded item still enumerates through
    /// `keys`/`query`/`query_resolved` exactly like any `b` value:
    /// embedding hides the *file*, never the *items*. `a`/`c` keep
    /// overriding `b` unchanged.
    pub fn declare_inlined_toml(&mut self, source: &str) -> Result<(), ConfigError> {
        let doc: toml::Value = source.parse().map_err(|e| {
            ConfigError::UnbridgedValue(format!("failed to parse inlined TOML: {e}"))
        })?;
        let flat = flatten_nested(&doc, "");
        self.store.layered_mut().install_inlined(flat)
    }

    /// Embed an already-decoded mapping as the inlined (`b`) default —
    /// the non-text counterpart of [`declare_inlined_toml`].
    ///
    /// [`declare_inlined_toml`]: ProductConfig::declare_inlined_toml
    pub fn declare_inlined_data(
        &mut self,
        data: BTreeMap<String, ConfigValue>,
    ) -> Result<(), ConfigError> {
        self.store.layered_mut().install_inlined(data)
    }

    /// Reconstruct the embedded (`b`) layer as equivalent TOML text.
    ///
    /// Embedding is not opacity: the inlined block is always recoverable
    /// to readable, hand-editable TOML. "Equivalent" means re-parsing the
    /// returned text yields the same decoded data the layer holds —
    /// guaranteed because it reuses the very same deterministic emitter
    /// the external file uses, over the same flat→nested transform, so
    /// encode∘decode is the identity for the scalar/array/table
    /// vocabulary.
    pub fn restore_inlined_toml(&self) -> Result<String, ConfigError> {
        serialize_flat(&self.store.layered().inlined)
    }

    // -- pure-internal (d) declaration ----------------------------------
    //
    // d is only declarable in code and has *no* runtime config presence:
    // the call returns a plain value the caller binds as an ordinary
    // constant, and the dev metadata lands solely in a side registry the
    // store never consults. Layer::D stays out of RUNTIME_MERGE_ORDER, so
    // keys()/resolve_all()/query() cannot surface it by construction.

    /// The dev-phase-only catalogue of `d` declarations, if any `d` was
    /// declared. `None` means a runtime-only projection that never built
    /// the registry — and crucially browsing the empty band must not
    /// construct it as a side effect.
    pub fn dev_internal(&self) -> Option<&DevInternalRegistry> {
        self.dev_internal.as_ref()
    }

    /// Declare a pure-internal datum and return the plain value.
    ///
    /// The return value is what the caller binds — equivalent to a
    /// hand-written constant, carrying no config-system reference. Its
    /// only visibility is dev-phase tag/name lookup via [`dev_internal`];
    /// it never enters the runtime store, so it is absent from
    /// `keys`/`query`/`resolve`.
    ///
    /// [`dev_internal`]: ProductConfig::dev_internal
    pub fn declare_internal<I, S>(
        &mut self,
        name: &str,
        value: ConfigValue,
        tags: I,
    ) -> Result<ConfigValue, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let reg = self.dev_internal.get_or_insert_with(DevInternalRegistry::new);
        reg.declare(name, value, tags)
    }

    pub fn keys(&self) -> Vec<String> {
        self.store.keys()
    }

    /// Tag- and read-tier-filtered key list. Pure passthrough to the
    /// language-agnostic store: the Rust bridge adds no filtering logic
    /// of its own, it only exposes the model's one query API.
    pub fn query(&self, tags: Option<&[&str]>, credential_level: i64) -> Vec<String> {
        self.store.query(tags, credential_level)
    }

    pub fn query_resolved(
        &self,
        tags: Option<&[&str]>,
        credential_level: i64,
    ) -> Result<BTreeMap<String, ResolvedValue>, ConfigError> {
        self.store.query_resolved(tags, credential_level)
    }

    pub fn max_read_level(&self) -> i64 {
        self.store.max_read_level()
    }

    pub fn read(&self, key: &str, credential_level: i64) -> Result<ConfigValue, ConfigError> {
        self.store.read(key, credential_level)
    }

    // -- unified browse + dev-phase d listing ---------------------------
    //
    // Pure passthrough to the language-agnostic store: the row schema and
    // the tier routing live in the model so any host bridge browses
    // identically. `dev_browse` is a structurally separate listing of the
    // pure-internal (d) band sourced from the side registry — d is not a
    // runtime config item, so it never appears in `browse` at any level
    // and is only ever surfaced here, tag-searchable, for development.

    /// Self-describing rows for every runtime item within the caller's
    /// read tier. Takes a credential *level* only — no
    /// principal/identity. At [`max_read_level`] this is the complete
    /// runtime key set; `d` items are never included.
    ///
    /// [`max_read_level`]: ProductConfig::max_read_level
    pub fn browse(
        &self,
        tags: Option<&[&str]>,
        credential_level: i64,
    ) -> Result<Vec<BrowseEntry>, ConfigError> {
        self.store.browse(tags, credential_level)
    }

    /// The dev-phase-only catalogue of pure-internal (`d`) data.
    ///
    /// Explicitly *not* part of the runtime browse: `d` is promoted to a
    /// plain host constant and has zero runtime footprint, so it is
    /// absent from [`browse`] at every level. When `tags` is given only
    /// `d` items whose tag set is a superset match (same
    /// freely-combinable tag-AND as the runtime query); otherwise every
    /// declared `d` item is listed. Returns records distinct in shape
    /// from a runtime [`BrowseEntry`] so the two ranges never blur. No
    /// `d` declared -> empty listing, and crucially the registry is *not*
    /// created as a side effect of browsing.
    ///
    /// [`browse`]: ProductConfig::browse
    pub fn dev_browse(&self, tags: Option<&[&str]>) -> Vec<DevBrowseRecord> {
        let reg = match &self.dev_internal {
            Some(r) => r,
            None => return Vec::new(),
        };
        let names = match tags {
            Some(t) => reg.search(t),
            None => reg.names(),
        };
        names
            .into_iter()
            .filter_map(|name| {
                reg.get(&name).map(|item| DevBrowseRecord {
                    name: item.name.clone(),
                    value: item.value.clone(),
                    tags: item.tags.clone(),
                })
            })
            .collect()
    }

    pub fn get(&self, key: &str) -> Result<ConfigValue, ConfigError> {
        self.store.get(key)
    }

    pub fn get_or(&self, key: &str, default: ConfigValue) -> ConfigValue {
        self.store.get_or(key, default)
    }

    pub fn resolve(&self, key: &str) -> Result<ResolvedValue, ConfigError> {
        self.store.resolve(key)
    }

    /// Validate + write through the model, then re-serialise the external
    /// layer to the user-managed file (when a path is set).
    pub fn set(
        &mut self,
        key: &str,
        value: ConfigValue,
        credential_level: i64,
    ) -> Result<(), ConfigError> {
        self.store.set(key, value, credential_level)?;
        if self.path.is_some() {
            self.write_external()?;
        }
        Ok(())
    }

    /// The external (`a`) layer as deterministic TOML text — the exact
    /// bytes [`set`] writes to `topo-app.toml`.
    ///
    /// [`set`]: ProductConfig::set
    pub fn serialize_external(&self) -> Result<String, ConfigError> {
        serialize_flat(self.store.pending_external())
    }

    fn write_external(&self) -> Result<(), ConfigError> {
        // Only reachable when a file-backed config is in use; a pathless
        // (in-memory) config never persists. Asserting the invariant
        // keeps the write honest rather than letting a None path reach
        // the filesystem through some future caller.
        let path = self.path.as_ref().ok_or_else(|| {
            ConfigError::WriteProtection(
                "cannot persist external layer: this config has no file path".to_string(),
            )
        })?;
        let text = self.serialize_external()?;
        fs::write(path, text).map_err(|e| {
            ConfigError::WriteProtection(format!(
                "failed to write {}: {e}",
                path.display()
            ))
        })
    }
}
