# Rust span-emitter fixture.
#
# Built with rustc directly (not cargo) so the binary lands at a known
# build-tree path. Links the static libtopo-observe archive plus the
# host C++ standard library (libtopo-observe is C++ internally; std::
# symbols must be resolved at link time).
#
# Platform notes:
#   macOS:  libc++   (-l c++)
#   Linux:  libstdc++ (-l stdc++)

find_program(RUSTC_EXECUTABLE rustc)
if(NOT RUSTC_EXECUTABLE)
    message(WARNING
        "topo-lang-rust/topo-profile: rustc not found on PATH; "
        "Rust e2e tests will be disabled.")
    return()
endif()

# The static topo-observe archive sits in topo-llvm/runtime/'s build dir.
set(_OBSERVE_LIB_DIR "$<TARGET_FILE_DIR:topo-observe>")

if(APPLE)
    set(_CXX_STDLIB_FLAG "-l" "dylib=c++")
elseif(UNIX)
    set(_CXX_STDLIB_FLAG "-l" "dylib=stdc++")
else()
    set(_CXX_STDLIB_FLAG "")
endif()

set(_SPANS_RS_SRC "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/spans_demo.rs")
set(_SPANS_RS_OUT "${CMAKE_CURRENT_BINARY_DIR}/spans_demo_rs_bin")

add_custom_command(
    OUTPUT "${_SPANS_RS_OUT}"
    COMMAND ${RUSTC_EXECUTABLE}
            -C opt-level=0
            -C debuginfo=2
            -L "native=${_OBSERVE_LIB_DIR}"
            -l "static=topo-observe"
            ${_CXX_STDLIB_FLAG}
            -o "${_SPANS_RS_OUT}"
            "${_SPANS_RS_SRC}"
    DEPENDS "${_SPANS_RS_SRC}" topo-observe
    COMMENT "Building spans_demo_rs_bin (rustc -g + libtopo-observe)"
    VERBATIM
)

add_custom_target(spans_demo_rs ALL
    DEPENDS "${_SPANS_RS_OUT}"
)

set_property(TARGET spans_demo_rs
    PROPERTY TOPO_FIXTURE_PATH "${_SPANS_RS_OUT}")
