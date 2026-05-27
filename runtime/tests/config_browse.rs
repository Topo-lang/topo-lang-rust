//! Parity with `test_config_browse.py`: the unified browse + agent
//! introspection — one call returning, within the caller's read tier, a
//! self-describing row per runtime item, routed through the tier-aware
//! door (never the tier-blind resolve_all), plus the tiered-transparency
//! invariant and the structurally separate dev-phase `d` listing.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{i, layered_store, s};
use topo::config::ProductConfig;
use topo::config_model::{
    BrowseEntry, ConfigValue, ImpactLevel, ItemPolicy, Layer,
};

fn by_key(rows: &[BrowseEntry]) -> BTreeMap<String, BrowseEntry> {
    rows.iter().map(|r| (r.key.clone(), r.clone())).collect()
}

fn tagset(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// -- FullPerItemSchema ---------------------------------------------------

#[test]
fn every_documented_field_present_and_correct() {
    let store = layered_store();
    let rows = store.browse(None, store.max_read_level()).unwrap();
    let by = by_key(&rows);

    // b-sourced: log.level. default == effective == inlined b value;
    // provenance B; low impact, open read tier.
    let log = &by["log.level"];
    assert_eq!(log.r#type, "str");
    assert_eq!(log.default, Some(s("warn")));
    assert_eq!(log.effective, s("warn"));
    assert_eq!(log.layer, Layer::B);
    assert_eq!(log.impact, ImpactLevel::Low);
    assert_eq!(log.required_write_level, 0);
    assert_eq!(log.required_read_level, 0);
    assert_eq!(log.tags, tagset(&["obs"]));

    // a-sourced: external overrode the inlined default. default is still
    // the b value, effective is the a value, provenance A.
    let net = &by["net.timeout_ms"];
    assert_eq!(net.r#type, "int");
    assert_eq!(net.default, Some(i(1000)));
    assert_eq!(net.effective, i(5000));
    assert_eq!(net.layer, Layer::A);
    assert_eq!(net.impact, ImpactLevel::High);
    assert_eq!(net.required_write_level, 1);
    assert_eq!(net.required_read_level, 0);
    assert_eq!(net.tags, tagset(&["network", "tuning"]));

    // c-sourced over a b default.
    let cache = &by["cache.size"];
    assert_eq!(cache.r#type, "int");
    assert_eq!(cache.default, Some(i(256)));
    assert_eq!(cache.effective, i(512));
    assert_eq!(cache.layer, Layer::C);

    // c-sourced with NO inlined default -> the no-default sentinel
    // (None), never a fabricated value, still typed from the effective.
    let flag = &by["feature.flag"];
    assert_eq!(flag.r#type, "bool");
    assert_eq!(flag.default, None);
    assert_eq!(flag.effective, ConfigValue::Bool(true));
    assert_eq!(flag.layer, Layer::C);

    // Gated item, visible only at/above its tier; both permission roles
    // exposed: read tier 2, write gate 1 (HIGH impact).
    let dsn = &by["db.dsn"];
    assert_eq!(dsn.required_read_level, 2);
    assert_eq!(dsn.required_write_level, 1);
    assert_eq!(dsn.r#type, "str");
}

// -- TieredTransparencyInvariant -----------------------------------------

#[test]
fn gated_item_absent_below_tier_present_at_and_above() {
    let store = layered_store();
    let below: BTreeSet<String> = store
        .browse(None, 0)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert!(!below.contains("db.dsn"));
    let at: BTreeSet<String> = store
        .browse(None, 2)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert!(at.contains("db.dsn"));
    let above: BTreeSet<String> = store
        .browse(None, 5)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert!(above.contains("db.dsn"));
}

#[test]
fn top_level_browse_equals_complete_runtime_key_set() {
    let store = layered_store();
    let top = store.max_read_level();
    let mut browsed: Vec<String> = store
        .browse(None, top)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    browsed.sort();
    assert_eq!(browsed, store.keys());
}

#[test]
fn each_level_is_exactly_that_levels_complete_range() {
    let store = layered_store();
    let mut zero: Vec<String> = store
        .browse(None, 0)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    zero.sort();
    let mut expected: Vec<String> = store
        .keys()
        .into_iter()
        .filter(|k| store.policy_of(k).read_level == 0)
        .collect();
    expected.sort();
    assert_eq!(zero, expected);
}

// -- RoutesThroughTierAwareDoor ------------------------------------------

#[test]
fn browse_does_not_use_resolve_all_to_leak() {
    let store = layered_store();
    // resolve_all is tier-blind: it surfaces the gated key at any level.
    let tier_blind: BTreeSet<String> =
        store.resolve_all().unwrap().keys().cloned().collect();
    assert!(tier_blind.contains("db.dsn"));

    let browsed: BTreeSet<String> = store
        .browse(None, 0)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert!(!browsed.contains("db.dsn"));
    assert_ne!(browsed, tier_blind);
    for r in store.browse(None, 0).unwrap() {
        assert_ne!(r.key, "db.dsn");
    }
}

// -- IdentityIndependence ------------------------------------------------

#[test]
fn same_level_yields_identical_browse() {
    let store = layered_store();
    let a = store.browse(None, 1).unwrap();
    let b = store.browse(None, 1).unwrap();
    // Same level -> structurally identical rows regardless of any caller
    // notion (BrowseEntry derives PartialEq). The signature carries only
    // a credential level (i64), so there is no identity param to vary.
    assert_eq!(a, b);
}

// -- LiveDerivedNoStaticList ---------------------------------------------

#[test]
fn key_added_after_construction_auto_appears() {
    let mut store = layered_store();
    let before: BTreeSet<String> = store
        .browse(None, 0)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert!(!before.contains("late.added"));
    store.set("late.added", s("hi"), 0).unwrap();
    let after: BTreeSet<String> = store
        .browse(None, 0)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert!(after.contains("late.added"));
}

// -- DevPhaseDListing ----------------------------------------------------

#[test]
fn d_absent_from_runtime_browse_at_every_level() {
    let mut cfg = ProductConfig::builder()
        .inlined(common::map(vec![("log.level", s("warn"))]))
        .build()
        .unwrap();
    cfg.declare_internal("BUILD_SALT", s("abc123"), ["crypto"])
        .unwrap();
    cfg.declare_internal("MAX_WIDGETS", i(64), ["limits"])
        .unwrap();
    for level in [0, 1, 99] {
        let keys: BTreeSet<String> = cfg
            .browse(None, level)
            .unwrap()
            .iter()
            .map(|r| r.key.clone())
            .collect();
        assert!(!keys.contains("BUILD_SALT"));
        assert!(!keys.contains("MAX_WIDGETS"));
    }
    assert!(!cfg.keys().contains(&"BUILD_SALT".to_string()));
}

#[test]
fn d_present_only_in_dev_listing_and_tag_searchable() {
    let mut cfg = ProductConfig::in_memory();
    cfg.declare_internal("BUILD_SALT", s("abc123"), ["crypto"])
        .unwrap();
    cfg.declare_internal("MAX_WIDGETS", i(64), ["limits"])
        .unwrap();

    let listed: BTreeSet<String> =
        cfg.dev_browse(None).iter().map(|r| r.name.clone()).collect();
    assert_eq!(listed, tagset(&["BUILD_SALT", "MAX_WIDGETS"]));

    let crypto = cfg.dev_browse(Some(&["crypto"]));
    assert_eq!(
        crypto.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
        vec!["BUILD_SALT"]
    );
    assert_eq!(crypto[0].value, s("abc123"));
    assert_eq!(crypto[0].tags, tagset(&["crypto"]));
}

#[test]
fn dev_browse_shape_is_distinct_from_runtime_entry() {
    let mut cfg = ProductConfig::in_memory();
    cfg.declare_internal("BUILD_SALT", s("abc123"), ["crypto"])
        .unwrap();
    let rec = &cfg.dev_browse(None)[0];
    // A dev record is its own type with exactly {name, value, tags},
    // never a BrowseEntry — the two ranges are structurally disjoint
    // (a Rust type-level guarantee, asserted here for the record).
    assert_eq!(rec.name, "BUILD_SALT");
    assert_eq!(rec.value, s("abc123"));
    assert_eq!(rec.tags, tagset(&["crypto"]));
}

#[test]
fn no_d_declared_yields_empty_listing_without_registry() {
    let cfg = ProductConfig::in_memory();
    // Browsing the empty dev band must not even create the side registry
    // (a runtime-only build never builds it).
    assert!(cfg.dev_browse(None).is_empty());
    assert!(cfg.dev_internal().is_none());
}

// -- ProductConfigBrowseParity ------------------------------------------

#[test]
fn bridge_browse_is_passthrough_to_model() {
    let mut cfg = ProductConfig::builder()
        .inlined(common::map(vec![("a.x", i(1)), ("b.y", s("two"))]))
        .injected(common::map(vec![("a.x", i(9))]))
        .build()
        .unwrap();
    cfg.declare(
        "b.y",
        ItemPolicy::new().with_tags(["t"]).with_read_level(1),
    )
    .unwrap();
    // Below tier: gated key absent.
    let low: BTreeSet<String> = cfg
        .browse(None, 0)
        .unwrap()
        .iter()
        .map(|r| r.key.clone())
        .collect();
    assert_eq!(low, tagset(&["a.x"]));
    // At tier: full range.
    let full = cfg.browse(None, cfg.max_read_level()).unwrap();
    let full_keys: BTreeSet<String> = full.iter().map(|r| r.key.clone()).collect();
    assert_eq!(full_keys, tagset(&["a.x", "b.y"]));
    let ax = by_key(&full)["a.x"].clone();
    assert_eq!(ax.default, Some(i(1)));
    assert_eq!(ax.effective, i(9));
    assert_eq!(ax.layer, Layer::C);
}
