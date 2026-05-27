//! Parity with `test_config_model.py`: frozen a/b/c merge precedence,
//! per-value provenance, the Topo.toml boundary guard.

mod common;

use common::{i, map, s};
use topo::config_model::{
    iter_provenance, merge_layers, reject_if_build_config_key, ConfigError, ConfigValue, Layer,
    LayeredConfig, PRODUCT_CONFIG_FILENAME,
};

fn sample() -> LayeredConfig {
    // Each layer is the sole winner of at least one key, plus a key all
    // three set so precedence is unambiguous.
    LayeredConfig {
        inlined: map(vec![
            ("log.level", s("warn")),  // only b -> b wins
            ("cache.size", i(64)),     // b, overridden by a
            ("retry.count", i(1)),     // b, overridden by a and c
        ]),
        external: map(vec![
            ("cache.size", i(256)),       // a beats b
            ("retry.count", i(3)),        // a beats b, lost to c
            ("feature.flag", ConfigValue::Bool(true)), // only a -> a wins
        ]),
        injected: map(vec![
            ("retry.count", i(9)),        // c beats a and b
            ("tracing.enabled", ConfigValue::Bool(false)), // only c -> c wins
        ]),
    }
}

#[test]
fn each_key_has_unique_effective_value_and_provenance() {
    let resolved = sample().resolve_all().unwrap();

    assert_eq!(resolved["log.level"].value, s("warn"));
    assert_eq!(resolved["log.level"].layer, Layer::B);

    assert_eq!(resolved["cache.size"].value, i(256));
    assert_eq!(resolved["cache.size"].layer, Layer::A);

    assert_eq!(resolved["feature.flag"].value, ConfigValue::Bool(true));
    assert_eq!(resolved["feature.flag"].layer, Layer::A);

    // Set by all three layers: c (most explicit) must win.
    assert_eq!(resolved["retry.count"].value, i(9));
    assert_eq!(resolved["retry.count"].layer, Layer::C);

    assert_eq!(resolved["tracing.enabled"].value, ConfigValue::Bool(false));
    assert_eq!(resolved["tracing.enabled"].layer, Layer::C);
}

#[test]
fn keys_enumerates_every_layer_once_sorted() {
    assert_eq!(
        sample().keys(),
        vec![
            "cache.size",
            "feature.flag",
            "log.level",
            "retry.count",
            "tracing.enabled",
        ]
    );
}

#[test]
fn iter_provenance_triples() {
    let triples = iter_provenance(&sample().resolve_all().unwrap());
    assert_eq!(
        triples,
        vec![
            ("cache.size".to_string(), i(256), Layer::A),
            ("feature.flag".to_string(), ConfigValue::Bool(true), Layer::A),
            ("log.level".to_string(), s("warn"), Layer::B),
            ("retry.count".to_string(), i(9), Layer::C),
            (
                "tracing.enabled".to_string(),
                ConfigValue::Bool(false),
                Layer::C
            ),
        ]
    );
}

#[test]
fn merge_layers_helper_matches() {
    let resolved = merge_layers(
        map(vec![("x", i(1))]),
        map(vec![("x", i(2))]),
        map(vec![("x", i(3))]),
    )
    .unwrap();
    assert_eq!(resolved["x"].value, i(3));
    assert_eq!(resolved["x"].layer, Layer::C);
}

#[test]
fn unknown_key_raises() {
    let err = sample().resolve("does.not.exist").unwrap_err();
    assert!(matches!(err, ConfigError::KeyNotFound(_)));
}

#[test]
fn d_layer_is_not_a_runtime_merge_layer() {
    // d exists in the vocabulary but is promoted to code, never merged
    // at runtime — asking the model to read it as a layer is an explicit
    // construction error (Python raises AssertionError), not a silent
    // empty result. Surfaced via the public resolve path that walks the
    // merge order; D is not in that order so it is unreachable, and the
    // explicit guard is exercised through resolve_all's validation never
    // touching D plus the dedicated layer_map error below.
    let cfg = LayeredConfig::default();
    // Resolving any key must never produce a D-layer result.
    let err = cfg.resolve("anything").unwrap_err();
    assert!(matches!(err, ConfigError::KeyNotFound(_)));
    // Direct probe of the layer-map guard: D is rejected outright, the
    // structural equivalent of the Python `AssertionError`.
    assert!(cfg.layer_map_for(Layer::D).is_err());
    assert!(cfg.layer_map_for(Layer::B).is_ok());
}

#[test]
fn build_section_key_rejected_and_points_to_topo_toml() {
    let err = reject_if_build_config_key("build.language").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Topo.toml"), "msg: {msg}");
    assert!(msg.contains(PRODUCT_CONFIG_FILENAME), "msg: {msg}");
}

#[test]
fn feature_mode_section_key_rejected() {
    for key in [
        "parallel.mode",
        "adaptive.min_trigger_ns",
        "optimize.indirection",
        "check.jobs",
        "topo.root",
    ] {
        assert!(
            reject_if_build_config_key(key).is_err(),
            "expected rejection for {key}"
        );
    }
}

#[test]
fn build_key_in_a_layer_rejected_on_resolve_all() {
    let cfg = LayeredConfig {
        external: map(vec![("build.standard", s("c++20"))]),
        ..Default::default()
    };
    let err = cfg.resolve_all().unwrap_err();
    assert!(err.to_string().contains("Topo.toml"));
}

#[test]
fn product_key_with_similar_name_is_not_rejected() {
    // Only the exact build sections are off-limits; product keys that
    // merely look related are fine.
    reject_if_build_config_key("checkout.timeout_ms").unwrap(); // not [check]
    reject_if_build_config_key("testing_endpoint.url").unwrap(); // not [test]
    let cfg = LayeredConfig {
        inlined: map(vec![("checkout.timeout_ms", i(5000))]),
        ..Default::default()
    };
    assert_eq!(cfg.resolve("checkout.timeout_ms").unwrap().value, i(5000));
}
