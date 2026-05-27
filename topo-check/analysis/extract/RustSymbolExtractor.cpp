// RustSymbolExtractor — L1 regex-based symbol extraction from Rust source files.
//
// Strategy: line scanning with brace-depth tracking for mod/struct/enum/impl scopes.
// Extracts function definitions, struct/enum/trait declarations, and maps
// Rust visibility modifiers (pub, pub(crate), pub(super), none) to Visibility.
//
// This is a SAFETY NET — false positives are acceptable, false negatives are bugs.

#include "RustSymbolExtractor.h"

#include <fstream>
#include <regex>
#include <string>
#include <vector>

namespace topo::check {

namespace {

/// Scope entry for tracking nested mod/struct/impl/trait blocks.
struct ScopeEntry {
    std::string name;
    int braceDepth;
    enum Kind { Mod, Struct, Enum, Trait, Impl } kind;
};

/// Build a qualified name from the scope stack.
std::string buildQualified(const std::vector<ScopeEntry>& scopes) {
    std::string result;
    for (const auto& s : scopes) {
        if (!result.empty()) result += "::";
        result += s.name;
    }
    return result;
}

/// Determine the enclosing type (if any) from the scope stack.
/// Returns the name of the innermost impl/struct scope.
std::string findEnclosingType(const std::vector<ScopeEntry>& scopes) {
    for (auto it = scopes.rbegin(); it != scopes.rend(); ++it) {
        if (it->kind == ScopeEntry::Impl || it->kind == ScopeEntry::Struct) {
            return it->name;
        }
    }
    return "";
}

/// Map Rust visibility modifier text to Visibility enum.
/// pub         → Public
/// pub(crate)  → Internal
/// pub(super)  → Protected
/// (none)      → Private
Visibility mapVisibility(const std::string& pubSpec) {
    if (pubSpec.empty()) return Visibility::Private;
    if (pubSpec.find("crate") != std::string::npos) return Visibility::Internal;
    if (pubSpec.find("super") != std::string::npos) return Visibility::Protected;
    return Visibility::Public;
}

/// Extract visibility specifier from a line prefix.
/// Returns the full pub(...) or "pub" token, or empty if none.
std::string extractPubSpec(const std::string& line) {
    // Match: pub(crate), pub(super), pub(in path), or bare pub
    static const std::regex pubRegex(R"(\bpub\s*(\([\w:]+\))?)");
    std::smatch m;
    if (std::regex_search(line, m, pubRegex)) {
        return m[0].str();
    }
    return "";
}

/// Extract return type from a Rust function signature.
/// Looks for "-> Type" after the parameter list.
std::string extractReturnType(const std::string& line) {
    auto arrowPos = line.find("->");
    if (arrowPos == std::string::npos) return "";

    std::string after = line.substr(arrowPos + 2);
    // Trim leading whitespace
    auto start = after.find_first_not_of(" \t");
    if (start == std::string::npos) return "";
    after = after.substr(start);

    // Take until '{', 'where', or end of line
    std::string result;
    int angleBracketDepth = 0;
    for (char c : after) {
        if (c == '<') ++angleBracketDepth;
        else if (c == '>') --angleBracketDepth;
        else if (angleBracketDepth == 0 && (c == '{' || c == ';')) break;
        result += c;
    }

    // Trim trailing whitespace and "where"
    auto end = result.find_last_not_of(" \t");
    if (end != std::string::npos) result = result.substr(0, end + 1);

    // If it ends with "where", strip it
    if (result.size() >= 5 && result.substr(result.size() - 5) == "where") {
        result = result.substr(0, result.size() - 5);
        end = result.find_last_not_of(" \t");
        if (end != std::string::npos) result = result.substr(0, end + 1);
    }

    return result;
}

/// Extract parameter types from a Rust function signature.
/// Rust params are "name: Type" — extracts the Type part. Skips self/&self/&mut self.
std::vector<std::string> extractParamTypes(const std::string& line) {
    auto openParen = line.find('(');
    auto closeParen = line.rfind(')');
    if (openParen == std::string::npos || closeParen == std::string::npos ||
        closeParen <= openParen)
        return {};

    std::string params = line.substr(openParen + 1, closeParen - openParen - 1);

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
        if (c == '<' || c == '(') ++depth;
        else if (c == '>' || c == ')') --depth;
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

        // Skip self variants: self, &self, &mut self, mut self
        if (p == "self" || p == "&self" || p == "&mut self" || p == "mut self") continue;

        // Rust param format: "name: Type" — extract the Type after ':'
        auto colonPos = p.find(':');
        if (colonPos != std::string::npos) {
            std::string type = p.substr(colonPos + 1);
            auto ts = type.find_first_not_of(" \t");
            if (ts != std::string::npos) {
                type = type.substr(ts);
                auto te = type.find_last_not_of(" \t");
                if (te != std::string::npos) type = type.substr(0, te + 1);
                types.push_back(type);
            }
        }
    }
    return types;
}

/// Check if a line is entirely a comment or whitespace.
bool isCommentOrEmpty(const std::string& line) {
    auto pos = line.find_first_not_of(" \t");
    if (pos == std::string::npos) return true;
    return (line.size() > pos + 1 && line[pos] == '/' && line[pos + 1] == '/');
}

} // anonymous namespace

