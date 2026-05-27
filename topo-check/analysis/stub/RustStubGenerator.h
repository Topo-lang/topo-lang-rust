#ifndef TOPO_CHECK_RUSTSTUBGENERATOR_H
#define TOPO_CHECK_RUSTSTUBGENERATOR_H

#include "topo/Check/StubGenerator.h"

#include <string>

namespace topo::check {

/// Rust implementation of StubGenerator.
/// Finds function definitions in Rust source files by matching `fn func_name(`,
/// then replaces the body using brace-balancing.
class RustStubGenerator : public StubGenerator {
public:
    StubResult stubFunction(const std::string& filePath, const std::string& funcName) override;

    bool restoreFile(const std::string& filePath, const StubResult& result) override;

    /// Find the position of a function body in Rust source text.
    /// Returns the index of the opening '{' of the function body,
    /// or std::string::npos if not found.
    static size_t findFunctionBodyStart(const std::string& source, const std::string& funcName);

    /// Find the matching closing '}' for a given opening '{' position.
    static size_t findMatchingBrace(const std::string& source, size_t openPos);

    /// Determine if a function returns unit (no return type or -> ()).
    static bool isUnitReturn(const std::string& source, size_t bodyStart);
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTSTUBGENERATOR_H
