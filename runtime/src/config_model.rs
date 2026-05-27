//! Language-agnostic core of the product runtime configuration:
//! the layered value model, its merge precedence, per-value provenance,
//! the stdlib-typed value contract, the tag system, the two orthogonal
//! permission roles, the unified browse, and the pure-internal band.
//!
//! This module owns *semantics*, not *wiring*. It has no TOML parser,
//! no file I/O and no Rust-ecosystem-specific behaviour. A host bridge
//! decodes its ecosystem's TOML into the plain [`ConfigValue`] data this
//! model consumes and projects the merged result back. The model would
//! read identically if reimplemented in another host runtime — it is a
//! 1:1 port of `topo-lang-python/runtime/topo/_config_model.py`.
//!
//! Why the product config is a separate file from the build-time
//! `Topo.toml`: `Topo.toml` configures the *toolchain build* (host
//! language, sources, optimisation feature-modes, check policy — owned
//! by topo-build). This model configures the *built product's* runtime
//! behaviour. They answer different questions ("how is it compiled" vs.
//! "how does the running product behave") and share no sections. The
//! fixed product-config name is `topo-app.toml`; a build-toolchain key
//! offered here is a category error the boundary guard rejects, naming
//! `Topo.toml` so the user is told where the key actually belongs.
//!
//! The three runtime layers, least to most explicit:
//! * `b` — inlined / hidden TOML embedded in the artifact (built-in default)
//! * `a` — the external `topo-app.toml` the user manages (overrides `b`)
//! * `c` — a value injected directly in code (overrides everything)
//!
//! Frozen precedence: inlined default (b) ◁ external file (a) ◁ in-code
//! injection (c) — "more explicit wins", per key.
//!
//! A fourth band `d` (pure-internal) exists in the vocabulary but is
//! intentionally absent from this runtime merge: `d` is promoted to a
//! plain host constant with zero configuration-system footprint at
//! runtime. There is nothing to merge because at runtime it is no longer
//! a config value at all — which is why [`Layer::D`] is excluded from
//! [`RUNTIME_MERGE_ORDER`].

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Fixed product runtime config filename for this proof of concept. Kept
/// in the model (not a bridge) so every host agrees on the boundary name.
pub const PRODUCT_CONFIG_FILENAME: &str = "topo-app.toml";

/// The build toolchain owns `Topo.toml`; these are its section names. A
/// key whose first dotted segment is one of these belongs to the build
/// config, never to the product runtime config. One explicit list keeps
/// the non-overlap boundary in a single place rather than scattered
/// string checks.
pub const BUILD_TOOLCHAIN_SECTIONS: &[&str] = &[
    "topo",
    "build",
    "builder",
    "parallel",
    "adaptive",
    "optimize",
    "observability",
    "lifetime",
    "loop_parallel",
    "types",
    "completeness",
    "check",
    "test",
];

/// Which runtime layer a value originates from.
///
/// The discriminants encode the merge precedence (higher wins) so the
/// merge never hard-codes an ordering separate from the layer identity.
/// `D` is listed for vocabulary completeness but is never produced by
/// the runtime merge (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// pure-internal; promoted to code, never merged at runtime
    D = 0,
    /// inlined / hidden TOML default embedded in the artifact
    B = 1,
    /// external topo-app.toml the user manages
    A = 2,
    /// in-code explicit injection through the topo interface
    C = 3,
}

/// Layers that participate in the runtime merge, least to most explicit.
pub const RUNTIME_MERGE_ORDER: [Layer; 3] = [Layer::B, Layer::A, Layer::C];

/// A writer with no credential is level 0 — enough for LOW items, short
/// of anything that requires deliberate intent.
pub const NO_CREDENTIAL_LEVEL: i64 = 0;

/// How disruptive a wrong write to a config item is.
///
/// Modelled as an *ordered* scale (not a bool) from the start so a later
/// multi-tier permission slice can introduce intermediate levels and a
/// per-item required-credential-level without reshaping callers: today
/// only the LOW/HIGH endpoints are used and the gate compares the
/// presented credential level against the item's required level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImpactLevel {
    /// routine; a wrong value is easily noticed and reverted
    Low = 0,
    /// outsized blast radius; a careless write must be deliberate
    High = 1,
}

impl ImpactLevel {
    /// The uppercase spelling used in user-facing messages, matching the
    /// Python `ImpactLevel.name` so a guard message reads identically.
    pub fn name(&self) -> &'static str {
        match self {
            ImpactLevel::Low => "LOW",
            ImpactLevel::High => "HIGH",
        }
    }
}

