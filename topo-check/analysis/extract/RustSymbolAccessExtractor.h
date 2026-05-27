#ifndef TOPO_CHECK_RUSTSYMBOLACCESSEXTRACTOR_H
#define TOPO_CHECK_RUSTSYMBOLACCESSEXTRACTOR_H

#include "topo/Check/SymbolAccessExtractor.h"

#include <string>
#include <vector>

namespace topo::check {

/// L1 regex-based Rust symbol access extractor used by PurityCheck.
///
/// Two-pass strategy:
///   1. Scan for module-level statics (`static X`, `static mut X`,
///      `const X`) and `thread_local! { static X: ... }` declarations.
///      Items inside `impl` and `fn` scopes are excluded.
///   2. Inside function bodies, emit SymbolAccess{isWrite=true} for
///      writes to detected globals: simple assignment, compound
///      assignment, deref-store via `*X = ...`, postfix/prefix `++`/`--`
///      (rare in Rust but kept for symmetry with C++).
///
/// Reads are deferred — writes are the load-bearing parallel-purity signal.
class RustSymbolAccessExtractor : public SymbolAccessExtractor {
public:
    std::vector<SymbolAccess> extractSymbolAccesses(const std::string& filePath) override;
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTSYMBOLACCESSEXTRACTOR_H
