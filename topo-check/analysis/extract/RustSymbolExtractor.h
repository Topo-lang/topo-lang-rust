#ifndef TOPO_CHECK_RUSTSYMBOLEXTRACTOR_H
#define TOPO_CHECK_RUSTSYMBOLEXTRACTOR_H

#include "topo/Check/SymbolExtractor.h"

#include <string>
#include <vector>

namespace topo::check {

/// Regex-based Rust symbol extractor (L1 safety net).
/// Uses line scanning with brace-depth tracking for mod/struct/impl scopes.
/// Provides basic symbol extraction when rust-analyzer is unavailable.
class RustSymbolExtractor : public SymbolExtractor {
public:
    std::vector<HostSymbol> extractSymbols(const std::string& filePath) override;
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTSYMBOLEXTRACTOR_H
