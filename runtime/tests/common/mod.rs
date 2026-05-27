//! Shared constructors for the config parity suite — the Rust analogue
//! of the `_tagged_store()` / `_layered_store()` helpers the Python test
//! files share. Kept in `tests/common` so every parity file builds the
//! exact same sample as its Python counterpart.
//!
//! Each integration test crate compiles this whole module but uses only
//! the subset it needs; `dead_code` here is an expected artifact of that
//! shared-module model, not unreachable production code.
#![allow(dead_code)]

use std::collections::BTreeMap;

use topo::config_model::{ConfigStore, ConfigValue, ImpactLevel, ItemPolicy, LayeredConfig};

/// Build a flat layer map from `(key, value)` pairs.
pub fn map(pairs: Vec<(&str, ConfigValue)>) -> BTreeMap<String, ConfigValue> {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

pub fn s(v: &str) -> ConfigValue {
    ConfigValue::Str(v.to_string())
}

pub fn i(v: i64) -> ConfigValue {
    ConfigValue::Int(v)
}

pub fn b(v: bool) -> ConfigValue {
    ConfigValue::Bool(v)
}

/// Mirrors Python `_tagged_store()`: freely-combinable tags + a mix of
/// read tiers so tag AND, the no-tag default, and the read tier can each
/// be exercised independently.
pub fn tagged_store() -> ConfigStore {
    let layered = LayeredConfig {
        inlined: map(vec![
            ("log.level", s("warn")),
            ("net.timeout_ms", i(5000)),
            ("net.retries", i(3)),
            ("cache.size", i(256)),
            ("db.dsn", s("postgres://local")),
            ("secret.api_key", s("k-xxx")),
        ]),
        ..Default::default()
    };
    let mut store = ConfigStore::new(layered, BTreeMap::new());
    store
        .declare("log.level", ItemPolicy::new().with_tags(["obs"]))
        .unwrap();
    store
        .declare(
            "net.timeout_ms",
            ItemPolicy::new().with_tags(["network", "tuning"]),
        )
        .unwrap();
    store
        .declare("net.retries", ItemPolicy::new().with_tags(["network"]))
        .unwrap();
    store
        .declare("cache.size", ItemPolicy::new().with_tags(["tuning"]))
        .unwrap();
    store
        .declare(
            "db.dsn",
            ItemPolicy::new()
                .with_tags(["network"])
                .with_read_level(2)
                .with_impact(ImpactLevel::High),
        )
        .unwrap();
    store
        .declare(
            "secret.api_key",
            ItemPolicy::new()
                .with_tags(["network"])
                .with_read_level(3)
                .with_impact(ImpactLevel::High),
        )
        .unwrap();
    store
}

/// Mirrors Python `_layered_store()`: values from all three runtime
/// layers, a mix of impact/read-tier/tags, so every browse row field can
/// be checked against a hand-built sample with known provenance.
pub fn layered_store() -> ConfigStore {
    let layered = LayeredConfig {
        inlined: map(vec![
            ("log.level", s("warn")),
            ("net.timeout_ms", i(1000)),
            ("cache.size", i(256)),
            ("db.dsn", s("postgres://default")),
        ]),
        external: map(vec![("net.timeout_ms", i(5000))]),
        injected: map(vec![("cache.size", i(512)), ("feature.flag", b(true))]),
    };
    let mut store = ConfigStore::new(layered, BTreeMap::new());
    store
        .declare("log.level", ItemPolicy::new().with_tags(["obs"]))
        .unwrap();
    store
        .declare(
            "net.timeout_ms",
            ItemPolicy::new()
                .with_tags(["network", "tuning"])
                .with_impact(ImpactLevel::High),
        )
        .unwrap();
    store
        .declare("cache.size", ItemPolicy::new().with_tags(["tuning"]))
        .unwrap();
    store
        .declare("feature.flag", ItemPolicy::new().with_tags(["features"]))
        .unwrap();
    store
        .declare(
            "db.dsn",
            ItemPolicy::new()
                .with_tags(["network"])
                .with_read_level(2)
                .with_impact(ImpactLevel::High),
        )
        .unwrap();
    store
}
