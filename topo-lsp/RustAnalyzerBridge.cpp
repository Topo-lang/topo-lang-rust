#include "RustAnalyzerBridge.h"

#include "topo/Platform/Platform.h"

#include <fstream>
#include <iostream>
#include <regex>

namespace topo::lsp {

RustAnalyzerBridge::RustAnalyzerBridge() : LSPBridge("[topo-lsp]") {}

bool RustAnalyzerBridge::start(const std::string& rootUri) {
    return start(std::string{}, rootUri);
}

bool RustAnalyzerBridge::start(const std::string& rustAnalyzerPath, const std::string& rootUri) {
    namespace plat = topo::platform;

    std::string exe = rustAnalyzerPath;
    if (exe.empty()) {
        exe = "rust-analyzer" + std::string(plat::ExeSuffix);
    }

    std::vector<std::string> args;
    if (!startProcess(exe, args, rootUri))
        return false;

    parseSemanticTokenLegend();
    return true;
}

std::optional<SymbolResult> RustAnalyzerBridge::findDefinition(const std::string& qualifiedName,
                                                               const std::vector<std::string>& /*sourceFiles*/) {
    if (!isAvailable()) return std::nullopt;
    return queryWorkspaceSymbol(qualifiedName);
}

std::vector<SymbolResult> RustAnalyzerBridge::findReferences(const std::string& qualifiedName,
                                                             const std::vector<std::string>& /*sourceFiles*/) {
    if (!isAvailable()) return {};

    auto defn = queryWorkspaceSymbol(qualifiedName);
    if (!defn) return {};

    json params = {{"textDocument", {{"uri", pathToUri(defn->file)}}},
                   {"position", {{"line", defn->line}, {"character", defn->column}}},
                   {"context", {{"includeDeclaration", true}}}};

    auto response = sendRequest("textDocument/references", params);
    if (!response || !response->is_array()) return {};

    std::vector<SymbolResult> results;
    for (const auto& loc : *response) {
        SymbolResult r;
        r.file = uriToPath(loc["uri"].get<std::string>());
        r.line = loc["range"]["start"]["line"].get<int>();
        r.column = loc["range"]["start"]["character"].get<int>();
        results.push_back(std::move(r));
    }
    return results;
}

std::optional<std::string> RustAnalyzerBridge::getHoverInfo(const std::string& qualifiedName,
                                                            const std::vector<std::string>& /*sourceFiles*/) {
    if (!isAvailable()) return std::nullopt;

    auto defn = queryWorkspaceSymbol(qualifiedName);
    if (!defn) return std::nullopt;

    json params = {{"textDocument", {{"uri", pathToUri(defn->file)}}},
                   {"position", {{"line", defn->line}, {"character", defn->column}}}};

    auto response = sendRequest("textDocument/hover", params);
    if (!response || response->is_null()) return std::nullopt;

    if (response->contains("contents")) {
        const auto& contents = (*response)["contents"];
        if (contents.is_string()) {
            return contents.get<std::string>();
        }
        if (contents.is_object() && contents.contains("value")) {
            return contents["value"].get<std::string>();
        }
    }
    return std::nullopt;
}

std::optional<SymbolResult> RustAnalyzerBridge::findTypeDefinition(const std::string& typeName,
                                                                   const std::vector<std::string>& sourceFiles,
                                                                   const std::vector<std::string>& /*includeDirs*/) {
    // Prefer the live index when rust-analyzer is running.
    if (isAvailable()) {
        auto result = queryWorkspaceSymbol(typeName);
        if (result) return result;
    }

    // Fallback: scan .rs source files for a matching type definition.
    // Matches: struct Foo, enum Foo, trait Foo, type Foo =
    const std::regex pattern(R"((?:pub(?:\s*\([^)]*\))?\s+)?(?:struct|enum|trait|type)\s+)" + typeName + R"([\s<{;=])");

    for (const auto& filePath : sourceFiles) {
        if (filePath.size() < 3 || filePath.substr(filePath.size() - 3) != ".rs") continue;

        std::ifstream file(filePath);
        if (!file.is_open()) continue;

        std::string line;
        int lineNo = 0;
        while (std::getline(file, line)) {
            ++lineNo;
            if (std::regex_search(line, pattern)) {
                return SymbolResult{filePath, lineNo, 0};
            }
        }
    }

    return std::nullopt;
}

} // namespace topo::lsp