/// The single decoded plain-data value vocabulary the model operates on.
///
/// A host bridge normalises its ecosystem's TOML into this shape; the
/// model never sees a parser type. The variants are exactly the stdlib
/// bridge contract (`bool`/`int`/`float`/`str`/`slice`/`record`) plus a
/// [`ConfigValue::Datetime`] variant that the type contract *rejects* —
/// it exists so a TOML date/time decoded by a bridge can be detected and
/// refused with a message naming the stdlib-bridging-types gap, rather
/// than the bridge silently dropping it before the model can object.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<ConfigValue>),
    /// A table value (a `record`) — addressed as one leaf, not recursed
    /// into for dotted-key addressing. `BTreeMap` keeps record key order
    /// deterministic for stable serialisation.
    Record(BTreeMap<String, ConfigValue>),
    /// TOML date/time. Has no stdlib bridge type; the contract rejects
    /// it. Carried so the rejection can name the offending value rather
    /// than the bridge dropping it silently.
    Datetime(String),
}

/// The error variants the model raises. Each carries a message that is
/// asserted on in the parity tests (the Python suite checks substrings
/// like `Topo.toml`, the stdlib-bridging-types gap pointer, `time_*`,
/// `HIGH`).
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    /// A key that belongs to the build toolchain was offered to the
    /// product runtime config. Message names `Topo.toml`.
    BuildConfigKey(String),
    /// A config value whose type has no stdlib bridge was offered. Message
    /// names the key and the stdlib-bridging-types gap.
    UnbridgedValue(String),
    /// A write was refused because the presented credential level is
    /// below the item's required level (mis-operation guard, not
    /// secrecy). Also used for a read below the item's read tier.
    WriteProtection(String),
    /// A requested key is set by no runtime layer (the `get`/`resolve`
    /// "no silent None" contract).
    KeyNotFound(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::BuildConfigKey(m)
            | ConfigError::UnbridgedValue(m)
            | ConfigError::WriteProtection(m) => write!(f, "{m}"),
            ConfigError::KeyNotFound(k) => write!(f, "key not found: {k}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// First dotted segment of a config key (`a.b.c` -> `a`).
fn root_section(key: &str) -> &str {
    key.split_once('.').map(|(head, _)| head).unwrap_or(key)
}

/// Boundary guard: refuse a key that belongs in `Topo.toml`.
///
/// The product runtime config and the build-time `Topo.toml` share no
/// sections by design; accepting a build key here would create a second,
/// silently-ignored home for it. Rejecting loudly — and naming the file
/// the key actually belongs to — keeps the boundary honest.
pub fn reject_if_build_config_key(key: &str) -> Result<(), ConfigError> {
    let section = root_section(key);
    if BUILD_TOOLCHAIN_SECTIONS.contains(&section) {
        return Err(ConfigError::BuildConfigKey(format!(
            "'{key}' configures the build toolchain (section \
             '[{section}]') and belongs in Topo.toml, not the product \
             runtime config ({PRODUCT_CONFIG_FILENAME}). The two files \
             share no sections; set this in Topo.toml instead."
        )));
    }
    Ok(())
}

/// An effective value plus the layer it came from.
///
/// Provenance travels with every value so any consumer (a human, an
/// agent, a later read/write slice) can answer "which layer set this?"
/// without re-running the merge.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedValue {
    pub value: ConfigValue,
    pub layer: Layer,
}

/// One self-describing row of the unified browse.
///
/// Carries everything a human or an agent needs to judge a config item
/// without a second query: identity and contract type, the built-in
/// default and the current effective value with the layer that produced
/// it, the write blast-radius (`impact`) and *both* permission
/// thresholds — `required_write_level` (the mis-operation gate) and
/// `required_read_level` (the read-visibility tier) — kept as separate
/// fields because the two roles are orthogonal, plus the
/// freely-combinable retrieval `tags`. `default` is `None` when the item
/// has no inlined (`b`) default — the explicit "no default exists"
/// marker, distinct from a stored null-like value.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowseEntry {
    pub key: String,
    pub r#type: String,
    pub default: Option<ConfigValue>,
    pub effective: ConfigValue,
    pub layer: Layer,
    pub impact: ImpactLevel,
    pub required_write_level: i64,
    pub required_read_level: i64,
    pub tags: BTreeSet<String>,
}

