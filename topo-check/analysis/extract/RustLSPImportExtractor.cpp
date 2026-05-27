// RustLSPImportExtractor -- Clean line-based Rust import extraction.
//
// `use` and `extern crate` are deterministic syntax (no need for LSP).
// This extractor uses direct string matching with block-comment and
// raw-string state tracking.

#include "RustLSPImportExtractor.h"
#include "RustUnsafeCatalog.h"

#include <cctype>
#include <cstring>
#include <fstream>
#include <string>

namespace topo::check {

namespace {

/// Skip leading whitespace, return position of first non-whitespace.
size_t skipWs(const std::string& line, size_t pos = 0) {
    while (pos < line.size() && (line[pos] == ' ' || line[pos] == '\t')) ++pos;
    return pos;
}

/// Check if text at pos matches the given keyword followed by whitespace.
bool matchKeyword(const std::string& line, size_t pos, const char* keyword) {
    size_t len = std::strlen(keyword);
    if (pos + len > line.size()) return false;
    if (line.compare(pos, len, keyword) != 0) return false;
    // Must be followed by whitespace or end of relevant content
    if (pos + len < line.size()) {
        char next = line[pos + len];
        if (next != ' ' && next != '\t') return false;
    }
    return true;
}

/// Extract the crate/module path from a use statement.
/// Reads identifier characters and :: separators starting at pos.
/// Stops at whitespace, ;, {, *, or ::{ patterns.
std::string extractPath(const std::string& line, size_t pos) {
    std::string path;
    while (pos < line.size()) {
        char c = line[pos];
        if (std::isalnum(static_cast<unsigned char>(c)) || c == '_') {
            path += c;
            ++pos;
        } else if (c == ':' && pos + 1 < line.size() && line[pos + 1] == ':') {
            // Check if next after :: is { or * (group import / glob)
            size_t afterSep = pos + 2;
            if (afterSep < line.size() && (line[afterSep] == '{' || line[afterSep] == '*')) {
                break; // path ends here, the :: belongs to group syntax
            }
            path += "::";
            pos += 2;
        } else {
            break;
        }
    }
    return path;
}

/// Extract the module prefix from a full path.
/// "std::fs::File" -> "std::fs" (uppercase last segment = type, drop it)
/// "std::net" -> "std::net" (lowercase last segment = module, keep it)
std::string modulePrefix(const std::string& path) {
    auto lastSep = path.rfind("::");
    if (lastSep == std::string::npos) return path;

    std::string tail = path.substr(lastSep + 2);
    if (!tail.empty() && std::isupper(static_cast<unsigned char>(tail[0]))) {
        return path.substr(0, lastSep);
    }
    return path;
}

} // anonymous namespace

std::vector<HostImport> RustLSPImportExtractor::extractImports(const std::string& filePath) {
    std::vector<HostImport> results;
    std::ifstream file(filePath);
    if (!file.is_open()) return results;

    std::string line;
    int lineNum = 0;
    bool inBlockComment = false;

    while (std::getline(file, line)) {
        ++lineNum;

        // --- State machine: block comment tracking ---
        if (inBlockComment) {
            auto closePos = line.find("*/");
            if (closePos != std::string::npos) {
                inBlockComment = false;
                line = line.substr(closePos + 2);
            } else {
                continue;
            }
        }

        // Check for block comment start
        {
            auto startPos = line.find("/*");
            if (startPos != std::string::npos) {
                auto endPos = line.find("*/", startPos + 2);
                if (endPos != std::string::npos) {
                    // Same-line block comment: remove it
                    line = line.substr(0, startPos) + line.substr(endPos + 2);
                } else {
                    inBlockComment = true;
                    line = line.substr(0, startPos);
                }
            }
        }

        // Strip line comments
        {
            auto commentPos = line.find("//");
            if (commentPos != std::string::npos) {
                line = line.substr(0, commentPos);
            }
        }

        // Skip blank lines
        size_t pos = skipWs(line);
        if (pos >= line.size()) continue;

        // Try "pub use ..." or "use ..."
        if (matchKeyword(line, pos, "pub")) {
            pos = skipWs(line, pos + 3);
        }

        if (matchKeyword(line, pos, "use")) {
            pos = skipWs(line, pos + 3);
            std::string path = extractPath(line, pos);
            if (!path.empty()) {
                // Check if this is a group import (path followed by ::{)
                size_t afterPath = pos + path.size();
                // For group imports, keep the prefix as-is
                // For regular imports, extract the module prefix
                std::string normalizedPath;
                if (afterPath + 2 <= line.size() &&
                    line[afterPath] == ':' && line[afterPath + 1] == ':' &&
                    afterPath + 2 < line.size() && line[afterPath + 2] == '{') {
                    normalizedPath = path;
                } else {
                    normalizedPath = modulePrefix(path);
                }

                HostImport imp;
                imp.normalizedPath = normalizedPath;
                imp.file = filePath;
                imp.line = lineNum;
                imp.unsafeLevel = RustUnsafeCatalog::classifyImport(imp.normalizedPath);
                results.push_back(std::move(imp));
            }
            continue;
        }

        // Try "extern crate ..."
        if (matchKeyword(line, pos, "extern")) {
            pos = skipWs(line, pos + 6);
            if (matchKeyword(line, pos, "crate")) {
                pos = skipWs(line, pos + 5);
                // Extract crate name (single identifier)
                std::string crateName;
                while (pos < line.size() &&
                       (std::isalnum(static_cast<unsigned char>(line[pos])) || line[pos] == '_')) {
                    crateName += line[pos];
                    ++pos;
                }
                if (!crateName.empty()) {
                    HostImport imp;
                    imp.normalizedPath = crateName;
                    imp.file = filePath;
                    imp.line = lineNum;
                    imp.unsafeLevel = RustUnsafeCatalog::classifyImport(imp.normalizedPath);
                    results.push_back(std::move(imp));
                }
            }
        }
    }

    return results;
}

std::vector<HostImport> RustLSPImportExtractor::extractAll(const std::vector<std::string>& files) {
    std::vector<HostImport> results;
    for (const auto& f : files) {
        auto imports = extractImports(f);
        results.insert(results.end(),
                       std::make_move_iterator(imports.begin()),
                       std::make_move_iterator(imports.end()));
    }
    return results;
}

} // namespace topo::check
