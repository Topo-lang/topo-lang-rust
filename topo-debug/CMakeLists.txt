# topo-debug-rust — Extract adapter for Rust binaries.
#
# Reuses the TopoDebugAdapter library defined by topo-lang-cpp/topo-debug/.
# adapter.cpp's logic is host-language-agnostic; the program name in
# stderr is derived from argv[0] at runtime — so the same object code
# ships as both topo-debug-cpp and topo-debug-rust without per-language
# source duplication.
#
# Standalone-build deferred: monorepo used $<TARGET_OBJECTS:TopoDebugAdapter>
# from an OBJECT lib in the same project. With TopoDebugAdapter now exported
# as a cross-repo STATIC lib (topo::lang-cpp::TopoDebugAdapter), pulling
# adapter.cpp's main into a separate executable needs either an adapter.cpp
# refactor (extract main into a callable function) or whole-archive linking
# (-force_load on Darwin, --whole-archive on ELF). Defer to a follow-up; the
# core LLVM-scope unblock chain (RustDriver / Plugin / extractor) does not
# depend on topo-debug-rust.
return()