/// The a/b/c layers as plain decoded data + the merge over them.
///
/// Each layer is a flat mapping of dotted-key -> already-decoded plain
/// value. TOML parsing is a separate concern: a bridge fills these maps;
/// this model only merges and attributes them. `BTreeMap` gives the
/// sorted, hand-checkable key enumeration the Python `sorted(...)` calls
/// produce.
#[derive(Debug, Clone, Default)]
pub struct LayeredConfig {
    pub inlined: BTreeMap<String, ConfigValue>,
    pub external: BTreeMap<String, ConfigValue>,
    pub injected: BTreeMap<String, ConfigValue>,
}

impl LayeredConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a block of already-decoded data as the inlined (`b`)
    /// layer — the artifact-embedded default.
    ///
    /// The model side of an explicit code-layer declaration that a config
    /// block travels *inside the artifact* instead of as a scattered
    /// external file. Decode-only: the bridge owns turning TOML text into
    /// this plain map and the symmetric restore. Embedding changes where
    /// the *file* lives, never whether the *items* are browsable — the
    /// installed keys merge as the ordinary `b` default. Build-toolchain
    /// keys are rejected here too so a misplaced key cannot sneak in
    /// through the embedded layer any more than through the external one.
    pub fn install_inlined(
        &mut self,
        data: BTreeMap<String, ConfigValue>,
    ) -> Result<(), ConfigError> {
        for key in data.keys() {
            reject_if_build_config_key(key)?;
        }
        self.inlined = data;
        Ok(())
    }

    /// The map for a runtime merge layer. `Layer::D` never participates
    /// by construction — asking for it is an explicit error, mirroring
    /// the Python `AssertionError`, not a silent empty result.
    fn layer_map(&self, layer: Layer) -> Result<&BTreeMap<String, ConfigValue>, ConfigError> {
        match layer {
            Layer::B => Ok(&self.inlined),
            Layer::A => Ok(&self.external),
            Layer::C => Ok(&self.injected),
            Layer::D => Err(ConfigError::WriteProtection(format!(
                "{layer:?} is not a runtime merge layer"
            ))),
        }
    }

    /// Public probe of the runtime-merge-layer guard, mirroring the
    /// Python `_layer_map` whose `Layer.D` path raises. `Layer::D` is not
    /// a runtime merge layer by construction; asking for it is an
    /// explicit error, not a silent empty result. Exposed so the parity
    /// suite can pin "a `d` band can never be merged at runtime".
    pub fn layer_map_for(&self, layer: Layer) -> Result<&BTreeMap<String, ConfigValue>, ConfigError> {
        self.layer_map(layer)
    }

    fn validate_keys(&self) -> Result<(), ConfigError> {
        for layer in RUNTIME_MERGE_ORDER {
            for key in self.layer_map(layer)?.keys() {
                reject_if_build_config_key(key)?;
            }
        }
        Ok(())
    }

    /// Every key contributed by any runtime layer, sorted for a stable,
    /// hand-checkable enumeration.
    pub fn keys(&self) -> Vec<String> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for layer in RUNTIME_MERGE_ORDER {
            if let Ok(map) = self.layer_map(layer) {
                seen.extend(map.keys().cloned());
            }
        }
        seen.into_iter().collect()
    }

    /// Effective value + provenance for one key.
    ///
    /// Walks the layers least-to-most explicit; the last layer that
    /// carries the key wins, and that layer is the recorded provenance.
    pub fn resolve(&self, key: &str) -> Result<ResolvedValue, ConfigError> {
        reject_if_build_config_key(key)?;
        let mut winner: Option<ResolvedValue> = None;
        for layer in RUNTIME_MERGE_ORDER {
            let map = self.layer_map(layer)?;
            if let Some(value) = map.get(key) {
                winner = Some(ResolvedValue {
                    value: value.clone(),
                    layer,
                });
            }
        }
        winner.ok_or_else(|| ConfigError::KeyNotFound(key.to_string()))
    }

    /// Every key -> (effective value, provenance layer). Build-toolchain
    /// keys are rejected up front so a misplaced key fails loudly rather
    /// than appearing as a phantom entry.
    pub fn resolve_all(&self) -> Result<BTreeMap<String, ResolvedValue>, ConfigError> {
        self.validate_keys()?;
        let mut out = BTreeMap::new();
        for key in self.keys() {
            let rv = self.resolve(&key)?;
            out.insert(key, rv);
        }
        Ok(out)
    }
}

