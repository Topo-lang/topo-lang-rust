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
gtest_discover_tests(topo-lang-rust-tests DISCOVERY_TIMEOUT 30 PROPERTIES LABELS "toolchain\;lang-rust" TIMEOUT 60)

# Extractor fidelity tests + check E2E tests stay behind
# TOPO_LANG_RUST_ENABLE_LLVM: fidelity consumes the cargo-built extractor
# binary (only built in that scope), and check-e2e needs topo-cli on the
# prefix, which only the verify-llvm lane provides. TopoRustPlugin itself
# is unconditional (zero-LLVM closure).
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
        PROPERTIES LABELS "toolchain\;lang-rust\;extractor-fidelity" TIMEOUT 60
        ENVIRONMENT "TOPO_EXTRACT_ROOT=${PROJECT_SOURCE_DIR}")

    # Checker-e2e suite — extracted to its own dir so an embedding
    # super-build can re-add it after topo-cli configures; the file
    # self-gates on TopoCheckRunner availability (same skip as before).
    add_subdirectory(check-e2e)
endif()
