// RustCallSiteExtractor -- L1 regex-based Rust escape pattern detection.
//
// Scans Rust source files for language escape constructs that do not require
// semantic analysis: unsafe blocks, extern ABI, asm!/include! macros,
// static mut, raw pointer dereferences, etc.
//
// Handles Allman-style brace placement for both unsafe blocks and fn definitions.

#include "RustCallSiteExtractor.h"
#include "RustUnsafeCatalog.h"
#include "topo/Check/CapabilityCatalog.h"

#include <fstream>
#include <regex>
#include <string>
#include <vector>

namespace topo::check {

namespace {

/// Scope entry for brace-depth tracking.
struct ScopeEntry {
    enum Kind { Fn, Impl, Mod };
    std::string name;
    int depth;
    Kind kind;
};

/// Build a caller name from the scope stack.
std::string buildCurrentCaller(const std::vector<ScopeEntry>& scopeStack,
                               bool inFunction, const std::string& currentFunction) {
    if (inFunction && !currentFunction.empty()) {
        // Walk scopes to build qualified name
        std::string result;
        for (const auto& scope : scopeStack) {
            if (!result.empty()) result += "::";
            result += scope.name;
        }
        return result.empty() ? currentFunction : result;
    }

    // Global or impl scope
    if (scopeStack.empty()) return "<global>";
    std::string result;
    for (const auto& scope : scopeStack) {
        if (!result.empty()) result += "::";
        result += scope.name;
    }
    result += "::<scope>";
    return result;
}

/// Strip line comments (// ...) and inline block comments (/* ... */).
/// Does not handle multi-line block comments (handled by state machine in main loop).
std::string stripComments(const std::string& line) {
    std::string result;
    result.reserve(line.size());
    bool inString = false;
    bool inChar = false;

    for (size_t i = 0; i < line.size(); ++i) {
        char c = line[i];

        // Track string literals (skip escaped quotes)
        if (!inChar && c == '"') {
            // Check for raw string: r"..." r#"..."# etc.
            // For simplicity, toggle inString on unescaped "
            if (i > 0 && line[i - 1] == '\\') {
                result += c;
                continue;
            }
            inString = !inString;
            result += c;
            continue;
        }
        if (!inString && c == '\'') {
            if (i > 0 && line[i - 1] == '\\') {
                result += c;
                continue;
            }
            inChar = !inChar;
            result += c;
            continue;
        }

        if (inString || inChar) {
            result += c;
            continue;
        }

        // Line comment
        if (c == '/' && i + 1 < line.size() && line[i + 1] == '/') {
            break; // rest is comment
        }

        // Inline block comment
        if (c == '/' && i + 1 < line.size() && line[i + 1] == '*') {
            auto closePos = line.find("*/", i + 2);
            if (closePos != std::string::npos) {
                i = closePos + 1; // skip to end of */
                continue;
            }
            break; // multi-line block comment starts here
        }

        result += c;
    }
    return result;
}

} // anonymous namespace

std::vector<DetectedCallSite> RustCallSiteExtractor::extractCallSites(const std::string& filePath) {
    std::vector<DetectedCallSite> results;
    std::ifstream file(filePath);
    if (!file.is_open()) return results;

    // --- Regex patterns (compiled once) ---

    // Function definition: optional qualifiers, fn name, params
    // Accepts any ABI: extern "C" fn, extern "system" fn, etc.
    static const std::regex fnRegex(
        R"((?:pub\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]+"\s+)?fn\s+(\w+))");

    // unsafe block: unsafe { on same line
    static const std::regex unsafeBlockRegex(R"(\bunsafe\s*\{)");
    // unsafe keyword not followed by fn/impl/trait (standalone — brace may be on next line)
    static const std::regex standaloneUnsafeRegex(R"(\bunsafe\s*$)");
    // unsafe fn/impl/trait (not an unsafe block)
    static const std::regex unsafeFnRegex(R"(\bunsafe\s+(fn|impl|trait)\b)");
    // unsafe trait declaration: unsafe trait Name
    static const std::regex unsafeTraitDeclRegex(R"(\bunsafe\s+trait\s+(\w+))");
    // unsafe impl block: unsafe impl[<...>] [!]Trait [for Type]
    static const std::regex unsafeImplRegex(
        R"(\bunsafe\s+impl(?:\s*<[^>]*>)?\s+!?(\w+))");