/// Convenience: build a [`LayeredConfig`] from the three layer maps and
/// return the resolved key -> value+provenance mapping.
pub fn merge_layers(
    inlined: BTreeMap<String, ConfigValue>,
    external: BTreeMap<String, ConfigValue>,
    injected: BTreeMap<String, ConfigValue>,
) -> Result<BTreeMap<String, ResolvedValue>, ConfigError> {
    LayeredConfig {
        inlined,
        external,
        injected,
    }
    .resolve_all()
}

/// Flatten a resolved mapping to `(key, value, layer)` triples in stable
/// key order — the shape later browse/introspection slices read.
pub fn iter_provenance(
    resolved: &BTreeMap<String, ResolvedValue>,
) -> Vec<(String, ConfigValue, Layer)> {
    resolved
        .iter()
        .map(|(k, rv)| (k.clone(), rv.value.clone(), rv.layer))
        .collect()
}

// --- Value-type contract ------------------------------------------------
//
// A config value only enters the model if it has a stdlib bridge type, so
// every value the running product reads has a known contract — the same
// schema vocabulary the handler In/Out boundary uses. Aggregates are
// validated element-wise so a datetime smuggled inside an array or table
// is caught, not just a top-level one. TOML date/time has no stdlib
// correspondence (the `time_*`/uuid/decimal128 family is the deferred
// stdlib-bridging-types gap), so it is rejected rather than given an
// ad-hoc contract.

/// The stdlib bridge spelling for a decoded value, or an error.
///
/// The raised error carries no key on its own; callers that know the key
/// re-raise with it attached (mirrors the Python two-stage message).
pub fn stdlib_type_of(value: &ConfigValue) -> Result<&'static str, ConfigError> {
    match value {
        ConfigValue::Datetime(_) => Err(ConfigError::UnbridgedValue(
            "value of type 'datetime' has no stdlib bridge type — TOML \
             date/time maps to the not-yet-implemented time_* family \
             (stdlib-bridging-types gap: the time_*/uuid/decimal128 \
             family is not yet wired). Accepting it would store a value \
             with no schema contract; use a bridged scalar instead."
                .to_string(),
        )),
        ConfigValue::Bool(_) => Ok("bool"),
        ConfigValue::Int(_) => Ok("int"),
        ConfigValue::Float(_) => Ok("float"),
        ConfigValue::Str(_) => Ok("str"),
        ConfigValue::Array(elements) => {
            for element in elements {
                stdlib_type_of(element)?;
            }
            Ok("slice")
        }
        ConfigValue::Record(fields) => {
            for element in fields.values() {
                stdlib_type_of(element)?;
            }
            Ok("record")
        }
    }
}

/// Type-gate a value about to be written under `key`.
///
/// Re-raises the underlying unbridged-value error with the offending key
/// prepended so a rejection always locates the problem.
pub fn validate_value(key: &str, value: &ConfigValue) -> Result<(), ConfigError> {
    match stdlib_type_of(value) {
        Ok(_) => Ok(()),
        Err(ConfigError::UnbridgedValue(msg)) => Err(ConfigError::UnbridgedValue(format!(
            "config key '{key}': {msg}"
        ))),
        Err(other) => Err(other),
    }
}

// --- Write protection: impact level + credential gate -------------------
//
// This gate exists to stop *mistaken* writes to items where a wrong value
// has outsized blast radius — a guard rail, not a secrecy boundary. It is
// identity-independent by construction: the check takes a credential
// *level*, never a principal. A human and an agent presenting the same
// level are treated identically; there is no "who" argument anywhere.

/// Credential level a writer must present to pass the gate for an item of
/// a given impact. An explicit ordered mapping (not `impact == High`) so
/// inserting a mid level later is a one-line edit, not a logic rewrite —
/// the multi-level scale the spec requires. Returned as `Option` so a
/// caller can distinguish "table has no entry" the way the Python suite
/// mutates `_REQUIRED_CREDENTIAL_LEVEL` to insert a mid threshold.
#[derive(Debug, Clone)]
pub struct RequiredCredentialTable {
    entries: BTreeMap<ImpactLevel, i64>,
}

impl Default for RequiredCredentialTable {
    fn default() -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(ImpactLevel::Low, 0);
        entries.insert(ImpactLevel::High, 1);
        Self { entries }
    }
}

