# Checker-e2e suite — self-gating and idempotent so it can be added from
# two places: inline by ../CMakeLists.txt (standalone configure; behavior
# unchanged), and re-added late by an embedding super-build after topo-cli
# configures, when the topo::cli::TopoCheckRunner alias becomes visible.
if(TARGET topo-lang-rust-check-tests)
    return()
endif()

# Defensive: TopoRustPlugin is unconditional in this package; the gate
# covers embedding configures that skip the plugin subtree.
if(NOT TARGET TopoRustPlugin)
    message(STATUS "topo-lang-rust: check-e2e tests skipped (TopoRustPlugin not available)")
    return()
endif()

# The suite needs TopoCheckRunner (owned by topo-cli) to drive the full
# check pipeline. Pull topo-cli in quietly; if its check-runner target is
# unavailable, skip the suite.
if(NOT TARGET topo::cli::TopoCheckRunner)
    find_package(topo-cli CONFIG QUIET)
endif()
if(NOT TARGET topo::cli::TopoCheckRunner)
    message(STATUS "topo-lang-rust: check-e2e tests skipped (topo-cli/TopoCheckRunner not on CMAKE_PREFIX_PATH)")
    return()
endif()

# Sources and fixtures stay under test/ — anchor paths on the parent dir
# so both add_subdirectory entry points resolve identically.
get_filename_component(_topo_rust_test_root ${CMAKE_CURRENT_SOURCE_DIR} DIRECTORY)

add_executable(topo-lang-rust-check-tests
    ${_topo_rust_test_root}/e2e/TestMain.cpp
    ${_topo_rust_test_root}/e2e/RustCompletenessTest.cpp
    ${_topo_rust_test_root}/e2e/RustContainmentTest.cpp
    ${_topo_rust_test_root}/e2e/RustPurityTest.cpp
    ${_topo_rust_test_root}/e2e/RustVisibilityTest.cpp
    ${_topo_rust_test_root}/e2e/RustStageIsolationTest.cpp
    ${_topo_rust_test_root}/e2e/RustShowcaseTest.cpp
)
target_link_libraries(topo-lang-rust-check-tests PRIVATE
    topo::cli::TopoCheckRunner
    TopoRustPlugin
    GTest::gtest
)
get_filename_component(_topo_rust_pkg_root ${_topo_rust_test_root} DIRECTORY)
target_compile_definitions(topo-lang-rust-check-tests PRIVATE
    TOPO_TEST_FIXTURES_DIR="${_topo_rust_test_root}/fixtures/check"
    TOPO_EXAMPLES_DIR="${_topo_rust_pkg_root}/examples"
)
topo_set_compiler_flags(topo-lang-rust-check-tests)
# TIMEOUT 180: every L2 case boots rust-analyzer + cargo metadata.
gtest_discover_tests(topo-lang-rust-check-tests DISCOVERY_TIMEOUT 30 PROPERTIES LABELS "toolchain\;checker\;lang-rust" TIMEOUT 180)
