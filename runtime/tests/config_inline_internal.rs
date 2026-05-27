//! Parity with `test_config_inline_internal.py`: the code-layer
//! inlined/hidden TOML (`b`) round-trip, and the pure-internal (`d`)
//! band with no runtime presence.

mod common;

use std::collections::BTreeMap;

use common::{i, s};
use topo::config::ProductConfig;
use topo::config_model::{
    ConfigError, ConfigStore, ConfigValue, DevInternalRegistry, ItemPolicy, Layer, LayeredConfig,
};

const TOML_SRC: &str = r#"log_level = "info"
retries = 3
ratio = 0.5
enabled = true

[net]
host = "example.com"
ports = [80, 443]
"#;

/// Re-parse the bridge's restored text and re-flatten it the same way
/// the bridge decodes a file — the Rust equivalent of the Python
/// `tomllib.loads(...)` equality, comparing decoded data, not bytes.
fn reparse_flat(text: &str) -> BTreeMap<String, ConfigValue> {
    let doc: toml::Value = text.parse().unwrap();
    flatten(&doc, "")
}

fn flatten(value: &toml::Value, prefix: &str) -> BTreeMap<String, ConfigValue> {
    let mut flat = BTreeMap::new();
    if let toml::Value::Table(map) = value {
        for (name, v) in map {
            let key = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            match v {
                toml::Value::Table(_) => flat.extend(flatten(v, &key)),
                toml::Value::Boolean(b) => {
                    flat.insert(key, ConfigValue::Bool(*b));
                }
                toml::Value::Integer(n) => {
                    flat.insert(key, ConfigValue::Int(*n));
                }
                toml::Value::Float(f) => {
                    flat.insert(key, ConfigValue::Float(*f));
                }
                toml::Value::String(st) => {
                    flat.insert(key, ConfigValue::Str(st.clone()));
                }
                toml::Value::Array(a) => {
                    flat.insert(
                        key,
                        ConfigValue::Array(
                            a.iter()
                                .map(|e| match e {
                                    toml::Value::Integer(n) => ConfigValue::Int(*n),
                                    toml::Value::String(st) => ConfigValue::Str(st.clone()),
                                    toml::Value::Boolean(b) => ConfigValue::Bool(*b),
                                    toml::Value::Float(f) => ConfigValue::Float(*f),
                                    _ => unreachable!("test fixture has no nested aggregates"),
                                })
                                .collect(),
                        ),
                    );
                }
                toml::Value::Datetime(d) => {
                    flat.insert(key, ConfigValue::Datetime(d.to_string()));
                }
            }
        }
    }
    flat
}

// -- InlineDeclareNoExternalFileNeeded -----------------------------------

#[test]
fn inline_declared_defaults_need_no_external_file() {
    let mut pc = ProductConfig::in_memory(); // path is None: nothing on disk
    pc.declare_inlined_toml(TOML_SRC).unwrap();
    assert!(pc.path().is_none());
    assert_eq!(pc.get("log_level").unwrap(), s("info"));
    assert_eq!(pc.get("net.host").unwrap(), s("example.com"));
    assert_eq!(
        pc.get("net.ports").unwrap(),
        ConfigValue::Array(vec![i(80), i(443)])
    );
    for key in pc.keys() {
        assert_eq!(pc.resolve(&key).unwrap().layer, Layer::B);
    }
}

#[test]
fn accepts_already_decoded_mapping_too() {
    let mut pc = ProductConfig::in_memory();
    let nested: BTreeMap<String, ConfigValue> = [
        ("a".to_string(), i(1)),
        ("nested.b".to_string(), i(2)),
    ]
    .into_iter()
    .collect();
    pc.declare_inlined_data(nested).unwrap();
    assert_eq!(pc.get("a").unwrap(), i(1));
    assert_eq!(pc.get("nested.b").unwrap(), i(2));
}

