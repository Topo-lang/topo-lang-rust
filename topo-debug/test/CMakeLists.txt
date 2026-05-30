# Rust fixture for the topo-debug-rust adapter.
#
# Built with rustc directly (not cargo) so the resulting binary lands at a
# known build-tree path the e2e tests can reference via $<TARGET_FILE:...>.
# -C opt-level=0 -C debuginfo=2 keeps the local `vec` array intact and the
# DWARF line table accurate — mirrors the -O0 -g policy of tiny_vector.cpp.

find_program(RUSTC_EXECUTABLE rustc)
if(NOT RUSTC_EXECUTABLE)
    message(WARNING
        "topo-lang-rust/topo-debug: rustc not found on PATH; "
        "Rust e2e tests will be disabled.")
    return()
endif()

set(_TINY_VECTOR_RS_SRC "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/tiny_vector.rs")
# Output filename differs from the CMake target name to avoid the Ninja
# "phony target names itself as an input" diagnostic (the target name and
# the OUTPUT path would otherwise resolve to the same ninja node).
set(_TINY_VECTOR_RS_OUT "${CMAKE_CURRENT_BINARY_DIR}/tiny_vector_rs_bin")

add_custom_command(
    OUTPUT "${_TINY_VECTOR_RS_OUT}"
    COMMAND ${RUSTC_EXECUTABLE}
            -C opt-level=0
            -C debuginfo=2
            -o "${_TINY_VECTOR_RS_OUT}"
            "${_TINY_VECTOR_RS_SRC}"
    DEPENDS "${_TINY_VECTOR_RS_SRC}"
    COMMENT "Building tiny_vector_rs_bin (rustc -g)"
    VERBATIM
)

add_custom_target(tiny_vector_rs ALL
    DEPENDS "${_TINY_VECTOR_RS_OUT}"
)

# Custom targets have no TARGET_FILE — expose the real path via a property
# so the topo-core e2e block can read it with get_target_property(...
# TOPO_FIXTURE_PATH).
set_property(TARGET tiny_vector_rs
    PROPERTY TOPO_FIXTURE_PATH "${_TINY_VECTOR_RS_OUT}")

# Rust multi-variable fixture (parallel to
# topo-lang-cpp/topo-debug/test/fixtures/tiny_multi.cpp). The shared
# adapter source loops FindVariable per `--var` entry; this fixture
# proves the multi-var protocol works against rustc-emitted DWARF too.
set(_TINY_MULTI_RS_SRC "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/tiny_multi.rs")
set(_TINY_MULTI_RS_OUT "${CMAKE_CURRENT_BINARY_DIR}/tiny_multi_rs_bin")

add_custom_command(
    OUTPUT "${_TINY_MULTI_RS_OUT}"
    COMMAND ${RUSTC_EXECUTABLE}
            -C opt-level=0
            -C debuginfo=2
            -o "${_TINY_MULTI_RS_OUT}"
            "${_TINY_MULTI_RS_SRC}"
    DEPENDS "${_TINY_MULTI_RS_SRC}"
    COMMENT "Building tiny_multi_rs_bin (rustc -g)"
    VERBATIM
)

add_custom_target(tiny_multi_rs ALL
    DEPENDS "${_TINY_MULTI_RS_OUT}"
)

set_property(TARGET tiny_multi_rs
    PROPERTY TOPO_FIXTURE_PATH "${_TINY_MULTI_RS_OUT}")

