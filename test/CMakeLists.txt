# Lang type tests (header-only, no LLVM)
add_executable(topo-lang-rust-tests
    unit/RustStubGeneratorTest.cpp
)
# TopoRustAnalysis PUBLIC-propagates TopoRustLSP → topo::core::TopoLSPBridge
# → topo::core::TopoPlatform, so explicit TopoPlatform is redundant.
target_link_libraries(topo-lang-rust-tests PRIVATE
    TopoRustAnalysis
    GTest::gtest_main
)
target_include_directories(topo-lang-rust-tests PRIVATE
    ${PROJECT_SOURCE_DIR}/topo-check   # for analysis/stub/RustStubGenerator.h
)
topo_set_compiler_flags(topo-lang-rust-tests)
# MinGW on GHA: cached upstream topo-install may have been built against a
# different libstdc++ ABI than the runner's current C:\mingw64\bin\, which
# manifests as STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) the moment
# gtest_discover_tests launches the binary. Static-link the C++ runtime
# so the test exe is hermetic.
if(MINGW)
    target_link_options(topo-lang-rust-tests PRIVATE
        -static -static-libgcc -static-libstdc++)
endif()
gtest_discover_tests(topo-lang-rust-tests DISCOVERY_TIMEOUT 30 PROPERTIES LABELS "toolchain;lang-rust" TIMEOUT 60)

# Extractor fidelity tests + check E2E tests depend on the cargo-built
# extractor binary and the plugin aggregator (TopoRustPlugin), both of
# which are gated behind TOPO_LANG_RUST_ENABLE_LLVM. See
# .aidesk/live/40-issue/split-topo-lang-rust-llvm-scope-deferred.md.
if(TOPO_LANG_RUST_ENABLE_LLVM)
    add_executable(topo-lang-rust-extractor-fidelity-tests
        unit/RustExtractorFidelityTest.cpp
    )
    target_link_libraries(topo-lang-rust-extractor-fidelity-tests PRIVATE
        nlohmann_json::nlohmann_json
        GTest::gtest_main
    )
    set(_topo_extract_rust_binary
        "${PROJECT_SOURCE_DIR}/topo-check/extractor/target/release/topo-extract-rust${CMAKE_EXECUTABLE_SUFFIX}")
    target_compile_definitions(topo-lang-rust-extractor-fidelity-tests PRIVATE
        RUST_FIDELITY_FIXTURES_DIR="${CMAKE_CURRENT_SOURCE_DIR}/fixtures/extractor_fidelity"
        RUST_EXTRACTOR_BINARY="${_topo_extract_rust_binary}"
    )
    add_dependencies(topo-lang-rust-extractor-fidelity-tests topo-extract-rust)
    topo_set_compiler_flags(topo-lang-rust-extractor-fidelity-tests)
    gtest_discover_tests(topo-lang-rust-extractor-fidelity-tests DISCOVERY_TIMEOUT 30
        PROPERTIES LABELS "toolchain;lang-rust;extractor-fidelity" TIMEOUT 60
        ENVIRONMENT "TOPO_EXTRACT_ROOT=${PROJECT_SOURCE_DIR}")

    # check-e2e suite needs TopoCheckRunner (owned by topo-cli) to drive the
    # full check pipeline. Pull topo-cli in quietly; if its check-runner
    # imported target is unavailable, skip the suite.
    find_package(topo-cli CONFIG QUIET)
    if(TARGET topo::cli::TopoCheckRunner)
        add_executable(topo-lang-rust-check-tests
            e2e/TestMain.cpp
            e2e/RustCompletenessTest.cpp
            e2e/RustContainmentTest.cpp
            e2e/RustPurityTest.cpp
            e2e/RustVisibilityTest.cpp
            e2e/RustStageIsolationTest.cpp
        )
        target_link_libraries(topo-lang-rust-check-tests PRIVATE
            topo::cli::TopoCheckRunner
            TopoRustPlugin
            GTest::gtest
        )
        target_compile_definitions(topo-lang-rust-check-tests PRIVATE
            TOPO_TEST_FIXTURES_DIR="${CMAKE_CURRENT_SOURCE_DIR}/fixtures/check"
        )
        topo_set_compiler_flags(topo-lang-rust-check-tests)
        gtest_discover_tests(topo-lang-rust-check-tests DISCOVERY_TIMEOUT 30 PROPERTIES LABELS "toolchain;checker;lang-rust" TIMEOUT 60)
    else()
        message(STATUS "topo-lang-rust: check-e2e tests skipped (topo-cli/TopoCheckRunner not on CMAKE_PREFIX_PATH)")
    endif()
endif()
