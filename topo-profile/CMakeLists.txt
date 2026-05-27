# topo-lang-rust/topo-profile — Rust span-emitter
# fixture for the topo-profile demo.
#
# Mirrors topo-lang-cpp/topo-profile but binds libtopo-observe via rustc
# `-l static` rather than the cmake link interface. Gated on rustc
# availability (already required by topo-debug-rust).

if(NOT TOPO_ENABLE_LLVM)
    return()
endif()

# Test fixture only consumed by monorepo e2e tests in topo-core/test.
if(TOPO_LANG_RUST_BUILD_TEST_FIXTURES)
    add_subdirectory(test)
endif()