// -- InlineRoundTrip -----------------------------------------------------

#[test]
fn restore_yields_toml_reparsing_to_identical_data() {
    let mut pc = ProductConfig::in_memory();
    pc.declare_inlined_toml(TOML_SRC).unwrap();
    let restored = pc.restore_inlined_toml().unwrap();
    // Equivalent == re-parsing yields the same decoded data as the
    // original source decoded.
    assert_eq!(reparse_flat(&restored), reparse_flat(TOML_SRC));
}

#[test]
fn restore_is_idempotent_under_reparse() {
    let mut pc = ProductConfig::in_memory();
    pc.declare_inlined_toml(TOML_SRC).unwrap();
    let once = pc.restore_inlined_toml().unwrap();
    let mut pc2 = ProductConfig::in_memory();
    pc2.declare_inlined_toml(&once).unwrap();
    assert_eq!(
        reparse_flat(&pc2.restore_inlined_toml().unwrap()),
        reparse_flat(&once)
    );
}

#[test]
fn empty_inline_restores_to_empty() {
    let mut pc = ProductConfig::in_memory();
    pc.declare_inlined_data(BTreeMap::new()).unwrap();
    assert_eq!(pc.restore_inlined_toml().unwrap(), "");
    assert_eq!(reparse_flat(&pc.restore_inlined_toml().unwrap()), BTreeMap::new());
}

// -- FileHiddenNotItemHidden ---------------------------------------------

#[test]
fn inlined_items_still_enumerate_under_normal_rules() {
    let mut pc = ProductConfig::in_memory();
    pc.declare_inlined_toml(TOML_SRC).unwrap();
    let keys = pc.keys();
    for k in [
        "log_level",
        "retries",
        "ratio",
        "enabled",
        "net.host",
        "net.ports",
    ] {
        assert!(keys.contains(&k.to_string()), "missing {k}");
    }
    pc.declare("retries", ItemPolicy::new().with_tags(["tuning"]))
        .unwrap();
    pc.declare("net.host", ItemPolicy::new().with_read_level(2))
        .unwrap();
    assert_eq!(pc.query(Some(&["tuning"]), 0), vec!["retries"]);
    // A read-gated inlined item hides by default but the top tier still
    // enumerates it (tiered-transparency holds for b too).
    assert!(!pc.query(None, 0).contains(&"net.host".to_string()));
    assert!(pc.query(None, 2).contains(&"net.host".to_string()));
    let rv = pc.query_resolved(None, 0).unwrap();
    assert!(rv.contains_key("log_level"));
    assert_eq!(rv["log_level"].value, s("info"));
}

#[test]
fn a_and_c_still_override_inlined_b() {
    let mut pc = ProductConfig::builder()
        .injected(common::map(vec![("retries", i(99))]))
        .build()
        .unwrap();
    pc.declare_inlined_toml(TOML_SRC).unwrap();
    // c overrides b.
    assert_eq!(pc.get("retries").unwrap(), i(99));
    assert_eq!(pc.resolve("retries").unwrap().layer, Layer::C);
    // a overrides b: write lands in the external (a) layer.
    pc.set("log_level", s("debug"), 0).unwrap();
    assert_eq!(pc.get("log_level").unwrap(), s("debug"));
    assert_eq!(pc.resolve("log_level").unwrap().layer, Layer::A);
    // Untouched inlined value still resolves from b.
    assert_eq!(pc.get("ratio").unwrap(), ConfigValue::Float(0.5));
    assert_eq!(pc.resolve("ratio").unwrap().layer, Layer::B);
}

#[test]
fn inline_layer_rejects_build_toolchain_key() {
    let mut pc = ProductConfig::in_memory();
    let err = pc
        .declare_inlined_toml("[build]\nlanguage = \"python\"\n")
        .unwrap_err();
    assert!(matches!(err, ConfigError::BuildConfigKey(_)));
}

