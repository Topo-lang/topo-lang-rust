// RustCallEdgeExtractor — L1 regex extractor for caller→callee edges.
//
// Mirrors RustCallSiteExtractor's scope-tracking state machine
// (mod/impl/fn brace-depth stack). For each identifier call inside a
// function body, emits a CallEdge with the caller qualified by the
// current mod/impl/fn scope and the callee set to the raw token
// (already scoped via `::` when the call was written as `module::foo(...)`).

#include "RustCallEdgeExtractor.h"

#include <cctype>
#include <fstream>
#include <regex>
#include <string>
#include <unordered_set>
#include <vector>

namespace topo::check {

namespace {

/// Scope entry for brace-depth tracking — same shape as the one in
/// RustCallSiteExtractor.
struct ScopeEntry {
    enum Kind { Fn, Impl, Mod };
    std::string name;
    int depth;
    Kind kind;
};

/// Build a caller qualified name from the scope stack. A `Fn` scope at
/// the top contributes its name; outer `Mod` and `Impl` scopes are
/// concatenated as `::` separated qualifiers.
std::string buildCallerName(const std::vector<ScopeEntry>& scopeStack) {
    std::string result;
    for (const auto& s : scopeStack) {
        if (!result.empty()) result += "::";
        result += s.name;
    }
    return result;
}

/// Strip line comments (// ...) and inline same-line block comments
/// (/* ... */). Multi-line block comments are handled by the state
/// machine in the main loop.
std::string stripComments(const std::string& line) {
    std::string result;
    result.reserve(line.size());
    bool inString = false;
    bool inChar = false;

    for (size_t i = 0; i < line.size(); ++i) {
        char c = line[i];

        if (!inChar && c == '"') {
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
            break;
        }

        // Inline block comment
        if (c == '/' && i + 1 < line.size() && line[i + 1] == '*') {
            auto closePos = line.find("*/", i + 2);
            if (closePos != std::string::npos) {
                i = closePos + 1;
                continue;
            }
            break;
        }

        result += c;
    }
    return result;
}

/// Mask string and char literal contents with spaces so call-like tokens
/// inside them do not produce spurious matches.
std::string maskStringLiterals(const std::string& line) {
    std::string out = line;
    for (size_t i = 0; i < out.size(); ++i) {
        char c = out[i];
        if (c == '"') {
            out[i] = ' ';
            ++i;
            while (i < out.size() && out[i] != '"') {
                if (out[i] == '\\' && i + 1 < out.size()) {
                    out[i] = ' ';
                    out[i + 1] = ' ';
                    i += 2;
                    continue;
                }
                out[i] = ' ';
                ++i;
            }
            if (i < out.size()) out[i] = ' ';
        } else if (c == '\'') {
            // Char literal or lifetime. Lifetimes (`'a`) terminate at a
            // non-alpha; char literals end at the next `'`. Lifetimes are
            // benign so we conservatively skip only single-quoted forms
            // that look like char literals.
            if (i + 2 < out.size() && (out[i + 2] == '\'' ||
                                       (out[i + 1] == '\\' && i + 3 < out.size()))) {
                out[i] = ' ';
                size_t end = (out[i + 1] == '\\') ? i + 3 : i + 2;
                for (size_t j = i + 1; j <= end && j < out.size(); ++j) out[j] = ' ';
                i = end;
            }
        }
    }
    return out;
}

/// Rust keywords + control statements that the regex `\w+\s*\(` would
/// otherwise mistake for callees. Skip these to avoid emitting spurious
/// edges for `if (...)`, `match (...)`, etc.
const std::unordered_set<std::string>& rustControlKeywords() {
    static const std::unordered_set<std::string> kws = {
        // Control flow / expressions
        "if", "else", "for", "while", "loop", "match", "return", "break",
        "continue", "let", "mut", "in", "as", "where", "move", "ref",
        "yield", "await", "async", "static", "const", "type",
        // Declarations
        "fn", "struct", "enum", "trait", "impl", "mod", "use", "pub",
        "extern", "unsafe", "self", "Self", "super", "crate", "dyn",
        "box",
        // Types and literals
        "true", "false", "Some", "None", "Ok", "Err",
        "i8", "i16", "i32", "i64", "i128", "isize",
        "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char", "str",
    };
    return kws;
}

bool isRustControlKeyword(const std::string& name) {
    const auto& kws = rustControlKeywords();
    return kws.count(name) > 0;
}

bool isPreprocessorLine(const std::string& line) {
    // Rust has no preprocessor; attribute lines like `#[...]` and
    // `#![...]` should be skipped because they may contain identifiers
    // that look like calls.
    auto pos = line.find_first_not_of(" \t");
    if (pos == std::string::npos) return false;
    return line[pos] == '#';
}

} // anonymous namespace

std::vector<CallEdge> RustCallEdgeExtractor::extractCallEdges(const std::string& filePath) {
    std::vector<CallEdge> results;
    std::ifstream file(filePath);
    if (!file.is_open()) return results;

    // --- Regex patterns (compiled once) ---
    static const std::regex fnRegex(
        R"((?:pub(?:\([\w:]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?(?:extern\s+"[^"]+"\s+)?fn\s+(\w+))");
    static const std::regex implRegex(
        R"(\bimpl\b(?:\s*<[^>]*>)?\s+(?:\w+\s+for\s+)?(\w+))");
    static const std::regex modRegex(R"(\bmod\s+(\w+)\s*\{)");

    // Match call targets: optional path prefix (`a::b::`) then identifier then `(`.
    // Capture group 1 is the full callee token (potentially scope-qualified).
    static const std::regex callRegex(
        R"(((?:[\w]+\s*::\s*)*[\w]+)\s*\()");

    // --- State variables ---
    int braceDepth = 0;
    bool inFunction = false;
    std::string currentFunction;
    std::vector<ScopeEntry> scopeStack;

    // Allman brace fix: pending fn signature when `{` is on the next line.
    std::string pendingFnName;

    // Block comment state machine.
    bool inBlockComment = false;

    std::string line;
    int lineNum = 0;

    while (std::getline(file, line)) {
        ++lineNum;

        // --- Block comment state machine ---
        if (inBlockComment) {
            auto closePos = line.find("*/");
            if (closePos != std::string::npos) {
                inBlockComment = false;
                line = line.substr(closePos + 2);
            } else {
                continue;
            }
        }

        {
            auto commentPos = line.find("/*");
            if (commentPos != std::string::npos) {
                auto closePos = line.find("*/", commentPos + 2);
                if (closePos == std::string::npos) {
                    inBlockComment = true;
                    line = line.substr(0, commentPos);
                }
            }
        }

        // Skip Rust attribute lines (`#[...]` / `#![...]`).
        if (isPreprocessorLine(line)) continue;

        // Strip same-line comments.
        std::string effectiveLine = stripComments(line);
        if (effectiveLine.find_first_not_of(" \t") == std::string::npos) continue;

        // Allman-style pending fn: previous line had a fn signature without `{`.
        if (!pendingFnName.empty() && effectiveLine.find('{') != std::string::npos) {
            inFunction = true;
            currentFunction = pendingFnName;
            scopeStack.push_back({pendingFnName, braceDepth, ScopeEntry::Fn});
            pendingFnName.clear();
        }

        // --- Brace tracking ---
        for (char c : effectiveLine) {
            if (c == '{') {
                ++braceDepth;
            } else if (c == '}') {
                --braceDepth;
                if (braceDepth < 0) braceDepth = 0;
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

        // mod block
        std::smatch modMatch;
        if (!inFunction && std::regex_search(effectiveLine, modMatch, modRegex)) {
            scopeStack.push_back({modMatch[1].str(), braceDepth - 1, ScopeEntry::Mod});
            continue;
        }

        // impl block
        std::smatch implMatch;
        if (!inFunction && std::regex_search(effectiveLine, implMatch, implRegex)) {
            if (effectiveLine.find('{') != std::string::npos) {
                scopeStack.push_back({implMatch[1].str(), braceDepth - 1, ScopeEntry::Impl});
            }
            continue;
        }

        // fn definition
        std::smatch fnMatch;
        if (!inFunction && pendingFnName.empty() &&
            std::regex_search(effectiveLine, fnMatch, fnRegex)) {
            std::string fname = fnMatch[1].str();
            if (effectiveLine.find('{') != std::string::npos) {
                inFunction = true;
                currentFunction = fname;
                scopeStack.push_back({fname, braceDepth - 1, ScopeEntry::Fn});
            } else if (effectiveLine.find(';') == std::string::npos) {
                // No body and no semicolon — Allman-style brace pending.
                pendingFnName = fname;
            }
            continue;
        }

        // Inside a function body: scan for call targets.
        if (!inFunction || braceDepth <= 0) continue;

        std::string callerName = buildCallerName(scopeStack);

        // Mask string/char literals so call-like tokens inside them are ignored.
        std::string scanLine = maskStringLiterals(effectiveLine);

        std::string remaining = scanLine;
        size_t absOffset = 0;
        while (true) {
            std::smatch m;
            if (!std::regex_search(remaining, m, callRegex)) break;
            std::string callee = m[1].str();
            size_t matchPos = absOffset + static_cast<size_t>(m.position(1));
            size_t matchLen = m[1].length();

            // Strip whitespace around `::` for a clean callee token.
            std::string normalized;
            normalized.reserve(callee.size());
            for (char ch : callee) {
                if (ch != ' ' && ch != '\t') normalized += ch;
            }

            // Strip Rust's `crate::` prefix so the callee qualified name
            // matches the .topo SymbolTable entries (which never include
            // `crate::`). Also strip a leading `::` (absolute path).
            if (normalized.rfind("crate::", 0) == 0) {
                normalized = normalized.substr(7);
            } else if (normalized.rfind("::", 0) == 0) {
                normalized = normalized.substr(2);
            }

            // Extract the simple (last) name for keyword filtering.
            std::string simple;
            auto scopeEnd = normalized.rfind("::");
            if (scopeEnd != std::string::npos) {
                simple = normalized.substr(scopeEnd + 2);
            } else {
                simple = normalized;
            }

            bool skip = false;
            if (simple.empty() ||
                (!std::isalpha(static_cast<unsigned char>(simple[0])) && simple[0] != '_')) {
                skip = true;
            }
            if (!skip && isRustControlKeyword(simple)) {
                skip = true;
            }

            // Skip macro invocations: `ident!(...)`. The `!` sits between
            // the identifier and the open paren.
            if (!skip) {
                size_t identEnd = matchPos + matchLen;
                while (identEnd < scanLine.size() &&
                       (scanLine[identEnd] == ' ' || scanLine[identEnd] == '\t')) {
                    ++identEnd;
                }
                if (identEnd < scanLine.size() && scanLine[identEnd] == '!') {
                    skip = true;
                }
            }

            // Skip method calls on receivers: `obj.foo(...)`.
            if (!skip && matchPos >= 1) {
                char prev = scanLine[matchPos - 1];
                if (prev == '.') skip = true;
            }

            // Skip the function definition itself when its `{` is on
            // the same line.
            if (!skip && simple == currentFunction &&
                effectiveLine.find('{') != std::string::npos) {
                skip = true;
            }

            if (!skip) {
                CallEdge edge;
                edge.caller = callerName;
                edge.callee = normalized;
                edge.file = filePath;
                edge.line = lineNum;
                results.push_back(std::move(edge));
            }

            size_t advance = static_cast<size_t>(m.position(1)) + matchLen;
            if (advance == 0) advance = 1;
            remaining = remaining.substr(advance);
            absOffset += advance;
        }
    }

    return results;
}

} // namespace topo::check
