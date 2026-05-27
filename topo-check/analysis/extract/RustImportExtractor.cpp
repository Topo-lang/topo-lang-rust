// RustImportExtractor — L1 regex-based import extraction from Rust source files.
//
// Strategy: line scanning for `use` declarations and `extern crate` statements.
// Extracts the top-level crate/module path and classifies using RustUnsafeCatalog.
//
// This is a SAFETY NET — false positives are acceptable, false negatives are bugs.

#include "RustImportExtractor.h"
#include "RustUnsafeCatalog.h"

#include <fstream>
#include <regex>
#include <string>

namespace topo::check {

namespace {

/// Extract the top-level module path from a Rust `use` path.
/// For "std::fs::File" → "std::fs"
/// For "std::io" → "std::io"
/// For "tokio::io" → "tokio"
/// For a single identifier "serde" → "serde"
std::string extractTopLevelPath(const std::string& fullPath) {
    // For std:: paths, keep two levels (std::fs, std::io, std::net, std::process)
    // to match the RustUnsafeCatalog classification granularity.
    if (fullPath.substr(0, 5) == "std::") {
        // Find the second :: separator
        auto secondSep = fullPath.find("::", 5);
        if (secondSep != std::string::npos) {
            return fullPath.substr(0, secondSep);
        }
        return fullPath;
    }

    // For non-std paths, the crate name is the first segment
    auto firstSep = fullPath.find("::");
    if (firstSep != std::string::npos) {
        return fullPath.substr(0, firstSep);
    }
    return fullPath;
}

/// Check if a line is entirely a comment or whitespace.
bool isCommentOrEmpty(const std::string& line) {
    auto pos = line.find_first_not_of(" \t");
    if (pos == std::string::npos) return true;
    return (line.size() > pos + 1 && line[pos] == '/' && line[pos + 1] == '/');
}

} // anonymous namespace

std::vector<HostImport> RustImportExtractor::extractImports(const std::string& filePath) {
    std::vector<HostImport> results;

    std::ifstream file(filePath);
    if (!file.is_open()) return results;

    // Regex patterns — compiled once
    // use path::to::module; or use path::to::module::{...};
    // Also handles: use path::to::module as alias;
    static const std::regex useRegex(R"(^\s*(?:pub(?:\([\w:]+\))?\s+)?use\s+([\w:]+))");
    // extern crate name;
    static const std::regex externCrateRegex(R"(^\s*extern\s+crate\s+(\w+))");

    // Block comment state
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
                // After */ could have code, but use statements start at line
                // beginning (anchored with ^), so it would not match.
            }
            continue;
        }

        // Check for block comment openings
        {
            bool skipLine = false;
            for (size_t i = 0; i < line.size(); ++i) {
                char c = line[i];
                // Line comment — stop scanning
                if (c == '/' && i + 1 < line.size() && line[i + 1] == '/') break;
                // Block comment start
                if (c == '/' && i + 1 < line.size() && line[i + 1] == '*') {
                    auto closePos = line.find("*/", i + 2);
                    if (closePos != std::string::npos) {
                        // Same-line block comment — skip past it
                        i = closePos + 1;
                        continue;
                    }
                    // Multiline block comment
                    inBlockComment = true;
                    skipLine = true;
                    break;
                }
                // Raw strings (r#"..."#) are skipped — "use" statements
                // inside raw strings are extremely unlikely, and the ^
                // anchor on the regex prevents false matches.
            }
            if (skipLine) continue;
        }

        if (isCommentOrEmpty(line)) continue;

        // Match use declarations
        std::smatch useMatch;
        if (std::regex_search(line, useMatch, useRegex)) {
            std::string fullPath = useMatch[1].str();

            // Normalize: Rust paths use :: already, keep as-is
            std::string topLevel = extractTopLevelPath(fullPath);

            HostImport imp;
            imp.normalizedPath = topLevel;
            imp.file = filePath;
            imp.line = lineNum;
            imp.unsafeLevel = RustUnsafeCatalog::classifyImport(topLevel);
            results.push_back(std::move(imp));

            // If the full path has more specificity, also record it
            // for better classification (e.g., use std::fs::File gets
            // std::fs which classifies as System)
            if (topLevel != fullPath) {
                // Check if the full path classifies differently
                auto fullLevel = RustUnsafeCatalog::classifyImport(fullPath);
                if (fullLevel != imp.unsafeLevel && fullLevel != UnsafeLevel::Safe) {
                    HostImport fullImp;
                    fullImp.normalizedPath = fullPath;
                    fullImp.file = filePath;
                    fullImp.line = lineNum;
                    fullImp.unsafeLevel = fullLevel;
                    results.push_back(std::move(fullImp));
                }
            }
            continue;
        }

        // Match extern crate declarations
        std::smatch crateMatch;
        if (std::regex_search(line, crateMatch, externCrateRegex)) {
            std::string crateName = crateMatch[1].str();

            HostImport imp;
            imp.normalizedPath = crateName;
            imp.file = filePath;
            imp.line = lineNum;
            imp.unsafeLevel = RustUnsafeCatalog::classifyImport(crateName);
            results.push_back(std::move(imp));
        }
    }

    return results;
}

} // namespace topo::check
