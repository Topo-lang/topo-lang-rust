#ifndef TOPO_CHECK_RUSTCALLSITEEXTRACTOR_H
#define TOPO_CHECK_RUSTCALLSITEEXTRACTOR_H

#include "topo/Check/CallSiteExtractor.h"

#include <string>
#include <vector>

namespace topo::check {

/// L1 regex-based Rust call site extractor.
///
/// Detects language escape constructs and dangerous patterns that do not
/// require semantic analysis:
///   - unsafe blocks (including Allman-style brace placement)
///   - unsafe fn declarations
///   - extern "ABI" blocks (any ABI string, not just "C")
///   - asm!/global_asm!/include!/include_bytes!/include_str! macros
///   - static mut declarations
///   - raw pointer dereference patterns
///   - env!() compile-time environment access
///
/// This extractor supplements the LSP-based RustLSPCallSiteExtractor which
/// handles qualified name resolution and call graph analysis.
class RustCallSiteExtractor : public CallSiteExtractor {
public:
    std::vector<DetectedCallSite> extractCallSites(const std::string& filePath) override;
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTCALLSITEEXTRACTOR_H
