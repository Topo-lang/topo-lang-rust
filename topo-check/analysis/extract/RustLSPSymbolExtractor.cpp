// RustLSPSymbolExtractor -- LSP-based Rust symbol extraction via rust-analyzer.
//
// Uses semantic tokens to find declaration/definition tokens, then
// resolves each via hover to get qualified names and signatures.

#include "RustLSPSymbolExtractor.h"
#include "RustLSPUtils.h"
#include "RustAnalyzerBridge.h"

#include <cctype>
#include <cstring>
#include <string>
#include <vector>

namespace topo::check {

RustLSPSymbolExtractor::RustLSPSymbolExtractor(lsp::RustAnalyzerBridge& bridge)
    : bridge_(bridge) {}

std::string RustLSPSymbolExtractor::parseReturnType(const std::string& hover) {
    // Rust hover format: "fn qualifiedName(params) -> ReturnType"
    // Find -> after the closing paren
    auto closeParen = hover.rfind(')');
    if (closeParen == std::string::npos) return "";

    auto arrowPos = hover.find("->", closeParen);
    if (arrowPos == std::string::npos) return "";

    std::string retType = hover.substr(arrowPos + 2);

    // Trim whitespace
    auto first = retType.find_first_not_of(" \t\n");
    if (first == std::string::npos) return "";
    auto last = retType.find_last_not_of(" \t\n");
    retType = retType.substr(first, last - first + 1);

    return retType;
}

std::vector<std::string> RustLSPSymbolExtractor::parseParamTypes(const std::string& hover) {
    auto openParen = hover.find('(');
    auto closeParen = hover.rfind(')');
    if (openParen == std::string::npos || closeParen == std::string::npos ||
        closeParen <= openParen) {
        return {};
    }

    std::string params = hover.substr(openParen + 1, closeParen - openParen - 1);

    // Trim
    auto start = params.find_first_not_of(" \t");
    if (start == std::string::npos) return {};
    params = params.substr(start);
    auto end = params.find_last_not_of(" \t");
    params = params.substr(0, end + 1);

    if (params.empty()) return {};

    // Split by comma, respecting angle brackets and parens
    std::vector<std::string> parts;
    int depth = 0;
    std::string current;
    for (char c : params) {
        if (c == '<' || c == '(')
            ++depth;
        else if (c == '>' || c == ')')
            --depth;
        else if (c == ',' && depth == 0) {
            parts.push_back(current);
            current.clear();
            continue;
        }
        current += c;
    }
    if (!current.empty()) parts.push_back(current);

    std::vector<std::string> types;
    for (auto& p : parts) {
        // Trim each param
        auto s = p.find_first_not_of(" \t");
        if (s == std::string::npos) continue;
        auto e = p.find_last_not_of(" \t");
        p = p.substr(s, e - s + 1);

        // Skip self/&self/&mut self
        if (p == "self" || p == "&self" || p == "&mut self" || p == "mut self") continue;

        // Rust format: "name: Type" -- extract after the colon
        auto colonPos = p.find(':');
        if (colonPos != std::string::npos) {
            std::string type = p.substr(colonPos + 1);
            auto ts = type.find_first_not_of(" \t");
            if (ts != std::string::npos) type = type.substr(ts);
            auto te = type.find_last_not_of(" \t");
            if (te != std::string::npos) type = type.substr(0, te + 1);
            if (!type.empty()) types.push_back(type);
        }
    }

    return types;
}

std::string RustLSPSymbolExtractor::detectEnclosingType(const std::string& qualifiedName) {
    // "MyStruct::method" -> "MyStruct"
    // "mymod::MyStruct::method" -> "mymod::MyStruct"
    auto lastSep = qualifiedName.rfind("::");
    if (lastSep == std::string::npos || lastSep == 0) return "";
    return qualifiedName.substr(0, lastSep);
}

std::vector<HostSymbol> RustLSPSymbolExtractor::extractSymbols(const std::string& filePath) {
    std::vector<HostSymbol> result;

    if (!bridge_.isAvailable()) return result;

    // 1. Open document for rust-analyzer analysis
    bridge_.openDocument(filePath);
    struct DocGuard {
        lsp::RustAnalyzerBridge& b;
        const std::string& path;
        ~DocGuard() { b.closeDocument(path); }
    } guard{bridge_, filePath};

    // 2. Get semantic tokens
    auto tokens = bridge_.getSemanticTokens(filePath);
    if (tokens.empty()) {
        return result;
    }

    // 3. Filter and process declaration/definition tokens
    for (const auto& token : tokens) {
        // Only interested in declarations and definitions
        if (!hasModifier(token.modifiers, "declaration") &&
            !hasModifier(token.modifiers, "definition")) {
            continue;
        }

        // Map rust-analyzer semantic token types to HostSymbolKind
        // Rust has no "class" -- struct and enum are the type-level constructs
        bool isFunction = (token.type == "function");
        bool isMethod = (token.type == "method");
        bool isStruct = (token.type == "struct");
        bool isEnum = (token.type == "enum");

        if (!isFunction && !isMethod && !isStruct && !isEnum) continue;

        // 4. Resolve via hover to get qualified name and signature
        auto hover = bridge_.getHoverAt(filePath, token.line, token.column);
        if (!hover) continue;

        std::string qualifiedName = extractQualifiedName(*hover);
        if (qualifiedName.empty()) continue;

        HostSymbol sym;
        sym.qualifiedName = qualifiedName;
        sym.file = filePath;
        sym.line = token.line + 1; // semantic tokens are 0-based, HostSymbol is 1-based

        // Extract simple name from qualified name
        auto lastSep = qualifiedName.rfind("::");
        sym.simpleName = (lastSep != std::string::npos)
                             ? qualifiedName.substr(lastSep + 2)
                             : qualifiedName;

        // Determine kind
        if (isStruct) {
            sym.kind = HostSymbolKind::Struct;
        } else if (isEnum) {
            sym.kind = HostSymbolKind::Enum;
        } else if (isMethod) {
            // Check for static modifier (associated functions without self)
            if (hasModifier(token.modifiers, "static")) {
                sym.kind = HostSymbolKind::StaticMethod;
                sym.isStatic = true;
            } else {
                sym.kind = HostSymbolKind::Method;
            }

            // Detect enclosing type for methods
            std::string enclosing = detectEnclosingType(qualifiedName);
            if (!enclosing.empty()) {
                sym.enclosingClass = enclosing;
            }

            // Parse return type and params from hover
            sym.returnType = parseReturnType(*hover);
            sym.paramTypes = parseParamTypes(*hover);
        } else if (isFunction) {
            sym.kind = HostSymbolKind::Function;
            sym.returnType = parseReturnType(*hover);
            sym.paramTypes = parseParamTypes(*hover);
        }

        result.push_back(std::move(sym));
    }

    return result;
}

} // namespace topo::check