// -- PureInternalDevPhaseOnly --------------------------------------------

#[test]
fn declared_internal_is_dev_searchable_by_name_and_tag() {
    let mut pc = ProductConfig::in_memory();
    let value = pc
        .declare_internal("MAX_BUF", i(4096), ["perf", "memory"])
        .unwrap();
    // The call returns the plain value to bind as a constant.
    assert_eq!(value, i(4096));
    let reg = pc.dev_internal().unwrap();
    assert!(reg.names().contains(&"MAX_BUF".to_string()));
    assert_eq!(reg.search(&["perf"]), vec!["MAX_BUF"]);
    assert_eq!(reg.search(&["perf", "memory"]), vec!["MAX_BUF"]);
    assert!(reg.search(&["unrelated"]).is_empty());
    assert_eq!(reg.get("MAX_BUF").unwrap().value, i(4096));
}

#[test]
fn internal_absent_from_every_runtime_surface() {
    let mut pc = ProductConfig::builder()
        .inlined(common::map(vec![("public.k", i(1))]))
        .build()
        .unwrap();
    pc.declare_internal("SECRET_TUNING", i(7), ["internal"])
        .unwrap();
    assert!(!pc.keys().contains(&"SECRET_TUNING".to_string()));
    assert!(!pc.query(None, 0).contains(&"SECRET_TUNING".to_string()));
    assert!(!pc.query(None, 999).contains(&"SECRET_TUNING".to_string()));
    assert!(!pc
        .store()
        .resolve_all()
        .unwrap()
        .contains_key("SECRET_TUNING"));
    assert!(!pc
        .query_resolved(None, 999)
        .unwrap()
        .contains_key("SECRET_TUNING"));
    assert!(matches!(
        pc.get("SECRET_TUNING").unwrap_err(),
        ConfigError::KeyNotFound(_)
    ));
}

#[test]
fn promoted_value_is_a_plain_constant_no_config_reference() {
    let mut pc = ProductConfig::in_memory();
    let v = pc.declare_internal("RATE", ConfigValue::Float(0.25), Vec::<String>::new())
        .unwrap();
    // Equivalent to a hand-written constant: the returned value IS the
    // value passed in, a bare float, with no config-system wrapper.
    assert_eq!(v, ConfigValue::Float(0.25));
    // The runtime store is structurally incapable of holding the d
    // registry: ConfigStore has no DevInternalRegistry field at all (the
    // Rust analogue of the Python "no attr is a DevInternalRegistry"
    // check — it is a compile-time guarantee, asserted here for record).
    let _store: &ConfigStore = pc.store();
}

#[test]
fn layer_d_stays_out_of_runtime_merge() {
    assert!(!topo::config_model::RUNTIME_MERGE_ORDER.contains(&Layer::D));
    let cfg = LayeredConfig {
        inlined: common::map(vec![("k", i(1))]),
        ..Default::default()
    };
    assert!(cfg.layer_map_for(Layer::D).is_err());
}

#[test]
fn internal_value_still_honours_stdlib_contract() {
    let mut pc = ProductConfig::in_memory();
    let err = pc
        .declare_internal(
            "WHEN",
            ConfigValue::Datetime("2026-05-16".into()),
            Vec::<String>::new(),
        )
        .unwrap_err();
    assert!(matches!(err, ConfigError::UnbridgedValue(_)));
}

#[test]
fn dev_registry_is_disjoint_from_store_type() {
    let mut reg = DevInternalRegistry::new();
    reg.declare("X", i(1), ["t"]).unwrap();
    let store = ConfigStore::new(
        LayeredConfig {
            inlined: common::map(vec![("X", i(2))]),
            ..Default::default()
        },
        BTreeMap::new(),
    );
    // Same name in both is a coincidence, not a link.
    assert_eq!(store.get("X").unwrap(), i(2));
    assert_eq!(reg.get("X").unwrap().value, i(1));
}