impl RequiredCredentialTable {
    /// The minimum credential level for an impact, or 0 if (somehow) the
    /// table has no entry — a defensive default that never under-gates.
    pub fn level_for(&self, impact: ImpactLevel) -> i64 {
        self.entries.get(&impact).copied().unwrap_or(0)
    }

    /// Re-point an impact's required level. This is the "table edit, not
    /// a logic rewrite" the spec demands so a mid threshold can be
    /// introduced without reshaping any caller.
    pub fn set(&mut self, impact: ImpactLevel, level: i64) {
        self.entries.insert(impact, level);
    }
}

/// Per-item declaration carrying *orthogonal* dimensions.
///
/// * `tags` — a freely-combinable set scoping *retrieval*. A pure label
///   set: tags never affect read or write permission, only which filter
///   a query matches. Stored sorted so tag identity is order-independent.
/// * `read_level` — the minimum permission level a caller must present to
///   have this item *enumerated or read*. `0` (default) = visible to
///   everyone. Above `0` makes the item permission-gated.
/// * `impact` — independent of the two above: it drives the *write*
///   mis-operation gate, not visibility.
///
/// The two permission roles ride the same integer scale but are
/// deliberately separate fields: an item can be freely readable yet
/// write-guarded, or read-gated yet low-impact to write. Tags are a
/// third, permission-independent axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPolicy {
    pub impact: ImpactLevel,
    pub tags: BTreeSet<String>,
    pub read_level: i64,
}

impl Default for ItemPolicy {
    fn default() -> Self {
        // Unguarded default: LOW impact, no tags, open read tier — a
        // plain non-permission item.
        Self {
            impact: ImpactLevel::Low,
            tags: BTreeSet::new(),
            read_level: 0,
        }
    }
}

impl ItemPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set the write-gate impact level.
    pub fn with_impact(mut self, impact: ImpactLevel) -> Self {
        self.impact = impact;
        self
    }

    /// Builder: accept any iterable of tag strings; stored order-free.
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Builder: set the read-visibility tier.
    pub fn with_read_level(mut self, level: i64) -> Self {
        self.read_level = level;
        self
    }
}

/// The minimum permission level a caller must present to have an item
/// enumerated/read. `0` means unrestricted. The read-visibility tiering
/// role — the orthogonal twin of the write gate. Both consult the same
/// integer scale; they answer different questions (may I *see* it vs. may
/// I *change* it) and never collapse into one another.
pub fn required_read_level(policy: &ItemPolicy) -> i64 {
    policy.read_level
}

/// Pass iff `credential_level` meets the item's required write level.
///
/// There is no principal/identity parameter. The gate cannot and does not
/// distinguish a human from an agent — it only compares levels, which is
/// exactly the "mistake-prevention, not secrecy" intent.
pub fn authorize_write(
    key: &str,
    policy: &ItemPolicy,
    credential_level: i64,
    table: &RequiredCredentialTable,
) -> Result<(), ConfigError> {
    let needed = table.level_for(policy.impact);
    if credential_level < needed {
        return Err(ConfigError::WriteProtection(format!(
            "config key '{key}' is impact={}; writing it requires \
             credential level >= {needed}, but the write presented level \
             {credential_level}. This guard prevents accidental \
             high-impact changes; re-issue the write with a sufficient \
             credential level if the change is intended.",
            policy.impact.name()
        )));
    }
    Ok(())
}

// --- Read/write API over the layered model ------------------------------

/// Read/write façade over [`LayeredConfig`].
///
/// Reads honour the frozen `b ◁ a ◁ c` precedence. Writes land in the
/// *external* layer (`a`) — the user-managed file's in-memory image —
/// because that is the layer a user/agent is allowed to author; the
/// inlined default (`b`) and in-code injection (`c`) are owned by other
/// mechanisms. Language-agnostic: it mutates the decoded `external` map
/// and reports the new value; turning that map into `topo-app.toml` bytes
/// is a host-bridge concern. A bridge calls [`ConfigStore::pending_external`]
/// to obtain the map to serialise after a write.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    cfg: LayeredConfig,
    policies: BTreeMap<String, ItemPolicy>,
    credential_table: RequiredCredentialTable,
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new(LayeredConfig::new(), BTreeMap::new())
    }
}

impl ConfigStore {
    pub fn new(
        layered: LayeredConfig,
        policies: BTreeMap<String, ItemPolicy>,
    ) -> Self {
        Self {
            cfg: layered,
            policies,
            credential_table: RequiredCredentialTable::default(),
        }
    }