# Project-level Rust fixtures — full topo-build flow (rust counterpart of
# topo-lang-cpp/topo-debug/test/fixtures/project_simple|project_multi).
#
# Unlike tiny_vector_rs / tiny_multi_rs (built directly with `rustc -g`),
# these fixtures exercise the declaration-driven path end-to-end: `topo
# build` parses main.topo, emits main.topo-dbg.json (the view registry),
# and produces a debuggable binary via the rust backend pipeline. Each
# fixture is copied to the build tree at configure time so `topo build`
# writes its outputs there, not into the source checkout. The Cargo.toml
# template gets the path to the topo runtime crate substituted in so the
# `--features topo/all-runtime` flag the rust driver hardcodes resolves
# against a real dep (the driver passes that feature unconditionally, so
# the fixture must declare a real `topo` dep for cargo to accept it).
set(TOPO_RUST_RUNTIME_DIR "${PROJECT_SOURCE_DIR}/runtime"
    CACHE INTERNAL "Path to the topo rust runtime crate (for fixture Cargo.toml templates)")

set(_PROJECT_SIMPLE_RS_SRC ${CMAKE_CURRENT_SOURCE_DIR}/fixtures/project_simple)
set(TOPO_DEBUG_RUST_PROJECT_SIMPLE_DIR
    ${CMAKE_CURRENT_BINARY_DIR}/fixtures/project_simple
    CACHE INTERNAL "Project-simple Rust fixture build dir")
file(MAKE_DIRECTORY ${TOPO_DEBUG_RUST_PROJECT_SIMPLE_DIR})
file(MAKE_DIRECTORY ${TOPO_DEBUG_RUST_PROJECT_SIMPLE_DIR}/src)
configure_file(${_PROJECT_SIMPLE_RS_SRC}/Topo.toml
    ${TOPO_DEBUG_RUST_PROJECT_SIMPLE_DIR}/Topo.toml COPYONLY)
configure_file(${_PROJECT_SIMPLE_RS_SRC}/main.topo
    ${TOPO_DEBUG_RUST_PROJECT_SIMPLE_DIR}/main.topo COPYONLY)
configure_file(${_PROJECT_SIMPLE_RS_SRC}/Cargo.toml.in
    ${TOPO_DEBUG_RUST_PROJECT_SIMPLE_DIR}/Cargo.toml @ONLY)
configure_file(${_PROJECT_SIMPLE_RS_SRC}/src/lib.rs
    ${TOPO_DEBUG_RUST_PROJECT_SIMPLE_DIR}/src/lib.rs COPYONLY)
configure_file(${_PROJECT_SIMPLE_RS_SRC}/src/main.rs
    ${TOPO_DEBUG_RUST_PROJECT_SIMPLE_DIR}/src/main.rs COPYONLY)

set(_PROJECT_MULTI_RS_SRC ${CMAKE_CURRENT_SOURCE_DIR}/fixtures/project_multi)
set(TOPO_DEBUG_RUST_PROJECT_MULTI_DIR
    ${CMAKE_CURRENT_BINARY_DIR}/fixtures/project_multi
    CACHE INTERNAL "Project-multi Rust fixture build dir")
file(MAKE_DIRECTORY ${TOPO_DEBUG_RUST_PROJECT_MULTI_DIR})
file(MAKE_DIRECTORY ${TOPO_DEBUG_RUST_PROJECT_MULTI_DIR}/src)
configure_file(${_PROJECT_MULTI_RS_SRC}/Topo.toml
    ${TOPO_DEBUG_RUST_PROJECT_MULTI_DIR}/Topo.toml COPYONLY)
configure_file(${_PROJECT_MULTI_RS_SRC}/main.topo
    ${TOPO_DEBUG_RUST_PROJECT_MULTI_DIR}/main.topo COPYONLY)
configure_file(${_PROJECT_MULTI_RS_SRC}/Cargo.toml.in
    ${TOPO_DEBUG_RUST_PROJECT_MULTI_DIR}/Cargo.toml @ONLY)
configure_file(${_PROJECT_MULTI_RS_SRC}/src/lib.rs
    ${TOPO_DEBUG_RUST_PROJECT_MULTI_DIR}/src/lib.rs COPYONLY)
configure_file(${_PROJECT_MULTI_RS_SRC}/src/main.rs
    ${TOPO_DEBUG_RUST_PROJECT_MULTI_DIR}/src/main.rs COPYONLY)
