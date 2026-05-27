# topo-lang-rust -- Rust Language Support

Rust language analysis, extraction, compilation driver, LSP bridge, FFI crate runtime, and proc macros for the Topo toolchain.

## Structure

Second-level directories are named after the `topo-<tool>` they serve, so the mapping from code to top-level component is explicit.

| Directory | Serves | Purpose |
|-----------|--------|---------|
| runtime/ | user code | FFI crate with safe Rust wrappers for parallel, adaptive, jit, observe, arena, pipeline |
| runtime-macros/ | user code | `#[topo_pipeline]` proc macro for pipeline code generation |
| topo-check/analysis/ | topo-check | RustAnalysisProvider + extractors + safety catalog + stub generator |
| topo-check/runner/ | topo-check | RustCheckRunner -- language-specific check orchestration |
| topo-check/extractor/ | topo-check | Cargo-built standalone symbol/body extractor (`topo-extract-rust`) |
| topo-build/ | topo-build | RustDriver (cargo rustc --emit=llvm-bc) |
| topo-init/ | topo-init | Rust project template provider |
| topo-lsp/ | topo-lsp | RustAnalyzerBridge -- proxies rust-analyzer for IDE integration |
| topo-transpile/ | topo-transpile | RustEmitter -- AST → Rust source |
| topo-debug/ | topo-debug | LLDB type-summary formatter for Rust hosts |
| topo-profile/ | topo-profile | Rust-host profile-data tests |
| topo-lang/ | topo-lang | RustPlugin -- registers all components with the language-plugin framework |
| test/ | — | Unit tests (RustStubGenerator, RustExtractorFidelity) |
| examples/ | — | quickstart and showcase projects |

## Build

Standalone build expects two upstream Topo packages installed and
discoverable via `CMAKE_PREFIX_PATH`:

- `topo-core` (built with `TOPO_CORE_WITH_LANG=ON`)
- `topo-lang`

```bash
cmake -S . -B build -G Ninja \
    -DCMAKE_PREFIX_PATH=<topo-install-prefix> \
    -DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake
cmake --build build
```

The zero-LLVM subset (analysis, runner, transpile, lsp, init, plus the
`topo-lang-rust-tests` unit test) builds by default. To opt into the
LLVM-coupled toolchain layer (RustDriver, `topo-debug-rust`,
`topo-profile` fixture, cargo-built `topo-extract-rust`, and the plugin
aggregator that links them), configure with
`-DTOPO_LANG_RUST_ENABLE_LLVM=ON`.

The Rust runtime crate and runtime-macros are built separately via Cargo.

## Tests

```bash
ctest --test-dir build --output-on-failure
```

## Downstream usage

```cmake
find_package(topo-lang-rust CONFIG REQUIRED)
target_link_libraries(<target> PRIVATE topo::lang-rust::TopoRustAnalysis)
```

The exported library targets are:

- `topo::lang-rust::TopoRustAnalysis`
- `topo::lang-rust::TopoRustCheck`
- `topo::lang-rust::TopoRustInit`
- `topo::lang-rust::TopoRustLSP`
- `topo::lang-rust::TopoRustTranspile`

When configured with `-DTOPO_LANG_RUST_ENABLE_LLVM=ON`, the plugin
aggregator `topo::lang-rust::TopoRustPlugin` is also available.
