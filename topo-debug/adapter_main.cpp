// topo-debug-rust entry translation unit.
//
// Intentionally empty: topo-debug-rust is the SAME program as topo-debug-cpp.
// Its main() and all logic live in adapter.cpp, compiled once into the shared
// STATIC archive topo::lang-cpp::TopoDebugAdapter and pulled in here via
// whole-archive linking (see CMakeLists.txt). The program name shown in stderr
// is derived from argv[0] at runtime, so the same object code serves both the
// C++ and Rust debug adapters without per-language source duplication.
//
// This TU exists only to give the executable a primary source so CMake can
// determine the linker language; it deliberately defines no symbols.