    // extern "ABI" block or fn — any ABI string, not just "C"
    static const std::regex externAbiRegex(R"(\bextern\s+"[^"]+")");

    // Inline assembly (asm!, core::arch::asm!, std::arch::asm!)
    static const std::regex asmRegex(
        R"(\b(?:core\s*::\s*arch\s*::\s*|std\s*::\s*arch\s*::\s*)?asm!\s*\()");
    // Global assembly (no unsafe required)
    static const std::regex globalAsmRegex(R"(\bglobal_asm!\s*\()");
    // include! (textual source inclusion from arbitrary path)
    static const std::regex includeRegex(R"(\binclude!\s*\()");
    // include_bytes!/include_str! (filesystem read at compile time)
    static const std::regex includeBytesRegex(R"(\binclude_(bytes|str)!\s*\()");

    // static mut declaration
    static const std::regex staticMutRegex(R"(\bstatic\s+mut\b)");

    // Raw pointer dereference: *ptr, *mut_ptr, (*expr)
    static const std::regex ptrDerefRegex(R"(\*\s*(?:mut\s+)?[a-zA-Z_]\w*)");

    // env! macro (compile-time environment variable access)
    static const std::regex envMacroRegex(R"(\benv!\s*\()");

    // impl scope: impl [Trait for] Type
    static const std::regex implRegex(R"(\bimpl\b(?:\s*<[^>]*>)?\s+(?:\w+\s+for\s+)?(\w+))");

    // mod scope: mod name
    static const std::regex modRegex(R"(\bmod\s+(\w+)\s*\{)");

    // --- State variables ---
    int braceDepth = 0;
    bool inFunction = false;
    std::string currentFunction;
    std::vector<ScopeEntry> scopeStack;

    // Allman brace: pending unsafe block
    bool pendingUnsafe = false;

    // Allman brace: pending fn definition
    std::string pendingFnName;
    bool pendingFnUnsafe = false;

    // Block comment state machine
    bool inBlockComment = false;

    // --- Helper lambda ---
    auto emitSite = [&](const std::string& caller, const std::string& callee,
                        std::optional<CapabilityKind> cap, UnsafeLevel level, int ln) {
        DetectedCallSite site;
        site.callerQualifiedName = caller;
        site.calleePattern = callee;
        site.capability = cap;
        site.unsafeLevel = level;
        site.file = filePath;
        site.line = ln;
        results.push_back(std::move(site));
    };

    // Scan a line for escape patterns.
    // Called for every effective line (both inside and outside function bodies).
    auto scanPatterns = [&](const std::string& scanLine, const std::string& caller, int ln) {
        // --- Escape (Level 4) patterns ---

        // unsafe trait declaration: `unsafe trait Name { ... }`
        // Must precede the unsafe-block test so the trait header is not also
        // counted as a stray "unsafe" token.
        std::smatch unsafeTraitMatch;
        if (std::regex_search(scanLine, unsafeTraitMatch, unsafeTraitDeclRegex)) {
            std::string callee = "<unsafe-trait-decl-" + unsafeTraitMatch[1].str() + ">";
            emitSite(caller, callee, std::nullopt, UnsafeLevel::Escape, ln);
            pendingUnsafe = false;
        }

        // unsafe impl block: `unsafe impl[<Generics>] [!]Trait [for Type]`
        std::smatch unsafeImplMatch;
        if (std::regex_search(scanLine, unsafeImplMatch, unsafeImplRegex)) {
            std::string callee = "<unsafe-impl-" + unsafeImplMatch[1].str() + ">";
            emitSite(caller, callee, std::nullopt, UnsafeLevel::Escape, ln);
            pendingUnsafe = false;
        }

        // unsafe block: unsafe { on same line
        if (std::regex_search(scanLine, unsafeBlockRegex)) {
            emitSite(caller, "unsafe-block", std::nullopt, UnsafeLevel::Escape, ln);
            pendingUnsafe = false; // consumed
        }
        // Check for standalone "unsafe" without brace (Allman style)
        else if (std::regex_search(scanLine, standaloneUnsafeRegex) &&
                 !std::regex_search(scanLine, unsafeFnRegex)) {
            pendingUnsafe = true;
        }

        // extern "ABI" (any ABI string)
        if (std::regex_search(scanLine, externAbiRegex)) {
            // Only report if not already part of a fn definition (extern fn is tracked separately)
            if (scanLine.find("fn ") == std::string::npos) {
                emitSite(caller, "extern-abi-block", std::nullopt, UnsafeLevel::Escape, ln);
            }
        }

        // Inline assembly
        if (std::regex_search(scanLine, asmRegex)) {
            emitSite(caller, "asm!", std::nullopt, UnsafeLevel::Escape, ln);
        }

        // Global assembly (no unsafe required)
        if (std::regex_search(scanLine, globalAsmRegex)) {
            emitSite(caller, "global_asm!", std::nullopt, UnsafeLevel::Escape, ln);
        }

        // include! (textual source inclusion)
        if (std::regex_search(scanLine, includeRegex)) {
            emitSite(caller, "include!", std::nullopt, UnsafeLevel::Escape, ln);
        }

        // include_bytes!/include_str! (filesystem read)
        if (std::regex_search(scanLine, includeBytesRegex)) {
            emitSite(caller, "include_bytes/str!", CapabilityKind::File, UnsafeLevel::System, ln);
        }

        // Raw pointer dereference
        if (std::regex_search(scanLine, ptrDerefRegex)) {
            emitSite(caller, "ptr-deref", std::nullopt, UnsafeLevel::Escape, ln);
        }

        // env! macro (compile-time environment access)
        if (std::regex_search(scanLine, envMacroRegex)) {
            emitSite(caller, "env!", CapabilityKind::Process, UnsafeLevel::System, ln);
        }
    };

    std::string line;
    int lineNum = 0;

    while (std::getline(file, line)) {
        ++lineNum;

        // --- Block comment state machine ---
        if (inBlockComment) {
            auto closePos = line.find("*/");
            if (closePos != std::string::npos) {
                inBlockComment = false;
            }
            continue;
        }

        // Check for block comment opening
        {
            auto commentPos = line.find("/*");
            if (commentPos != std::string::npos) {
                auto closePos = line.find("*/", commentPos + 2);
                if (closePos == std::string::npos) {
                    // Multi-line block comment starts
                    inBlockComment = true;
                    line = line.substr(0, commentPos);
                }
                // Same-line block comments are handled by stripComments
            }
        }

        // Strip comments from the line
        std::string effectiveLine = stripComments(line);

        // Skip empty/whitespace-only lines
        if (effectiveLine.find_first_not_of(" \t") == std::string::npos) continue;

        // --- Allman-style pending unsafe: previous line had "unsafe" without "{" ---
        if (pendingUnsafe && effectiveLine.find('{') != std::string::npos) {
            std::string caller = buildCurrentCaller(scopeStack, inFunction, currentFunction);
            emitSite(caller, "unsafe-block", std::nullopt, UnsafeLevel::Escape, lineNum);
            pendingUnsafe = false;
        }

        // --- Allman-style pending fn: previous line had fn signature without "{" ---
        if (!pendingFnName.empty() && effectiveLine.find('{') != std::string::npos) {
            inFunction = true;
            currentFunction = pendingFnName;
            scopeStack.push_back({pendingFnName, braceDepth, ScopeEntry::Fn});
            if (pendingFnUnsafe) {
                std::string caller = buildCurrentCaller(scopeStack, inFunction, currentFunction);
                emitSite(caller, "unsafe-fn-decl", std::nullopt, UnsafeLevel::Escape, lineNum);
            }
            pendingFnName.clear();
            pendingFnUnsafe = false;
        }

        // --- Brace tracking ---
        for (char c : effectiveLine) {
            if (c == '{') {
                ++braceDepth;
            } else if (c == '}') {
                --braceDepth;
                if (braceDepth < 0) braceDepth = 0;

                // Pop scope if we've returned to its depth
                while (!scopeStack.empty() && braceDepth <= scopeStack.back().depth) {
                    if (scopeStack.back().kind == ScopeEntry::Fn) {
                        inFunction = false;
                        currentFunction.clear();
                    }
                    scopeStack.pop_back();
                }
            }
        }

        // --- Scope detection ---

        // impl block
        std::smatch implMatch;
        if (!inFunction && std::regex_search(effectiveLine, implMatch, implRegex)) {
            if (effectiveLine.find('{') != std::string::npos) {
                scopeStack.push_back({implMatch[1].str(), braceDepth - 1, ScopeEntry::Impl});
            }
        }

        // mod block
        std::smatch modMatch;
        if (!inFunction && std::regex_search(effectiveLine, modMatch, modRegex)) {
            scopeStack.push_back({modMatch[1].str(), braceDepth - 1, ScopeEntry::Mod});
        }

        // --- Function detection ---
        std::smatch fnMatch;
        if (!inFunction && pendingFnName.empty() &&
            std::regex_search(effectiveLine, fnMatch, fnRegex)) {
            std::string fname = fnMatch[1].str();
            bool isUnsafe = (effectiveLine.find("unsafe") != std::string::npos &&
                             !std::regex_search(effectiveLine, unsafeBlockRegex));

            if (effectiveLine.find('{') != std::string::npos) {
                inFunction = true;
                currentFunction = fname;
                scopeStack.push_back({fname, braceDepth - 1, ScopeEntry::Fn});

                if (isUnsafe) {
                    std::string caller = buildCurrentCaller(scopeStack, inFunction, currentFunction);
                    emitSite(caller, "unsafe-fn-decl", std::nullopt, UnsafeLevel::Escape, lineNum);
                }
            } else {
                // Allman style: brace on next line
                pendingFnName = fname;
                pendingFnUnsafe = isUnsafe;
            }
        }

        // --- Pattern scanning ---
        std::string caller = buildCurrentCaller(scopeStack, inFunction, currentFunction);
        scanPatterns(effectiveLine, caller, lineNum);

        // --- static mut at module/impl scope (not inside function body) ---
        if (!inFunction && std::regex_search(effectiveLine, staticMutRegex)) {
            emitSite(caller, "static-mut", std::nullopt, UnsafeLevel::Escape, lineNum);
        }
    }

    return results;
}

} // namespace topo::check
