#ifndef TOPO_CHECK_RUSTUNSAFECATALOG_H
#define TOPO_CHECK_RUSTUNSAFECATALOG_H

#include "topo/Check/CapabilityCatalog.h"
#include <string>

namespace topo::check {

/// Rust unsafe behavior catalog.
/// Classifies Rust patterns by unsafe level.
/// Level 1 (System): std::fs, std::io, std::net, std::process
/// Level 2 (Dep): third-party crates (non std::*)
/// Level 3 (Input): web framework crates (actix, axum, etc.)
/// Level 4 (Escape): transmute, raw pointers, libc FFI
class RustUnsafeCatalog {
public:
    /// Classify a call site pattern (function name or qualified call).
    static UnsafeLevel classifyCall(const std::string& pattern);

    /// Classify an import path (use/extern crate).
    static UnsafeLevel classifyImport(const std::string& path);
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTUNSAFECATALOG_H
