# Rust syn parser boundary

> Why `topo-lang-rust/` ships two crates that both depend on `syn`
> instead of one shared crate, and what divides their responsibilities.

## TL;DR

`topo-lang-rust/topo-check/extractor/` (`topo-extract-rust`) and
`topo-lang-rust/topo-build/static-frontend/`
(`topo-app-static-rust`) both depend on `syn = "2"` with overlapping
feature sets (`full`, `parsing`, `visit`). The duplication is a
deliberate boundary, not an oversight: the two crates do related but
distinct jobs at different points in the toolchain, with different
ownership and lifecycle.

## Why two crates

| crate | role | invoked by | output |
|-------|------|------------|--------|
| `topo-extract-rust` | function-body / type-def extractor | `topo-check` subprocess | TranspileModel wire JSON for the check / transpile path |
| `topo-app-static-rust` | static topo-app graph reconstructor | user / `topo-build` build orchestration | `.topo` file emitted from the static analysis of registration calls |

The check-path extractor must run as a subprocess so `topo-check` can
treat each Rust source file as an isolated unit and parallelise across
files; the static-frontend is invoked once per crate as a library + CLI
that walks the whole crate's call graph to reconstruct the topo-app
registration order. They have different invocation models, different
caching strategies, different error surfaces (`topo-check` consumes
wire JSON; the static-frontend produces emitted `.topo` text), and
different lifecycles.

## What is deliberately shared

The dependency surface only: same `syn` major version, same feature
set. Both crates depend on `syn = "2"` with `full`, `parsing`, `visit`
so a Rust syntax extension that lands in one is already supported in
the other without a separate dep bump. This is the *minimum* shared
surface that keeps the two from drifting on `syn` semver alone.

## What is deliberately not shared

Type-conversion helpers (`trait_bound_to_node`, `type_to_ref`,
`path_to_type_node`, the visitor structs, the error types) live
independently in each crate. The two have different output shapes —
`topo-extract-rust` populates fields the static-frontend ignores
(`ownership`, `unsupported`, fidelity downgrades) and vice versa
(`registration_order`, `flow_edges`). Forcing them through a single
`type → node` helper would mean the helper has to accept the union of
both consumers' fields and one of the two has to silently ignore the
other half — a worse seam than two purpose-built helpers.

## When to revisit

Promote the syn-using code into a `topo-syn-common/` crate when:

- One of the two helpers gains a non-trivial bug that needs the same
  fix applied to the other (currently zero such bugs);
- `syn` 3 lands and the dependency-bump cadence becomes a real
  coordination cost;
- A third Rust-source-consuming crate appears under `topo-lang-rust/`
  (then "three independent visitors" is the moment to extract).

Until one of those triggers fires, two purpose-built crates with the
same `syn` dep is the right shape.

## Pointers

- `topo-lang-rust/topo-check/extractor/Cargo.toml` — extractor crate.
- `topo-lang-rust/topo-build/static-frontend/Cargo.toml` — static crate
  (its own `Cargo.toml` carries a comment noting the shared `syn`
  dependency).
- `topo-lang-rust/topo-check/extractor/src/main.rs` —
  `trait_bound_to_node`, `path_to_type_node`.
- `topo-lang-rust/topo-build/static-frontend/src/analyze.rs` —
  `type_to_ref`, the static visitor.
