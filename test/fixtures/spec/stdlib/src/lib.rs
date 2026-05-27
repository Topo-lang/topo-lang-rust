// Matching Rust host source for minimal.topo.
//
// Each function signature corresponds 1:1 to the .topo declaration via the
// RustEmitter stdlib mapping. This file is reference material for the spec
// fixture; topo-check L1 will treat these as the host implementations when
// the surrounding harness runs.
//
// Lifetime annotations: Rust's elision rules apply to every signature here
// (single input ref, or no ref → static); no explicit `'a` is required. If
// future fixtures need named lifetimes, that becomes a host-side concern
// rather than an emitter-mapping concern (stdlib-types and
// idiom-transpile are orthogonal axes).

pub fn isReady() -> bool {
    true
}

pub fn nextId() -> i64 {
    42
}

pub fn averageScore() -> f64 {
    0.75
}

pub fn label() -> &'static str {
    "topo"
}

pub fn maybeFlag() -> Option<bool> {
    None
}

pub fn samples() -> &'static [f64] {
    &[1.0, 2.0, 3.0]
}

pub fn boundary(
    id: i64,
    name: &str,
    flags: Option<bool>,
    values: &[f64],
) -> Option<i64> {
    match flags {
        None => None,
        Some(_) => {
            if !name.is_empty() && !values.is_empty() {
                Some(id)
            } else {
                None
            }
        }
    }
}
