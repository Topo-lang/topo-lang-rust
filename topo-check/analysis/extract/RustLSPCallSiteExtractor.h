#ifndef TOPO_CHECK_RUSTLSPCALLSITEEXTRACTOR_H
#define TOPO_CHECK_RUSTLSPCALLSITEEXTRACTOR_H

#include "topo/Check/ContainmentTypes.h"

#include <string>
#include <vector>

// Forward declaration
namespace topo::lsp { class RustAnalyzerBridge; }

namespace topo::check {

/// LSP-based Rust call site extractor using rust-analyzer semantic tokens and hover.
///
/// Extracts function/method call references by:
/// 1. Opening the document for rust-analyzer analysis
/// 2. Requesting semantic tokens to find call references (non-declaration tokens)
/// 3. Using hover to resolve qualified callee names
/// 4. Classifying each callee via RustUnsafeCatalog and CapabilityCatalog
///
/// This extractor handles what regex cannot: qualified name resolution
/// across modules, trait method dispatch, and macro-expanded code.
/// Language escape constructs (unsafe blocks, asm!, extern ABI, etc.) are
/// detected by the L1 regex-based RustCallSiteExtractor as a supplement.
class RustLSPCallSiteExtractor {
public:
    explicit RustLSPCallSiteExtractor(lsp::RustAnalyzerBridge& bridge);

    /// Extract call sites from a single source file.
    /// Returns only function/method call references with resolved qualified names.
    std::vector<DetectedCallSite> extractCallSites(const std::string& filePath);

private:
    lsp::RustAnalyzerBridge& bridge_;
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTLSPCALLSITEEXTRACTOR_H
