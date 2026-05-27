#ifndef TOPO_CHECK_RUSTIMPORTEXTRACTOR_H
#define TOPO_CHECK_RUSTIMPORTEXTRACTOR_H

#include "topo/Check/ImportExtractor.h"

#include <string>
#include <vector>

namespace topo::check {

/// Regex-based Rust import extractor (L1 safety net).
/// Parses `use` declarations and `extern crate` statements.
class RustImportExtractor : public ImportExtractor {
public:
    std::vector<HostImport> extractImports(const std::string& filePath) override;
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTIMPORTEXTRACTOR_H
