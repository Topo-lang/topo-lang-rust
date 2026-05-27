#ifndef TOPO_CHECK_RUSTLSPIMPORTEXTRACTOR_H
#define TOPO_CHECK_RUSTLSPIMPORTEXTRACTOR_H

#include "topo/Check/ContainmentTypes.h"

#include <string>
#include <vector>

namespace topo::check {

/// Clean line-based Rust import extractor.
///
/// Rust imports (`use`, `extern crate`) are deterministic syntax -- no need for LSP.
/// This extractor does the same job as RustImportExtractor but with
/// a cleaner, more maintainable implementation:
///   - Direct string matching instead of regex
///   - Same block-comment state machine
///   - RustUnsafeCatalog classification for each import
///
/// Functionally equivalent to RustImportExtractor.
class RustLSPImportExtractor {
public:
    /// Extract all import paths from a single file.
    std::vector<HostImport> extractImports(const std::string& filePath);

    /// Extract imports from multiple files.
    std::vector<HostImport> extractAll(const std::vector<std::string>& files);
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTLSPIMPORTEXTRACTOR_H
