#ifndef TOPO_CHECK_RUSTLSPSYMBOLEXTRACTOR_H
#define TOPO_CHECK_RUSTLSPSYMBOLEXTRACTOR_H

#include "topo/Check/SymbolExtractor.h"

// Forward declaration
namespace topo::lsp { class RustAnalyzerBridge; }

namespace topo::check {

/// LSP-based Rust symbol extractor using rust-analyzer semantic tokens and hover.
///
/// Extracts symbols by:
/// 1. Opening the document for rust-analyzer analysis
/// 2. Requesting semantic tokens to find declarations/definitions
/// 3. Using hover to resolve qualified names and signatures
///
/// Falls back gracefully: if rust-analyzer returns empty tokens for a file,
/// the result is simply empty.
class RustLSPSymbolExtractor : public SymbolExtractor {
public:
    explicit RustLSPSymbolExtractor(lsp::RustAnalyzerBridge& bridge);

    std::vector<HostSymbol> extractSymbols(const std::string& filePath) override;

private:
    /// Parse return type from a hover signature like "fn ns::func(args) -> RetType".
    static std::string parseReturnType(const std::string& hover);

    /// Parse parameter types from a hover signature.
    /// Rust params are "name: Type" -- extracts the Type part. Skips self/&self/&mut self.
    static std::vector<std::string> parseParamTypes(const std::string& hover);

    /// Detect enclosing type from a qualified name like "MyStruct::method".
    static std::string detectEnclosingType(const std::string& qualifiedName);

    lsp::RustAnalyzerBridge& bridge_;
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTLSPSYMBOLEXTRACTOR_H
