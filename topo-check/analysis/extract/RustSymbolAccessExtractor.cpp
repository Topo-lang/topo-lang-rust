// RustSymbolAccessExtractor — L1 regex extractor for global symbol writes.
//
// Strategy:
//   Pass 1: scan the file once and collect module-level statics:
//             - `static X: T = ...;`
//             - `static mut X: T = ...;`
//             - `const X: T = ...;`
//             - `thread_local! { static X: T = ...; }`
//           Items inside `impl` and `fn` scopes are filtered out by the
//           scope tracker.
//   Pass 2: re-scan and emit SymbolAccess{isWrite=true} for writes to
//           known globals inside function bodies. Writes include:
//             - `name = ...`
//             - `*name = ...` (raw pointer / static mut deref)
//             - `name += ...`, `name -= ...`, etc.
//             - `++name` / `name++` (rare in Rust but kept for symmetry)
//
// Reads are deferred to a later milestone — the load-bearing signal for
// PurityCheck is writes in parallel stages.

#include "RustSymbolAccessExtractor.h"

#include <cctype>
#include <fstream>
#include <regex>
#include <string>
#include <unordered_set>
#include <vector>

namespace topo::check {

namespace {

struct ScopeEntry {
    enum Kind { Fn, Impl, Mod };
    std::string name;
    int depth;
    Kind kind;
};

std::string buildCallerName(const std::vector<ScopeEntry>& scopeStack) {
    std::string result;
    for (const auto& s : scopeStack) {
        if (!result.empty()) result += "::";
        result += s.name;
    }
    return result;
}

bool isPreprocessorLine(const std::string& line) {
    auto pos = line.find_first_not_of(" \t");
    if (pos == std::string::npos) return false;
    return line[pos] == '#';
}

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
        if (c == '/' && i + 1 < line.size() && line[i + 1] == '/') break;
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
        }
    }
    return out;
}

/// Pass 1: collect module-level statics, mut statics, consts, and
/// thread-local statics. Items inside any fn or impl scope are
/// excluded by the scope tracker.
std::unordered_set<std::string> collectGlobals(const std::string& filePath) {
    std::unordered_set<std::string> globals;
    std::ifstream file(filePath);
    if (!file.is_open()) return globals;

    std::vector<ScopeEntry> scopeStack;
    int braceDepth = 0;
    bool inBlockComment = false;
    bool inThreadLocal = false;
    int threadLocalDepth = -1;

    static const std::regex fnRegex(
        R"((?:pub(?:\([\w:]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?(?:extern\s+"[^"]+"\s+)?fn\s+(\w+))");
    static const std::regex implRegex(
        R"(\bimpl\b(?:\s*<[^>]*>)?\s+(?:\w+\s+for\s+)?(\w+))");
    static const std::regex modRegex(R"(\bmod\s+(\w+)\s*\{)");

    // Module-scope global declarations.
    static const std::regex staticRegex(
        R"(^\s*(?:pub(?:\([\w:]+\))?\s+)?static\s+(?:mut\s+)?(\w+)\s*:)");
    static const std::regex constRegex(
        R"(^\s*(?:pub(?:\([\w:]+\))?\s+)?const\s+(\w+)\s*:)");
    // thread_local! { static NAME: TYPE = ...; }
    static const std::regex threadLocalOpenRegex(R"(\bthread_local!\s*\{)");
    static const std::regex threadLocalStaticRegex(
        R"(^\s*(?:pub(?:\([\w:]+\))?\s+)?static\s+(\w+)\s*:)");

    std::string line;
    std::string pendingFnName;

    while (std::getline(file, line)) {
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
        if (isPreprocessorLine(line)) continue;
        std::string effective = stripComments(line);
        if (effective.find_first_not_of(" \t") == std::string::npos) continue;
        std::string masked = maskStringLiterals(effective);

        // Allman-style pending fn opens body on this line.
        if (!pendingFnName.empty() && masked.find('{') != std::string::npos) {
            scopeStack.push_back({pendingFnName, braceDepth, ScopeEntry::Fn});
            pendingFnName.clear();
        }

        // Track braces. `peakBraceDepth` is the deepest level reached on
        // this line, so a fn whose body opens AND closes on this line is
        // recognised as single-line below and does not leak a never-closed
        // Fn scope entry (which would hide every following module static).
        int peakBraceDepth = braceDepth;
        for (char c : masked) {
            if (c == '{') {
                ++braceDepth;
                if (braceDepth > peakBraceDepth) peakBraceDepth = braceDepth;
            } else if (c == '}') {
                --braceDepth;
                if (braceDepth < 0) braceDepth = 0;
                if (inThreadLocal && braceDepth <= threadLocalDepth) {
                    inThreadLocal = false;
                    threadLocalDepth = -1;
                }
                while (!scopeStack.empty() && braceDepth <= scopeStack.back().depth) {
                    scopeStack.pop_back();
                }
            }
        }

        // Detect thread_local! opening on this line.
        if (std::regex_search(masked, threadLocalOpenRegex)) {
            inThreadLocal = true;
            threadLocalDepth = braceDepth - 1;
        }

        // Inside thread_local! we accept `static NAME:` lines as globals
        // regardless of any outer fn/impl scope (thread_local! can only
        // appear at module scope but the macro creates its own block).
        if (inThreadLocal) {
            std::smatch m;
            if (std::regex_search(masked, m, threadLocalStaticRegex)) {
                globals.insert(m[1].str());
            }
            continue;
        }

        // mod scope detection.
        std::smatch modMatch;
        if (std::regex_search(masked, modMatch, modRegex)) {
            scopeStack.push_back({modMatch[1].str(), braceDepth - 1, ScopeEntry::Mod});
            continue;
        }

        // impl scope detection.
        std::smatch implMatch;
        if (std::regex_search(masked, implMatch, implRegex)) {
            if (masked.find('{') != std::string::npos) {
                scopeStack.push_back({implMatch[1].str(), braceDepth - 1, ScopeEntry::Impl});
            }
            continue;
        }

        // fn detection — opens a function scope that disables global
        // collection on subsequent lines.
        std::smatch fnMatch;
        if (std::regex_search(masked, fnMatch, fnRegex)) {
            std::string fname = fnMatch[1].str();
            if (masked.find('{') != std::string::npos) {
                // A single-line `fn f() { ... }` body opens and closes on
                // this line; pushing it would leak a never-closed Fn entry.
                if (braceDepth > peakBraceDepth - 1) {
                    scopeStack.push_back({fname, peakBraceDepth - 1, ScopeEntry::Fn});
                }
            } else if (masked.find(';') == std::string::npos) {
                pendingFnName = fname;
            }
            continue;
        }

        // Outside fn/impl scope: accept module-level statics.
        bool inFnOrImpl = false;
        for (const auto& s : scopeStack) {
            if (s.kind == ScopeEntry::Fn || s.kind == ScopeEntry::Impl) {
                inFnOrImpl = true;
                break;
            }
        }
        if (inFnOrImpl) continue;

        std::smatch staticMatch;
        if (std::regex_search(masked, staticMatch, staticRegex)) {
            globals.insert(staticMatch[1].str());
            continue;
        }
        std::smatch constMatch;
        if (std::regex_search(masked, constMatch, constRegex)) {
            globals.insert(constMatch[1].str());
            continue;
        }
    }

    return globals;
}

} // anonymous namespace

std::vector<SymbolAccess> RustSymbolAccessExtractor::extractSymbolAccesses(const std::string& filePath) {
    std::vector<SymbolAccess> results;

    auto globals = collectGlobals(filePath);
    if (globals.empty()) return results;

    std::ifstream file(filePath);
    if (!file.is_open()) return results;

    std::vector<ScopeEntry> scopeStack;
    int braceDepth = 0;
    bool inFunction = false;
    int currentFunctionDepth = -1;
    std::string currentFunction;
    std::string pendingFnName;
    bool inBlockComment = false;

    static const std::regex fnRegex(
        R"((?:pub(?:\([\w:]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?(?:extern\s+"[^"]+"\s+)?fn\s+(\w+))");
    static const std::regex implRegex(
        R"(\bimpl\b(?:\s*<[^>]*>)?\s+(?:\w+\s+for\s+)?(\w+))");
    static const std::regex modRegex(R"(\bmod\s+(\w+)\s*\{)");

    std::string line;
    int lineNum = 0;

    while (std::getline(file, line)) {
        ++lineNum;

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
        if (isPreprocessorLine(line)) continue;

        std::string effective = stripComments(line);
        if (effective.find_first_not_of(" \t") == std::string::npos) continue;
        std::string masked = maskStringLiterals(effective);

        // `peakBraceDepth` is the deepest level reached on this line. fn
        // entry (Allman pending + inline) is detected AFTER the brace loop
        // and placed at `peakBraceDepth - 1`, so a single-line
        // `fn f() { ... }` whose closing `}` is on the same line is given
        // its correct entry depth. The previous code pushed the fn scope
        // with `braceDepth - 1` after the brace loop had already returned
        // braceDepth to 0, yielding depth -1 — a scope that can never be
        // popped, so `inFunction` leaked into every following fn and their
        // global writes were misattributed to the first single-line fn.
        int peakBraceDepth = braceDepth;
        bool enteredFnThisLine = false;
        bool deferredFnExit = false;

        for (char c : masked) {
            if (c == '{') {
                ++braceDepth;
                if (braceDepth > peakBraceDepth) peakBraceDepth = braceDepth;
            } else if (c == '}') {
                --braceDepth;
                if (braceDepth < 0) braceDepth = 0;
                while (!scopeStack.empty() && braceDepth <= scopeStack.back().depth) {
                    if (scopeStack.back().kind == ScopeEntry::Fn) {
                        inFunction = false;
                        currentFunction.clear();
                        currentFunctionDepth = -1;
                    }
                    scopeStack.pop_back();
                }
            }
        }

        // Allman-style pending fn body opens on this line.
        if (!pendingFnName.empty() && masked.find('{') != std::string::npos) {
            inFunction = true;
            currentFunction = pendingFnName;
            currentFunctionDepth = peakBraceDepth - 1;
            scopeStack.push_back({pendingFnName, peakBraceDepth - 1, ScopeEntry::Fn});
            pendingFnName.clear();
            enteredFnThisLine = true;
        }

        // mod scope detection.
        std::smatch modMatch;
        if (!inFunction && std::regex_search(masked, modMatch, modRegex)) {
            scopeStack.push_back({modMatch[1].str(), braceDepth - 1, ScopeEntry::Mod});
            continue;
        }

        // impl scope detection.
        std::smatch implMatch;
        if (!inFunction && std::regex_search(masked, implMatch, implRegex)) {
            if (masked.find('{') != std::string::npos) {
                scopeStack.push_back({implMatch[1].str(), braceDepth - 1, ScopeEntry::Impl});
            }
            continue;
        }

        // fn detection. No `continue` follows: a single-line
        // `fn f() { G += 1; }` body must still reach the write-scan below.
        std::smatch fnMatch;
        if (!inFunction && pendingFnName.empty() &&
            std::regex_search(masked, fnMatch, fnRegex)) {
            std::string fname = fnMatch[1].str();
            if (masked.find('{') != std::string::npos) {
                inFunction = true;
                currentFunction = fname;
                currentFunctionDepth = peakBraceDepth - 1;
                scopeStack.push_back({fname, peakBraceDepth - 1, ScopeEntry::Fn});
                enteredFnThisLine = true;
            } else if (masked.find(';') == std::string::npos) {
                pendingFnName = fname;
            }
        }

        // A fn whose body opens AND closes on this line: the running
        // braceDepth is already back at/below the entry depth. Scan the body
        // here, then schedule the scope-exit so `inFunction` does not leak.
        if (enteredFnThisLine && braceDepth <= currentFunctionDepth) {
            deferredFnExit = true;
        }

        // Only scan inside fn bodies. The exception is a body that opens on
        // this very line (`enteredFnThisLine`): its statements are here even
        // though braceDepth may have already returned to the entry depth.
        if (!inFunction ||
            (!enteredFnThisLine && braceDepth <= currentFunctionDepth)) {
            if (deferredFnExit) {
                inFunction = false;
                currentFunction.clear();
                currentFunctionDepth = -1;
                if (!scopeStack.empty() &&
                    scopeStack.back().kind == ScopeEntry::Fn) {
                    scopeStack.pop_back();
                }
            }
            continue;
        }

        std::string callerName = buildCallerName(scopeStack);

        // For each known global, look for write candidates on this line.
        for (const auto& name : globals) {
            size_t pos = 0;
            bool wroteOnce = false;
            while (pos < masked.size() && !wroteOnce) {
                size_t found = masked.find(name, pos);
                if (found == std::string::npos) break;

                // Word boundary before
                bool leftOK = true;
                bool isDeref = false;
                if (found > 0) {
                    char prev = masked[found - 1];
                    if (std::isalnum(static_cast<unsigned char>(prev)) || prev == '_') leftOK = false;
                    if (prev == '.') leftOK = false;
                    // `*X = ...` style write — leftOK still true; remember
                    // for clearer disambiguation later.
                    if (prev == '*') isDeref = true;
                }
                size_t end = found + name.size();
                bool rightOK = true;
                if (end < masked.size()) {
                    char nxt = masked[end];
                    if (std::isalnum(static_cast<unsigned char>(nxt)) || nxt == '_') rightOK = false;
                }

                if (!leftOK || !rightOK) {
                    pos = found + 1;
                    continue;
                }

                size_t after = end;
                while (after < masked.size() && (masked[after] == ' ' || masked[after] == '\t')) ++after;

                bool isWrite = false;
                if (after < masked.size()) {
                    char c = masked[after];
                    // `=` but not `==`
                    if (c == '=' && (after + 1 >= masked.size() || masked[after + 1] != '=')) {
                        isWrite = true;
                    }
                    // Compound: `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`,
                    // `^=`, `<<=`, `>>=`
                    if (!isWrite && after + 1 < masked.size() && masked[after + 1] == '=') {
                        if (c == '+' || c == '-' || c == '*' || c == '/' || c == '%' ||
                            c == '&' || c == '|' || c == '^') {
                            isWrite = true;
                        }
                    }
                    if (!isWrite && after + 2 < masked.size() && masked[after + 2] == '=') {
                        if ((c == '<' && masked[after + 1] == '<') ||
                            (c == '>' && masked[after + 1] == '>')) {
                            isWrite = true;
                        }
                    }
                    // Postfix ++/-- (rare in Rust but symmetric with C++).
                    if (!isWrite && after + 1 < masked.size()) {
                        if ((c == '+' && masked[after + 1] == '+') ||
                            (c == '-' && masked[after + 1] == '-')) {
                            isWrite = true;
                        }
                    }
                }

                // Prefix ++/-- (rare in Rust but symmetric).
                if (!isWrite && found >= 2) {
                    char p1 = masked[found - 1];
                    char p2 = masked[found - 2];
                    if ((p1 == '+' && p2 == '+') || (p1 == '-' && p2 == '-')) {
                        isWrite = true;
                    }
                }

                if (isWrite) {
                    SymbolAccess access;
                    access.function = callerName;
                    access.symbol = name;
                    access.isWrite = true;
                    access.file = filePath;
                    access.line = lineNum;
                    results.push_back(std::move(access));
                    wroteOnce = true;  // one write per global per line
                    (void)isDeref;
                    break;
                }

                pos = found + name.size();
            }
        }

        // Apply a deferred single-line fn scope-exit now that this line's
        // body has been scanned, so `inFunction` does not leak into the next
        // fn (which previously misattributed its global writes).
        if (deferredFnExit) {
            inFunction = false;
            currentFunction.clear();
            currentFunctionDepth = -1;
            if (!scopeStack.empty() && scopeStack.back().kind == ScopeEntry::Fn) {
                scopeStack.pop_back();
            }
        }
    }

    return results;
}

} // namespace topo::check