std::vector<HostSymbol> RustSymbolExtractor::extractSymbols(const std::string& filePath) {
    std::vector<HostSymbol> result;

    std::ifstream file(filePath);
    if (!file.is_open()) return result;

    // Scope tracking
    std::vector<ScopeEntry> scopeStack;

    // Regex patterns — compiled once
    // Module: [pub] mod name { or mod name;
    static const std::regex modRegex(
        R"(^\s*(?:pub(?:\([\w:]+\))?\s+)?mod\s+(\w+)\s*\{)");
    // Struct/enum: [pub] struct/enum Name [<...>] [{ or ( or ;]
    static const std::regex structEnumRegex(
        R"(^\s*(?:pub(?:\([\w:]+\))?\s+)?(?:struct|enum)\s+(\w+))");
    // Trait: [pub] trait Name [<...>] {
    static const std::regex traitRegex(
        R"(^\s*(?:pub(?:\([\w:]+\))?\s+)?trait\s+(\w+))");
    // Impl: impl [<...>] [Trait for] Type [<...>] {
    static const std::regex implRegex(
        R"(^\s*impl(?:\s*<[^>]*>)?\s+(?:(\w+)\s+for\s+)?(\w+))");
    // Function: [pub] [async] [unsafe] [extern "C"] fn name [<...>] (
    static const std::regex fnRegex(
        R"(^\s*(?:pub(?:\([\w:]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?(?:extern\s+"C"\s+)?fn\s+(\w+)\s*(?:<[^>]*>)?\s*\()");

    // Block comment state
    bool inBlockComment = false;

    std::string line;
    int lineNum = 0;
    int braceDepth = 0;

    while (std::getline(file, line)) {
        ++lineNum;

        // --- Block comment state machine ---
        if (inBlockComment) {
            auto closePos = line.find("*/");
            if (closePos != std::string::npos) {
                inBlockComment = false;
                // Process remainder after */
                line = line.substr(closePos + 2);
            } else {
                continue;
            }
        }

        // Skip line comments and empty lines
        if (isCommentOrEmpty(line)) continue;

        // Strip inline comments for analysis
        std::string effectiveLine = line;
        for (size_t i = 0; i < effectiveLine.size(); ++i) {
            char c = effectiveLine[i];
            // Skip string literals
            if (c == '"') {
                // Check for raw string: r"...", r#"..."#, etc.
                if (i > 0 && effectiveLine[i - 1] == 'r') {
                    // Count # before "
                    size_t hashStart = i - 1;
                    while (hashStart > 0 && effectiveLine[hashStart - 1] == '#') --hashStart;
                    // Skip raw string content (simplified: may miss multi-line)
                }
                ++i;
                while (i < effectiveLine.size() && effectiveLine[i] != '"') {
                    if (effectiveLine[i] == '\\') ++i;
                    ++i;
                }
                continue;
            }
            if (c == '\'') {
                // Char literal or lifetime — skip single character
                if (i + 2 < effectiveLine.size() && effectiveLine[i + 2] == '\'') {
                    i += 2;
                } else if (i + 3 < effectiveLine.size() && effectiveLine[i + 1] == '\\') {
                    i += 3;
                }
                // Otherwise it's likely a lifetime annotation — not a problem
                continue;
            }
            if (c == '/' && i + 1 < effectiveLine.size()) {
                if (effectiveLine[i + 1] == '/') {
                    effectiveLine = effectiveLine.substr(0, i);
                    break;
                }
                if (effectiveLine[i + 1] == '*') {
                    auto closePos = effectiveLine.find("*/", i + 2);
                    if (closePos != std::string::npos) {
                        effectiveLine.erase(i, closePos + 2 - i);
                        --i;
                    } else {
                        effectiveLine = effectiveLine.substr(0, i);
                        inBlockComment = true;
                        break;
                    }
                }
            }
        }

        // Track braces on effective line
        for (size_t i = 0; i < effectiveLine.size(); ++i) {
            char c = effectiveLine[i];
            // Skip string contents for brace counting
            if (c == '"') {
                ++i;
                while (i < effectiveLine.size() && effectiveLine[i] != '"') {
                    if (effectiveLine[i] == '\\') ++i;
                    ++i;
                }
                continue;
            }
            if (c == '{') {
                ++braceDepth;
            } else if (c == '}') {
                --braceDepth;
                if (braceDepth < 0) braceDepth = 0;
                // Pop scope if we've closed back to the entry depth
                while (!scopeStack.empty() && braceDepth <= scopeStack.back().braceDepth) {
                    scopeStack.pop_back();
                }
            }
        }

        // --- Pattern matching on effective line ---

        // Module declaration with brace
        std::smatch modMatch;
        if (std::regex_search(effectiveLine, modMatch, modRegex)) {
            std::string modName = modMatch[1].str();
            scopeStack.push_back({modName, braceDepth - 1, ScopeEntry::Mod});
            continue;
        }

        // Trait declaration
        std::smatch traitMatch;
        if (std::regex_search(effectiveLine, traitMatch, traitRegex)) {
            std::string traitName = traitMatch[1].str();

            HostSymbol sym;
            std::string prefix = buildQualified(scopeStack);
            sym.qualifiedName = prefix.empty() ? traitName : prefix + "::" + traitName;
            sym.simpleName = traitName;
            sym.kind = HostSymbolKind::Class; // Traits map to Class kind
            sym.file = filePath;
            sym.line = lineNum;

            std::string pubSpec = extractPubSpec(effectiveLine);
            sym.hostVisibility = mapVisibility(pubSpec);

            result.push_back(std::move(sym));

            if (effectiveLine.find('{') != std::string::npos) {
                scopeStack.push_back({traitName, braceDepth - 1, ScopeEntry::Trait});
            }
            continue;
        }

        // Impl block
        std::smatch implMatch;
        if (std::regex_search(effectiveLine, implMatch, implRegex)) {
            // implMatch[1] = trait name (if "Trait for"), implMatch[2] = type name
            std::string typeName = implMatch[2].str();
            if (effectiveLine.find('{') != std::string::npos) {
                scopeStack.push_back({typeName, braceDepth - 1, ScopeEntry::Impl});
            }
            continue;
        }

        // Struct/enum declaration
        std::smatch seMatch;
        if (std::regex_search(effectiveLine, seMatch, structEnumRegex)) {
            std::string name = seMatch[1].str();
            bool isStruct = effectiveLine.find("struct") != std::string::npos;

            HostSymbol sym;
            std::string prefix = buildQualified(scopeStack);
            sym.qualifiedName = prefix.empty() ? name : prefix + "::" + name;
            sym.simpleName = name;
            sym.kind = isStruct ? HostSymbolKind::Struct : HostSymbolKind::Enum;
            sym.file = filePath;
            sym.line = lineNum;

            std::string pubSpec = extractPubSpec(effectiveLine);
            sym.hostVisibility = mapVisibility(pubSpec);

            result.push_back(std::move(sym));

            if (effectiveLine.find('{') != std::string::npos) {
                auto kind = isStruct ? ScopeEntry::Struct : ScopeEntry::Enum;
                scopeStack.push_back({name, braceDepth - 1, kind});
            }
            continue;
        }

        // Function definition
        std::smatch fnMatch;
        if (std::regex_search(effectiveLine, fnMatch, fnRegex)) {
            std::string funcName = fnMatch[1].str();

            HostSymbol sym;
            std::string prefix = buildQualified(scopeStack);
            sym.qualifiedName = prefix.empty() ? funcName : prefix + "::" + funcName;
            sym.simpleName = funcName;
            sym.file = filePath;
            sym.line = lineNum;

            // Determine kind based on scope
            std::string enclosing = findEnclosingType(scopeStack);
            if (!enclosing.empty()) {
                // Inside an impl or struct block — it's a method
                sym.enclosingClass = prefix;
                // Check for static: no self parameter
                bool hasSelf = (effectiveLine.find("&self") != std::string::npos ||
                                effectiveLine.find("&mut self") != std::string::npos ||
                                effectiveLine.find("self") != std::string::npos);
                // More precise self check: look for self as a parameter (not in a type position)
                auto parenPos = effectiveLine.find('(');
                if (parenPos != std::string::npos) {
                    auto afterParen = effectiveLine.substr(parenPos + 1);
                    auto trimmedParam = afterParen.substr(0, afterParen.find(','));
                    auto trimStart = trimmedParam.find_first_not_of(" \t");
                    if (trimStart != std::string::npos) {
                        trimmedParam = trimmedParam.substr(trimStart);
                    }
                    hasSelf = (trimmedParam.find("self") == 0 ||
                               trimmedParam.find("&self") == 0 ||
                               trimmedParam.find("&mut self") == 0 ||
                               trimmedParam.find("mut self") == 0);
                }

                if (hasSelf) {
                    sym.kind = HostSymbolKind::Method;
                } else {
                    sym.kind = HostSymbolKind::StaticMethod;
                    sym.isStatic = true;
                }
            } else {
                sym.kind = HostSymbolKind::Function;
            }

            // Extract return type
            sym.returnType = extractReturnType(effectiveLine);

            // Extract parameter types
            sym.paramTypes = extractParamTypes(effectiveLine);

            // Map visibility
            std::string pubSpec = extractPubSpec(effectiveLine);
            sym.hostVisibility = mapVisibility(pubSpec);

            // Check const fn
            if (effectiveLine.find("const fn") != std::string::npos) {
                sym.isConst = true;
            }

            result.push_back(std::move(sym));
        }
    }

    return result;
}

} // namespace topo::check