    /// Direct access to the underlying layered model. The bridge needs
    /// this to install the inlined layer and to read the `b` map for the
    /// browse `default` column without widening visibility.
    pub fn layered(&self) -> &LayeredConfig {
        &self.cfg
    }

    pub fn layered_mut(&mut self) -> &mut LayeredConfig {
        &mut self.cfg
    }

    /// Mutable access to the required-credential table so a caller can
    /// insert a mid threshold (a table edit, not a logic rewrite) — the
    /// generalized multi-level write gate the spec requires.
    pub fn credential_table_mut(&mut self) -> &mut RequiredCredentialTable {
        &mut self.credential_table
    }

    // -- declaration -----------------------------------------------------

    /// Attach a write-protection / tag / read-tier policy to `key`.
    pub fn declare(&mut self, key: &str, policy: ItemPolicy) -> Result<(), ConfigError> {
        reject_if_build_config_key(key)?;
        self.policies.insert(key.to_string(), policy);
        Ok(())
    }

    /// The item's declared policy, or the unguarded LOW-impact default.
    pub fn policy_of(&self, key: &str) -> ItemPolicy {
        self.policies.get(key).cloned().unwrap_or_default()
    }

    // -- tag + read-visibility query ------------------------------------
    //
    // One query API, two orthogonal filter dimensions, *zero* ambient
    // state. It takes the filter (tags, level) as arguments and reads no
    // identity — so the same method called from two sites with different
    // arguments yields different visibility purely from what each site
    // passes in. There is intentionally no principal/user/agent argument.

    /// The highest read-level any runtime item requires.
    ///
    /// A caller presenting this level (or above) can enumerate *every*
    /// runtime item — no level at which some runtime fragment stays
    /// invisible. This makes the tiered-transparency invariant checkable:
    /// the top of the scale always sees the whole runtime range. `0` when
    /// nothing is permission-gated.
    pub fn max_read_level(&self) -> i64 {
        self.cfg
            .keys()
            .iter()
            .map(|k| self.policy_of(k).read_level)
            .max()
            .unwrap_or(0)
    }

    fn visible(&self, key: &str, credential_level: i64) -> bool {
        credential_level >= self.policy_of(key).read_level
    }

