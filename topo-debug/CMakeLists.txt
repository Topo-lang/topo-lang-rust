# topo-debug-rust — Extract adapter for Rust binaries.
#
# Reuses the TopoDebugAdapter library defined by topo-lang-cpp/topo-debug/.
# adapter.cpp's logic is host-language-agnostic; the program name in stderr is
# derived from argv[0] at runtime — so the same object code ships as both
# topo-debug-cpp and topo-debug-rust without per-language source duplication.
#
# Needs the shared adapter archive topo::lang-cpp::TopoDebugAdapter. In the
# unified meta build it is an in-tree target (topo-lang-cpp is configured
# first); in a standalone topo-lang-rust build it is imported via
# find_package(topo-lang-cpp) (see ../CMakeLists.txt). topo-lang-cpp only
# defines it when liblldb is present, so its absence is a clean self-skip.
if(NOT TOPO_ENABLE_LLVM OR NOT TARGET topo::lang-cpp::TopoDebugAdapter)
    return()
endif()

# adapter.cpp's main() lives INSIDE the STATIC archive with no rust-side
# referrer, so the whole archive must be force-linked. topo-debug-cpp uses
# $<TARGET_OBJECTS:TopoDebugAdapter> in-tree, but that generator expression is
# unavailable for an IMPORTED archive across the install boundary — so
# whole-archive of the archive file is the single code path that works both
# in-tree and cross-repo. adapter_main.cpp is an empty TU that only gives the
# executable a primary source (linker language); main comes from the archive.
add_executable(topo-debug-rust adapter_main.cpp)

# Link the target for its transitive interface (topo::core::TopoDebug, the
# absolute liblldb path, nlohmann_json + include dirs); force-load the archive
# FILE so adapter.cpp's objects (incl. main) are pulled in whole.
target_link_libraries(topo-debug-rust PRIVATE topo::lang-cpp::TopoDebugAdapter)
if(APPLE)
    target_link_options(topo-debug-rust PRIVATE
        "LINKER:-force_load,$<TARGET_FILE:topo::lang-cpp::TopoDebugAdapter>")
elseif(WIN32)
    # MSVC/clang-cl: /WHOLEARCHIVE is a link.exe flag passed straight to the
    # linker driver (no -Wl, wrapping), so it is given directly rather than via
    # the GNU-style LINKER: prefix.
    target_link_options(topo-debug-rust PRIVATE
        "/WHOLEARCHIVE:$<TARGET_FILE:topo::lang-cpp::TopoDebugAdapter>")
else() # ELF
    target_link_options(topo-debug-rust PRIVATE
        "LINKER:--whole-archive"
        "$<TARGET_FILE:topo::lang-cpp::TopoDebugAdapter>"
        "LINKER:--no-whole-archive")
endif()
topo_set_compiler_flags(topo-debug-rust)

# Mirror topo-debug-cpp's RPATH so the binary finds liblldb at runtime.
# Prefer the stable unversioned Homebrew opt symlink over the versioned Cellar
# keg LLVMConfig may resolve to (…/Cellar/llvm/22.1.7 → …/opt/llvm), so the
# dev-fallback rpaths survive `brew upgrade llvm` patch bumps.
set(_lldb_lib_dir "${LLVM_INSTALL_PREFIX}/lib")
if(_lldb_lib_dir MATCHES "^(.+)/Cellar/(llvm[^/]*)/[^/]+/lib$")
    set(_lldb_lib_dir_stable "${CMAKE_MATCH_1}/opt/${CMAKE_MATCH_2}/lib")
    if(EXISTS "${_lldb_lib_dir_stable}")
        set(_lldb_lib_dir "${_lldb_lib_dir_stable}")
    endif()
endif()
# BUILD_RPATH covers dev runs from the build tree; INSTALL_RPATH keeps the
# build-host dir as the dev fallback. topo_relocate_llvm_rpath below prepends
# the relocation rpath (@loader_path/../lib / $ORIGIN/../lib) ahead of it AND
# — on macOS — rewrites the absolute liblldb.<ver> load command to
# @rpath/<name> on the installed copy. Without that rewrite the relocation
# rpath is dead weight: dyld consults LC_RPATH only for @rpath/ load commands.
set_target_properties(topo-debug-rust PROPERTIES
    BUILD_RPATH "${_lldb_lib_dir}"
    INSTALL_RPATH "${_lldb_lib_dir}"
)

install(TARGETS topo-debug-rust RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})

# Relocatable install: see the rpath note above.
topo_relocate_llvm_rpath(topo-debug-rust liblldb)

# Test fixtures (tiny_vector_rs / tiny_multi_rs / project_simple / project_multi)
# are built here and consumed by the topo-core/test rust adapter e2e block
# ($<TARGET_FILE:topo-debug-rust>, get_target_property(... TOPO_FIXTURE_PATH)).
# Mirrors topo-lang-cpp/topo-debug/CMakeLists.txt. Without this the fixtures
# never build, so every e2e.topo-debug-rust.* self-skips on `TARGET
# tiny_vector_rs` — i.e. the Rust adapter has no runtime coverage at all. Gated
# off in standalone mode (only useful inside the monorepo e2e).
if(TOPO_LANG_RUST_BUILD_TEST_FIXTURES)
    add_subdirectory(test)
endif()
