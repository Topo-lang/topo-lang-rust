#ifndef TOPO_LSP_RUSTANALYZERBRIDGE_H
#define TOPO_LSP_RUSTANALYZERBRIDGE_H

#include "topo/LSP/LSPBridge.h"

namespace topo::lsp {

class RustAnalyzerBridge : public LSPBridge {
public:
    RustAnalyzerBridge();

    bool start(const std::string& rootUri) override;
    std::string displayName() const override { return "Rust"; }

    bool start(const std::string& rustAnalyzerPath, const std::string& rootUri);

    // Query implementations
    std::optional<SymbolResult> findDefinition(const std::string& qualifiedName,
                                               const std::vector<std::string>& sourceFiles) override;

    std::vector<SymbolResult> findReferences(const std::string& qualifiedName,
                                             const std::vector<std::string>& sourceFiles) override;

    std::optional<std::string> getHoverInfo(const std::string& qualifiedName,
                                            const std::vector<std::string>& sourceFiles) override;

    /// Find host-language type definition for a named type.
    /// Queries rust-analyzer workspace index first; falls back to scanning
    /// sourceFiles (.rs) for struct/enum/trait/type definitions matching
    /// typeName.
    std::optional<SymbolResult> findTypeDefinition(const std::string& typeName,
                                                   const std::vector<std::string>& sourceFiles,
                                                   const std::vector<std::string>& includeDirs) override;

    std::string languageId() const override { return "rust"; }
};

} // namespace topo::lsp

#endif // TOPO_LSP_RUSTANALYZERBRIDGE_H
