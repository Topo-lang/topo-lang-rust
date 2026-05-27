//! Parity with `test_config_rw.py`: get/set over the frozen b ◁ a ◁ c
//! precedence, stdlib-contract value validation, and the
//! identity-independent high-impact write gate.

mod common;

use std::collections::BTreeMap;

use common::{i, map, s};
use topo::config::ProductConfig;
use topo::config_model::{
    ConfigError, ConfigStore, ConfigValue, ImpactLevel, ItemPolicy, Layer, LayeredConfig,
};

// -- ReadWriteRoundTrip --------------------------------------------------

#[test]
fn set_then_get_through_store() {
    let mut store = ConfigStore::new(
        LayeredConfig {
            inlined: map(vec![("log.level", s("warn"))]),
            ..Default::default()
        },
        BTreeMap::new(),
    );
    // An external write must override the inlined default for the same
    // key and report A as the provenance.
    store.set("log.level", s("debug"), 0).unwrap();
    assert_eq!(store.get("log.level").unwrap(), s("debug"));
    assert_eq!(store.resolve("log.level").unwrap().layer, Layer::A);
    // An injected (c) value still wins over the external write.
    store
        .layered_mut()
        .injected
        .insert("log.level".to_string(), s("trace"));
    assert_eq!(store.get("log.level").unwrap(), s("trace"));
    assert_eq!(store.resolve("log.level").unwrap().layer, Layer::C);
}

#[test]
fn get_default_only_when_no_layer_sets_key() {
    let store = ConfigStore::new(
        LayeredConfig {
            inlined: map(vec![("present", i(1))]),
            ..Default::default()
        },
        BTreeMap::new(),
    );
    assert_eq!(store.get_or("absent", i(42)), i(42));
    // No default -> no silent None.
    assert!(matches!(
        store.get("absent").unwrap_err(),
        ConfigError::KeyNotFound(_)
    ));
    assert_eq!(store.get_or("present", i(99)), i(1));
}