    /// Keys matching a tag filter *and* within the caller's read tier.
    ///
    /// Two independent axes: `tags` empty/None => every item matches the
    /// tag axis; otherwise an item matches only if its tag set is a
    /// *superset* of the requested set (tag AND, freely combinable). Tags
    /// never grant or deny permission. `credential_level` lists an item
    /// only when this level meets its `read_level`; with no credential
    /// every permission-gated item is hidden. At [`max_read_level`] the
    /// read axis admits all keys.
    ///
    /// [`max_read_level`]: ConfigStore::max_read_level
    pub fn query(&self, tags: Option<&[&str]>, credential_level: i64) -> Vec<String> {
        let wanted: BTreeSet<String> = tags
            .map(|t| t.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        for key in self.cfg.keys() {
            if !self.visible(&key, credential_level) {
                continue;
            }
            if !wanted.is_empty() && !wanted.is_subset(&self.policy_of(&key).tags) {
                continue;
            }
            out.push(key);
        }
        out.sort();
        out
    }

    /// [`query`] but returning effective value + provenance for each
    /// matched key — the read counterpart of the filter.
    ///
    /// [`query`]: ConfigStore::query
    pub fn query_resolved(
        &self,
        tags: Option<&[&str]>,
        credential_level: i64,
    ) -> Result<BTreeMap<String, ResolvedValue>, ConfigError> {
        let mut out = BTreeMap::new();
        for key in self.query(tags, credential_level) {
            let rv = self.cfg.resolve(&key)?;
            out.insert(key, rv);
        }
        Ok(out)
    }

    // -- read ------------------------------------------------------------

    /// Every key any runtime layer contributes, sorted.
    pub fn keys(&self) -> Vec<String> {
        self.cfg.keys()
    }

    /// Effective value honouring `b ◁ a ◁ c`, or [`ConfigError::KeyNotFound`]
    /// when no layer sets it (no silent default). Tier-blind internal.
    pub fn get(&self, key: &str) -> Result<ConfigValue, ConfigError> {
        Ok(self.cfg.resolve(key)?.value)
    }

    /// Effective value, or `default` if no layer sets the key. The
    /// explicit-default counterpart of [`get`].
    ///
    /// [`get`]: ConfigStore::get
    pub fn get_or(&self, key: &str, default: ConfigValue) -> ConfigValue {
        match self.cfg.resolve(key) {
            Ok(rv) => rv.value,
            Err(_) => default,
        }
    }

    /// Effective value + which layer it came from. Tier-blind.
    pub fn resolve(&self, key: &str) -> Result<ResolvedValue, ConfigError> {
        self.cfg.resolve(key)
    }

    /// Every key -> (effective value, provenance layer). Tier-blind.
    pub fn resolve_all(&self) -> Result<BTreeMap<String, ResolvedValue>, ConfigError> {
        self.cfg.resolve_all()
    }

    /// Read honouring the read-visibility tier.
    ///
    /// Below the item's `read_level` the item is treated as not listable,
    /// so a read is refused the same way enumeration hides it. [`get`] is
    /// the raw, tier-blind accessor; `read` is the tier-aware door.
    ///
    /// [`get`]: ConfigStore::get
    pub fn read(&self, key: &str, credential_level: i64) -> Result<ConfigValue, ConfigError> {
        if !self.visible(key, credential_level) {
            let needed = self.policy_of(key).read_level;
            return Err(ConfigError::WriteProtection(format!(
                "config key '{key}' requires read level >= {needed} to be \
                 listed or read; the request presented level \
                 {credential_level}. Permission-gated items are hidden \
                 below their tier; re-issue with a sufficient level."
            )));
        }
        Ok(self.cfg.resolve(key)?.value)
    }

    // -- write -----------------------------------------------------------

    /// Write `value` for `key` into the external layer (`a`).
    ///
    /// Order of checks: a build-toolchain key is a category error
    /// (rejected first); then the value must have a stdlib contract; then
    /// the write-protection gate. Only after all three pass is the
    /// external map mutated, so a rejected write never leaves a partial
    /// state.
    pub fn set(
        &mut self,
        key: &str,
        value: ConfigValue,
        credential_level: i64,
    ) -> Result<(), ConfigError> {
        reject_if_build_config_key(key)?;
        validate_value(key, &value)?;
        let policy = self.policy_of(key);
        authorize_write(key, &policy, credential_level, &self.credential_table)?;
        self.cfg.external.insert(key.to_string(), value);
        Ok(())
    }

    // -- unified browse + introspection ---------------------------------
    //
    // A single call that yields, *within the caller's read tier*, a
    // self-describing row per config item — enough for a human or an
    // agent to judge "what does changing this affect / is it high-impact
    // / what level do I need to see and to write it" without a second
    // round trip. Built strictly on the tier-aware door (query_resolved
    // -> query -> policy_of); it never calls the tier-blind resolve_all,
    // so a permission-gated item cannot leak into a lower-level caller's
    // view. The row set is derived live on every call, so a key declared
    // after construction appears with no list to maintain.

    /// Self-describing rows for every item in the caller's read tier.
    ///
    /// Routes through [`query_resolved`] (the tier-aware door): at
    /// [`max_read_level`] every runtime item is present (the
    /// tiered-transparency invariant); below an item's `read_level` that
    /// item is wholly absent, value included. Takes a credential *level*
    /// only — no principal/identity. `d` is not a runtime item and never
    /// appears here.
    ///
    /// [`query_resolved`]: ConfigStore::query_resolved
    /// [`max_read_level`]: ConfigStore::max_read_level
    pub fn browse(
        &self,
        tags: Option<&[&str]>,
        credential_level: i64,
    ) -> Result<Vec<BrowseEntry>, ConfigError> {
        let resolved = self.query_resolved(tags, credential_level)?;
        let mut rows = Vec::new();
        for (key, rv) in resolved.iter() {
            let policy = self.policy_of(key);
            // Default = the inlined (b) built-in when the key has one;
            // absent otherwise. Read from the b map directly (not via the
            // tier-blind resolve), so this stays a pure lookup that
            // cannot widen visibility.
            let default_value = self.cfg.inlined.get(key).cloned();
            // Type from the contract that already governs every stored
            // value; prefer the effective value, fall back to the default
            // so a row still types when both exist (matching the Python
            // "prefer effective, else default" with the same datetime
            // fallback to the effective value's own error path).
            let type_source: &ConfigValue = default_value
                .as_ref()
                .filter(|_| matches!(rv.value, ConfigValue::Datetime(_)))
                .unwrap_or(&rv.value);
            let value_type = match stdlib_type_of(type_source) {
                Ok(t) => t,
                Err(_) => stdlib_type_of(&rv.value)?,
            };
            rows.push(BrowseEntry {
                key: key.clone(),
                r#type: value_type.to_string(),
                default: default_value,
                effective: rv.value.clone(),
                layer: rv.layer,
                impact: policy.impact,
                required_write_level: self.credential_table.level_for(policy.impact),
                required_read_level: required_read_level(&policy),
                tags: policy.tags.clone(),
            });
        }
        Ok(rows)
    }

    // -- bridge hook -----------------------------------------------------

    /// The external-layer map a host bridge serialises to the
    /// user-managed config file after a write. The model never touches
    /// files itself.
    pub fn pending_external(&self) -> &BTreeMap<String, ConfigValue> {
        &self.cfg.external
    }
}

// --- Pure-internal (d) band: dev-phase registry, no runtime presence ----
//
// `d` is the innermost band. Unlike a/b/c it is *not* a runtime config
// value: after toolchain processing it is promoted to a plain host
// constant with zero configuration-system footprint, which is why
// `Layer::D` is excluded from RUNTIME_MERGE_ORDER and the runtime merge
// never sees it. Its tags exist for one purpose only — being discoverable
// *while developing* — so it gets its own registry structurally disjoint
// from ConfigStore/LayeredConfig. Nothing on the runtime read/merge path
// holds a reference to this type; a runtime build can drop this registry
// entirely without changing any resolved value.

/// A pure-internal datum as seen *only during development*.
///
/// Carries the declared name, its constant value, and dev-phase
/// retrieval tags. It has no `read_level`/`impact`: those gate runtime
/// visibility and write blast-radius, and `d` has neither a runtime
/// presence nor a runtime write path.
#[derive(Debug, Clone, PartialEq)]
pub struct DevInternalItem {
    pub name: String,
    pub value: ConfigValue,
    pub tags: BTreeSet<String>,
}

/// A development-phase-only catalogue of `d` declarations.
///
/// The *only* place a `d` item is visible, deliberately a free-standing
/// object the runtime config path never consults: `ConfigStore` does not
/// hold one, `LayeredConfig` does not reference one, and
/// `resolve`/`query`/`keys` cannot reach it. Its sole job is to let a
/// developer find a pure-internal datum by name or tag while building. A
/// production build may simply never construct this registry — the
/// promoted constants stand on their own.
#[derive(Debug, Clone, Default)]
pub struct DevInternalRegistry {
    items: BTreeMap<String, DevInternalItem>,
}

impl DevInternalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a pure-internal datum for dev-phase discovery and return
    /// the plain value to be bound as a host constant.
    ///
    /// The value still must satisfy the stdlib contract, but it is *not*
    /// stored as a config item anywhere: the returned value is what the
    /// caller binds, byte-equivalent to a hand-written constant. The
    /// build-toolchain boundary guard applies to the name as well, so
    /// `d` cannot be used to smuggle a build key either.
    pub fn declare<I, S>(
        &mut self,
        name: &str,
        value: ConfigValue,
        tags: I,
    ) -> Result<ConfigValue, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        reject_if_build_config_key(name)?;
        validate_value(name, &value)?;
        self.items.insert(
            name.to_string(),
            DevInternalItem {
                name: name.to_string(),
                value: value.clone(),
                tags: tags.into_iter().map(Into::into).collect(),
            },
        );
        Ok(value)
    }

    /// Every declared `d` name, sorted — dev-phase enumeration.
    pub fn names(&self) -> Vec<String> {
        self.items.keys().cloned().collect()
    }

    /// The dev-phase record for `name`, if declared.
    pub fn get(&self, name: &str) -> Option<&DevInternalItem> {
        self.items.get(name)
    }

    /// `d` names whose tag set is a superset of `tags` (tag AND, same
    /// freely-combinable semantics as the runtime tag query) — the *only*
    /// retrieval `d`'s tags ever serve.
    pub fn search(&self, tags: &[&str]) -> Vec<String> {
        let wanted: BTreeSet<String> = tags.iter().map(|s| s.to_string()).collect();
        let mut out: Vec<String> = self
            .items
            .iter()
            .filter(|(_, item)| wanted.is_subset(&item.tags))
            .map(|(name, _)| name.clone())
            .collect();
        out.sort();
        out
    }
}
