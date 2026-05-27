//! Parity with `test_config_tags_perm.py`: the tag system, the tag-query
//! API, the two orthogonal multi-level permission roles (read-visibility
//! tiering vs the write mis-operation gate), the tiered-transparency
//! invariant, and the bridge exposing the same single query API.

mod common;

use common::{i, tagged_store};
use topo::config::ProductConfig;
use topo::config_model::{
    required_read_level, ConfigError, ConfigStore, ImpactLevel, ItemPolicy,
};

// -- TagQuery ------------------------------------------------------------

#[test]
fn single_tag_returns_exact_subset() {
    let store = tagged_store();
    // network items, but the two permission-gated ones are hidden
    // without a credential.
    assert_eq!(
        store.query(Some(&["network"]), 0),
        vec!["net.retries", "net.timeout_ms"]
    );
}

#[test]
fn multi_tag_is_and_combination() {
    let store = tagged_store();
    // Only net.timeout_ms carries BOTH network and tuning.
    assert_eq!(
        store.query(Some(&["network", "tuning"]), 0),
        vec!["net.timeout_ms"]
    );
    // Order of the requested tags must not matter.
    assert_eq!(
        store.query(Some(&["tuning", "network"]), 0),
        vec!["net.timeout_ms"]
    );
}

#[test]
fn no_tag_returns_all_non_permission_items() {
    let store = tagged_store();
    assert_eq!(
        store.query(None, 0),
        vec!["cache.size", "log.level", "net.retries", "net.timeout_ms"]
    );
}

#[test]
fn tag_with_no_match_returns_empty() {
    let store = tagged_store();
    assert!(store.query(Some(&["does-not-exist"]), 0).is_empty());
}

#[test]
fn query_resolved_carries_values_and_provenance() {
    let store = tagged_store();
    let rv = store.query_resolved(Some(&["tuning"]), 0).unwrap();
    let keys: std::collections::BTreeSet<&String> = rv.keys().collect();
    let expected: std::collections::BTreeSet<String> =
        ["net.timeout_ms".to_string(), "cache.size".to_string()]
            .into_iter()
            .collect();
    assert_eq!(keys, expected.iter().collect());
    assert_eq!(rv["cache.size"].value, i(256));
}

// -- ReadVisibilityTiering -----------------------------------------------

#[test]
fn gated_item_hidden_without_credential() {
    let store = tagged_store();
    assert!(!store.query(None, 0).contains(&"db.dsn".to_string()));
    assert!(!store.query(None, 0).contains(&"secret.api_key".to_string()));
    assert!(matches!(
        store.read("db.dsn", 0).unwrap_err(),
        ConfigError::WriteProtection(_)
    ));
}

#[test]
fn each_level_sees_that_levels_complete_range() {
    let store = tagged_store();
    // Level 2 admits db.dsn (read_level 2) but still not the level-3
    // secret — each tier sees its own complete range.
    let keys_l2 = store.query(None, 2);
    assert!(keys_l2.contains(&"db.dsn".to_string()));
    assert!(!keys_l2.contains(&"secret.api_key".to_string()));
    assert_eq!(
        store.read("db.dsn", 2).unwrap(),
        common::s("postgres://local")
    );
    assert!(store.read("secret.api_key", 2).is_err());
}

#[test]
fn tiered_transparency_highest_level_enumerates_everything() {
    let store = tagged_store();
    let top = store.max_read_level();
    assert_eq!(top, 3);
    let enumerated: std::collections::BTreeSet<String> =
        store.query(None, top).into_iter().collect();
    let all: std::collections::BTreeSet<String> = store.keys().into_iter().collect();
    assert_eq!(enumerated, all);
    // And every item is actually readable at the top level.
    for key in store.keys() {
        store.read(&key, top).unwrap();
    }
}

#[test]
fn tag_filter_and_read_tier_are_orthogonal() {
    let store = tagged_store();
    let top = store.max_read_level();
    assert_eq!(
        store.query(Some(&["network"]), top),
        vec!["db.dsn", "net.retries", "net.timeout_ms", "secret.api_key"]
    );
    assert_eq!(
        store.query(Some(&["network"]), 0),
        vec!["net.retries", "net.timeout_ms"]
    );
}

// -- SameQueryDifferentSites ---------------------------------------------

#[test]
fn two_callsites_different_args_different_visibility() {
    let store = tagged_store();
    let site_one = store.query(Some(&["network"]), 0);
    let site_two = store.query(Some(&["network"]), store.max_read_level());
    assert_ne!(site_one, site_two);
    assert!(!site_one.contains(&"db.dsn".to_string()));
    assert!(site_two.contains(&"db.dsn".to_string()));
}

// -- WriteGateGeneralizedMultiLevel --------------------------------------

#[test]
fn mid_level_threshold_via_required_credential_table() {
    // The write gate is the orthogonal twin of read tiering. Insert a
    // mid threshold by extending the explicit table, not by rewriting
    // logic — proves the scale is multi-level. (The Python suite mutates
    // the module-level dict and restores it; Rust scopes the mutation to
    // this store's own table, which is the cleaner equivalent of the
    // same "table edit, not logic rewrite" assertion.)
    let mut store = ConfigStore::default();
    store
        .credential_table_mut()
        .set(ImpactLevel::High, 2); // re-point HIGH to level 2
    store
        .declare("db.dsn", ItemPolicy::new().with_impact(ImpactLevel::High))
        .unwrap();
    assert!(store.set("db.dsn", common::s("x"), 1).is_err()); // below 2
    store.set("db.dsn", common::s("ok"), 2).unwrap(); // meets 2
    assert_eq!(store.get("db.dsn").unwrap(), common::s("ok"));
}

#[test]
fn read_level_and_write_gate_are_independent_fields() {
    let mut store = ConfigStore::default();
    store
        .declare(
            "public.but.guarded",
            ItemPolicy::new()
                .with_read_level(0)
                .with_impact(ImpactLevel::High),
        )
        .unwrap();
    store
        .declare(
            "gated.but.cheap",
            ItemPolicy::new()
                .with_read_level(2)
                .with_impact(ImpactLevel::Low),
        )
        .unwrap();

    // Freely readable, but a write needs a credential.
    assert_eq!(
        required_read_level(&store.policy_of("public.but.guarded")),
        0
    );
    assert!(store.set("public.but.guarded", i(1), 0).is_err());

    // Read-gated, but writing it needs no credential.
    assert_eq!(required_read_level(&store.policy_of("gated.but.cheap")), 2);
    store.set("gated.but.cheap", i(1), 0).unwrap();
    assert!(store.read("gated.but.cheap", 0).is_err()); // read tier still bites
}

// -- BridgeExposesOneQueryAPI -------------------------------------------

#[test]
fn product_config_query_passthrough() {
    let mut pc = ProductConfig::builder()
        .inlined(common::map(vec![("a", i(1)), ("b", i(2))]))
        .build()
        .unwrap();
    pc.declare("b", ItemPolicy::new().with_tags(["x"])).unwrap();
    pc.declare("a", ItemPolicy::new().with_read_level(2)).unwrap();
    assert_eq!(pc.query(None, 0), vec!["b"]); // a is read-gated
    assert_eq!(pc.query(Some(&["x"]), 0), vec!["b"]);
    assert_eq!(pc.max_read_level(), 2);
    assert_eq!(pc.query(None, 2), vec!["a", "b"]);
    assert_eq!(pc.read("a", 2).unwrap(), i(1));
}
