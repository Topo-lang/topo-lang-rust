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

# Mirror topo-debug-cpp's RPATH so the binary finds liblldb at runtime: the
# build-host LLVM lib dir for dev runs, @loader_path/../lib for a packaged tree.
set_target_properties(topo-debug-rust PROPERTIES
    BUILD_RPATH "${LLVM_INSTALL_PREFIX}/lib"
    INSTALL_RPATH "@loader_path/../lib;${LLVM_INSTALL_PREFIX}/lib"
)

install(TARGETS topo-debug-rust RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
