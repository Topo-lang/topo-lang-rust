// RustStubGenerator — Stub function bodies in Rust source files.
//
// Strategy:
// 1. Search for `fn func_name` pattern, then skip optional generic params `<...>`
// 2. Skip past parameter list and optional return type to find '{'
// 3. Use brace-balancing to find the complete body
// 4. Replace with `{ }` (unit) or `{ Default::default() }` (typed return)

#include "RustStubGenerator.h"

#include <algorithm>
#include <fstream>
#include <sstream>

namespace topo::check {

namespace {

bool readFile(const std::string& path, std::string& content) {
    std::ifstream ifs(path, std::ios::binary);
    if (!ifs) return false;
    std::ostringstream ss;
    ss << ifs.rdbuf();
    content = ss.str();
    return true;
}

bool writeFile(const std::string& path, const std::string& content) {
    std::ofstream ofs(path, std::ios::binary | std::ios::trunc);
    if (!ofs) return false;
    ofs << content;
    return ofs.good();
}

} // anonymous namespace

size_t RustStubGenerator::findFunctionBodyStart(const std::string& source, const std::string& funcName) {
    // Search for `fn funcName` pattern — then accept either `(` or `<` after the name
    std::string pattern = "fn " + funcName;
    size_t searchStart = 0;

    while (searchStart < source.size()) {
        size_t pos = source.find(pattern, searchStart);
        if (pos == std::string::npos) return std::string::npos;

        // Verify word boundary before "fn"
        if (pos > 0) {
            char before = source[pos - 1];
            if (std::isalnum(static_cast<unsigned char>(before)) || before == '_') {
                searchStart = pos + pattern.size();
                continue;
            }
        }

        // Position right after the function name
        size_t afterName = pos + pattern.size();

        // Verify word boundary after funcName — next char must not be alnum/_
        if (afterName < source.size()) {
            char after = source[afterName];
            if (std::isalnum(static_cast<unsigned char>(after)) || after == '_') {
                searchStart = afterName;
                continue;
            }
        }

        // Skip optional generic parameters: <...>
        size_t cur = afterName;
        if (cur < source.size() && source[cur] == '<') {
            int angleDepth = 1;
            ++cur;
            while (cur < source.size() && angleDepth > 0) {
                if (source[cur] == '<')
                    ++angleDepth;
                else if (source[cur] == '>')
                    --angleDepth;
                ++cur;
            }
            if (angleDepth != 0) {
                searchStart = cur;
                continue;
            }
        }

        // Now expect '(' for the parameter list
        if (cur >= source.size() || source[cur] != '(') {
            searchStart = cur;
            continue;
        }

        // Find matching ')' for the parameter list
        size_t parenStart = cur;
        int depth = 1;
        size_t i = parenStart + 1;
        while (i < source.size() && depth > 0) {
            char c = source[i];
            if (c == '(')
                ++depth;
            else if (c == ')')
                --depth;
            else if (c == '"') {
                ++i;
                while (i < source.size() && source[i] != '"') {
                    if (source[i] == '\\') ++i;
                    ++i;
                }
            }
            ++i;
        }

        if (depth != 0) {
            searchStart = i;
            continue;
        }

        // Skip past optional return type `-> Type` and where clauses until '{'
        size_t afterParen = i;
        while (afterParen < source.size()) {
            char c = source[afterParen];
            if (c == '{') return afterParen;
            if (c == ';') break; // declaration without body
            ++afterParen;
        }

        searchStart = afterParen;
    }

    return std::string::npos;
}

size_t RustStubGenerator::findMatchingBrace(const std::string& source, size_t openPos) {
    if (openPos >= source.size() || source[openPos] != '{') return std::string::npos;

    int depth = 1;
    size_t i = openPos + 1;

    while (i < source.size() && depth > 0) {
        char c = source[i];

        // Handle string literals
        if (c == '"') {
            ++i;
            while (i < source.size() && source[i] != '"') {
                if (source[i] == '\\') ++i;
                ++i;
            }
            if (i < source.size()) ++i;
            continue;
        }

        // Handle line comments
        if (c == '/' && i + 1 < source.size() && source[i + 1] == '/') {
            while (i < source.size() && source[i] != '\n')
                ++i;
            continue;
        }

        // Handle block comments (Rust supports nested)
        if (c == '/' && i + 1 < source.size() && source[i + 1] == '*') {
            int commentDepth = 1;
            i += 2;
            while (i + 1 < source.size() && commentDepth > 0) {
                if (source[i] == '/' && source[i + 1] == '*') {
                    ++commentDepth;
                    ++i;
                } else if (source[i] == '*' && source[i + 1] == '/') {
                    --commentDepth;
                    ++i;
                }
                ++i;
            }
            continue;
        }

        if (c == '{')
            ++depth;
        else if (c == '}')
            --depth;

        ++i;
    }

    return (depth == 0) ? (i - 1) : std::string::npos;
}

bool RustStubGenerator::isUnitReturn(const std::string& source, size_t bodyStart) {
    // Look backwards from '{' for '->' to check return type.
    // If there's no '->' between ')' and '{', it's a unit return.
    // If '->' is followed by '()', it's also a unit return.

    // Search backwards from bodyStart for ')' (end of params)
    size_t searchStart = (bodyStart > 200) ? (bodyStart - 200) : 0;
    std::string region = source.substr(searchStart, bodyStart - searchStart);

    size_t arrowPos = region.rfind("->");
    if (arrowPos == std::string::npos) {
        return true; // no return type specified = unit
    }

    // Check what follows '->'
    size_t afterArrow = arrowPos + 2;
    while (afterArrow < region.size() && std::isspace(static_cast<unsigned char>(region[afterArrow])))
        ++afterArrow;

    // Check for "()" — explicit unit return
    if (afterArrow + 1 < region.size() && region[afterArrow] == '(' && region[afterArrow + 1] == ')') {
        return true;
    }

    return false;
}

StubResult RustStubGenerator::stubFunction(const std::string& filePath, const std::string& funcName) {
    StubResult result;

    if (!readFile(filePath, result.originalContent)) {
        result.error = "failed to read file: " + filePath;
        return result;
    }

    size_t bodyStart = findFunctionBodyStart(result.originalContent, funcName);
    if (bodyStart == std::string::npos) {
        result.error = "function '" + funcName + "' not found in " + filePath;
        return result;
    }

    size_t bodyEnd = findMatchingBrace(result.originalContent, bodyStart);
    if (bodyEnd == std::string::npos) {
        result.error = "unmatched brace for function '" + funcName + "' in " + filePath;
        return result;
    }

    // Determine stub body
    bool isUnit = isUnitReturn(result.originalContent, bodyStart);
    std::string stubBody = isUnit ? "{ }" : "{ Default::default() }";

    // Replace the body
    std::string modified =
        result.originalContent.substr(0, bodyStart) + stubBody + result.originalContent.substr(bodyEnd + 1);

    if (!writeFile(filePath, modified)) {
        result.error = "failed to write modified file: " + filePath;
        return result;
    }

    result.success = true;
    return result;
}

bool RustStubGenerator::restoreFile(const std::string& filePath, const StubResult& result) {
    if (result.originalContent.empty()) return false;
    return writeFile(filePath, result.originalContent);
}

} // namespace topo::check