#[test]
fn set_reflected_in_serialized_external_toml() {
    let dir = std::env::temp_dir().join(format!("topo-cfg-rw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("topo-app.toml");

    let mut pc = ProductConfig::builder().path(&path).build().unwrap();
    pc.set("cache.size", i(256), 0).unwrap();
    pc.set("log.level", s("debug"), 0).unwrap();
    pc.set(
        "feature.flags",
        ConfigValue::Array(vec![s("a"), s("b")]),
        0,
    )
    .unwrap();

    // Round-trips through the real ecosystem TOML parser.
    let text = std::fs::read_to_string(&path).unwrap();
    let reloaded: toml::Value = text.parse().unwrap();
    assert_eq!(reloaded["cache"]["size"].as_integer(), Some(256));
    assert_eq!(reloaded["log"]["level"].as_str(), Some("debug"));
    let flags = reloaded["feature"]["flags"].as_array().unwrap();
    assert_eq!(flags[0].as_str(), Some("a"));
    assert_eq!(flags[1].as_str(), Some("b"));

    // A fresh ProductConfig over the same file reads it back.
    let pc2 = ProductConfig::builder().path(&path).build().unwrap();
    assert_eq!(pc2.get("cache.size").unwrap(), i(256));
    assert_eq!(
        pc2.get("feature.flags").unwrap(),
        ConfigValue::Array(vec![s("a"), s("b")])
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn keys_enumerates_all_layers() {
    let mut store = ConfigStore::new(
        LayeredConfig {
            inlined: map(vec![("a.x", i(1))]),
            injected: map(vec![("c.z", i(3))]),
            ..Default::default()
        },
        BTreeMap::new(),
    );
    store.set("b.y", i(2), 0).unwrap();
    assert_eq!(store.keys(), vec!["a.x", "b.y", "c.z"]);
}

// -- ValueTypeContract ---------------------------------------------------

#[test]
fn stdlib_scalars_accepted() {
    let mut store = ConfigStore::default();
    store.set("s", s("str"), 0).unwrap();
    store.set("i", i(7), 0).unwrap();
    store.set("f", ConfigValue::Float(1.5), 0).unwrap();
    store.set("b", ConfigValue::Bool(true), 0).unwrap();
    store
        .set("arr", ConfigValue::Array(vec![i(1), i(2), i(3)]), 0)
        .unwrap();
    let rec: BTreeMap<String, ConfigValue> = [
        ("id".to_string(), i(1)),
        ("amount".to_string(), ConfigValue::Float(2.0)),
    ]
    .into_iter()
    .collect();
    store
        .set("rec", ConfigValue::Record(rec.clone()), 0)
        .unwrap();
    assert_eq!(store.get("rec").unwrap(), ConfigValue::Record(rec));
}

#[test]
fn datetime_rejected_points_to_stdlib_bridging_gap() {
    let mut store = ConfigStore::default();
    let err = store
        .set("event.at", ConfigValue::Datetime("2026-05-16T12:00:00".into()), 0)
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("event.at"), "locates the key: {msg}");
    assert!(msg.contains("stdlib-bridging-types"), "names the gap source: {msg}");
    assert!(msg.contains("time_*"), "names the missing family: {msg}");
}

#[test]
fn datetime_nested_in_array_rejected() {
    let mut store = ConfigStore::default();
    let err = store
        .set(
            "schedule.points",
            ConfigValue::Array(vec![ConfigValue::Datetime("2026-01-01".into())]),
            0,
        )
        .unwrap_err();
    assert!(err.to_string().contains("schedule.points"));
}

#[test]
fn build_toolchain_key_still_rejected_on_write() {
    let mut store = ConfigStore::default();
    let err = store.set("build.standard", s("c++20"), 0).unwrap_err();
    assert!(err.to_string().contains("Topo.toml"));
}

// -- WriteProtectionGate -------------------------------------------------

fn gated_store() -> ConfigStore {
    let mut store = ConfigStore::default();
    store
        .declare("db.dsn", ItemPolicy::new().with_impact(ImpactLevel::High))
        .unwrap();
    store
        .declare("ui.theme", ItemPolicy::new().with_impact(ImpactLevel::Low))
        .unwrap();
    store
}

#[test]
fn high_impact_write_without_credential_rejected() {
    let mut store = gated_store();
    let err = store.set("db.dsn", s("postgres://prod"), 0).unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, ConfigError::WriteProtection(_)));
    assert!(msg.contains("db.dsn"));
    assert!(msg.contains("HIGH"));
    // The guard message is about credentials, never about identity.
    assert!(!msg.to_lowercase().contains("human"));
    assert!(!msg.to_lowercase().contains("agent"));
}

#[test]
fn high_impact_write_with_credential_succeeds() {
    let mut store = gated_store();
    store.set("db.dsn", s("postgres://prod"), 1).unwrap();
    assert_eq!(store.get("db.dsn").unwrap(), s("postgres://prod"));
}

#[test]
fn low_impact_write_needs_no_credential() {
    let mut store = gated_store();
    store.set("ui.theme", s("dark"), 0).unwrap();
    assert_eq!(store.get("ui.theme").unwrap(), s("dark"));
}

#[test]
fn undeclared_item_defaults_to_low_impact() {
    let mut store = ConfigStore::default();
    store.set("anything.unlisted", i(1), 0).unwrap();
    assert_eq!(store.get("anything.unlisted").unwrap(), i(1));
}

#[test]
fn gate_is_identity_independent() {
    // The authorize/set surface takes a credential *level* and no
    // principal: a "human" and an "agent" presenting the same level get
    // the exact same outcome. The Rust signatures structurally cannot
    // carry an identity (the only extra param is `credential_level: i64`);
    // behavioural equivalence is asserted here.
    let mut store_a = gated_store();
    let mut store_b = gated_store();
    assert!(store_a.set("db.dsn", s("x"), 0).is_err()); // "the human"
    assert!(store_b.set("db.dsn", s("x"), 0).is_err()); // "the agent"
    store_a.set("db.dsn", s("ok"), 1).unwrap();
    store_b.set("db.dsn", s("ok"), 1).unwrap();
    assert_eq!(
        store_a.get("db.dsn").unwrap(),
        store_b.get("db.dsn").unwrap()
    );
}
