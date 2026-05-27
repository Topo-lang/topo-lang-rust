cmake_minimum_required(VERSION 3.20)

project(topo-lang-rust
    VERSION 0.1.0
    DESCRIPTION "Topo Rust language support — runtime crates, check, build/transpile, LSP bridge, plugin"
    HOMEPAGE_URL "https://github.com/topo-lang/topo-lang-rust"
    LANGUAGES C CXX
)

# topo-lang-rust: Rust language support (FFI crates + compilation driver)
# Provides safe Rust wrappers around C ABI runtime libraries and the Rust
# compilation driver (RustDriver). C ABI runtime libraries live in
# topo-llvm/runtime/.

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
include(TopoLangRustCompilerFlags)

# ── Upstream Topo packages ────────────────────────────
if(NOT TARGET topo::core::TopoBasic)
    find_package(topo-core CONFIG REQUIRED)
endif()
if(NOT TARGET topo::lang::TopoLang)
    find_package(topo-lang CONFIG REQUIRED)
endif()

# ── External dependencies ─────────────────────────────
find_package(nlohmann_json CONFIG REQUIRED)

# ── Optional LLVM-dependent toolchain layer ───────────
# topo-build/RustDriver, topo-debug, topo-profile fixtures, and the
# topo-check/extractor (cargo-built Rust extractor) couple back to the
# LLVM-side runtime libraries (libtopo-observe) and/or LLDB. Default OFF;
# consumers that need them set -DTOPO_LANG_RUST_ENABLE_LLVM=ON and arrange
# Clang/LLDB findable.
option(TOPO_LANG_RUST_ENABLE_LLVM
    "Enable LLVM-dependent components (RustDriver, debug adapter, profile fixture, cargo extractor build)"
    OFF)

if(TOPO_LANG_RUST_ENABLE_LLVM)
    find_package(LLVM CONFIG REQUIRED)
    message(STATUS "topo-lang-rust: LLVM ${LLVM_VERSION} enabled")
    # topo-debug-rust (currently deferred) needs topo::lang-cpp::TopoDebugAdapter.
    # Pull it in quietly so its targets become available when the package
    # is on CMAKE_PREFIX_PATH; absence is fine because topo-debug-rust
    # self-skips when the target isn't imported.
    if(NOT TARGET topo::lang-cpp::TopoCppPlugin)
        find_package(topo-lang-cpp CONFIG QUIET)
    endif()
    # Existing TOPO_ENABLE_LLVM-gated subdir CMakeLists rely on this name
    # to fire their content.
    set(TOPO_ENABLE_LLVM ON)
endif()

# Test-only fixture builds (tiny_vector_rs / project_simple_rs) consumed by
# monorepo e2e tests in topo-core/test. Default OFF in standalone mode.
option(TOPO_LANG_RUST_BUILD_TEST_FIXTURES
    "Build Rust debug/profile test fixture binaries (only useful inside monorepo e2e)"
    OFF)

# ── Subdirs (most are zero-LLVM; LLVM-gated ones self-skip) ───────────
# Order: analysis/runner first because other subdirs include their headers.
add_subdirectory(topo-check/analysis)
add_subdirectory(topo-check/runner)
add_subdirectory(topo-transpile)
add_subdirectory(topo-lsp)
add_subdirectory(topo-init)
# These self-gate on TOPO_ENABLE_LLVM (topo-debug/topo-profile) or live
# inside the LLVM gate here (topo-build/RustDriver couples to TopoBuildLib
# which only ships when topo-core has WITH_LANG):
if(TOPO_LANG_RUST_ENABLE_LLVM)
    add_subdirectory(topo-build)
endif()
add_subdirectory(topo-debug)
add_subdirectory(topo-profile)
# Plugin aggregator — depends on the above libs. Some PRIVATE includes
# point into LLVM-gated subdirs, so we gate it on TOPO_LANG_RUST_ENABLE_LLVM
# for the pilot (covering it requires the build/debug subdirs being live).
if(TOPO_LANG_RUST_ENABLE_LLVM)
    add_subdirectory(topo-lang)
endif()

# Build topo-extract-rust via cargo. Gated on the LLVM flag because the
# fidelity tests under test/ that consume this binary live in the
# extractor-fidelity test target, which itself participates in the
# LLVM-coupled toolchain layer.
if(TOPO_LANG_RUST_ENABLE_LLVM)
    set(_TOPO_EXTRACT_RUST_BIN
        "${CMAKE_CURRENT_SOURCE_DIR}/topo-check/extractor/target/release/topo-extract-rust"
    )
    add_custom_target(topo-extract-rust ALL
        COMMAND cargo build --release --manifest-path ${CMAKE_CURRENT_SOURCE_DIR}/topo-check/extractor/Cargo.toml
        BYPRODUCTS ${_TOPO_EXTRACT_RUST_BIN}
        COMMENT "Building topo-extract-rust"
    )
    install(PROGRAMS ${_TOPO_EXTRACT_RUST_BIN}
        DESTINATION ${CMAKE_INSTALL_BINDIR})
endif()

# ── Tests ─────────────────────────────────────────────
option(TOPO_LANG_RUST_BUILD_TESTS "Build topo-lang-rust tests" ON)
if(TOPO_LANG_RUST_BUILD_TESTS AND (PROJECT_IS_TOP_LEVEL OR TOPO_META_DRIVING_TESTS))
    enable_testing()
    include(FetchContent)
    FetchContent_Declare(googletest
        GIT_REPOSITORY https://github.com/google/googletest.git
        GIT_TAG v1.15.2)
    set(gtest_force_shared_crt ON CACHE BOOL "" FORCE)
    FetchContent_MakeAvailable(googletest)
    include(GoogleTest)
    add_subdirectory(test)
endif()

# ── Install / export ──────────────────────────────────
install(EXPORT topo-lang-rustTargets
    FILE topo-lang-rustTargets.cmake
    NAMESPACE topo::lang-rust::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/topo-lang-rust
)

configure_package_config_file(
    ${CMAKE_CURRENT_SOURCE_DIR}/cmake/topo-lang-rustConfig.cmake.in
    ${CMAKE_CURRENT_BINARY_DIR}/topo-lang-rustConfig.cmake
    INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/topo-lang-rust
)

write_basic_package_version_file(
    ${CMAKE_CURRENT_BINARY_DIR}/topo-lang-rustConfigVersion.cmake
    VERSION       ${PROJECT_VERSION}
    COMPATIBILITY SameMajorVersion
)

install(FILES
    ${CMAKE_CURRENT_BINARY_DIR}/topo-lang-rustConfig.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/topo-lang-rustConfigVersion.cmake
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/topo-lang-rust
)
