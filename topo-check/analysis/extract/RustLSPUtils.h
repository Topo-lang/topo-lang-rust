#ifndef TOPO_CHECK_RUSTLSPUTILS_H
#define TOPO_CHECK_RUSTLSPUTILS_H

#include <cctype>
#include <string>

namespace topo::check {

/// Extract qualified name from rust-analyzer hover markdown.
///
/// Input examples:
///   "fn std::fs::read_to_string(path: impl AsRef<Path>) -> Result<String>"
///       -> "std::fs::read_to_string"
///   "struct std::vec::Vec<T>"           -> "std::vec::Vec"
///   "mod std::io"                       -> "std::io"
///   "enum std::option::Option<T>"       -> "std::option::Option"
///   "fn process(data: &[u8]) -> bool"   -> "process"
///   "pub fn mymod::helper(x: i32)"      -> "mymod::helper"
///
/// For functions (with parens), extracts the qualified name before the
/// opening paren. For types/mods (without parens), extracts the last
/// qualified name segment.
///
/// Rust uses :: as path separator natively -- no conversion needed.
inline std::string extractQualifiedName(const std::string& hover) {
    // Try function-style first: find opening paren of parameter list
    auto parenPos = hover.find('(');
    if (parenPos != std::string::npos) {
        // Work backwards from the paren to find the function name
        // Skip whitespace before paren
        size_t end = parenPos;
        while (end > 0 && hover[end - 1] == ' ') --end;
        if (end == 0) return "";

        // Scan backwards for the qualified name (alphanumeric, ::, _)
        size_t start = end;
        while (start > 0) {
            char c = hover[start - 1];
            if (std::isalnum(static_cast<unsigned char>(c)) || c == '_' || c == ':') {
                --start;
            } else {
                break;
            }
        }

        std::string name = hover.substr(start, end - start);
        // Strip leading ::
        if (name.size() >= 2 && name[0] == ':' && name[1] == ':') {
            name = name.substr(2);
        }
        return name;
    }

    // Type-style hover: "struct ns::Name<T>", "mod ns::Name", "enum ns::Name"
    // Find the qualified name (before any generic params <...>)
    size_t end = hover.size();

    // Trim trailing whitespace
    while (end > 0 && std::isspace(static_cast<unsigned char>(hover[end - 1]))) --end;
    if (end == 0) return "";

    // Strip trailing generic parameters <...>
    if (end > 0 && hover[end - 1] == '>') {
        int depth = 1;
        --end;
        while (end > 0 && depth > 0) {
            --end;
            if (hover[end] == '>') ++depth;
            else if (hover[end] == '<') --depth;
        }
        // end now points at '<', trim whitespace before it
        while (end > 0 && std::isspace(static_cast<unsigned char>(hover[end - 1]))) --end;
    }

    // Scan backwards for the qualified name
    size_t nameEnd = end;
    size_t nameStart = nameEnd;
    while (nameStart > 0) {
        char c = hover[nameStart - 1];
        if (std::isalnum(static_cast<unsigned char>(c)) || c == '_' || c == ':') {
            --nameStart;
        } else {
            break;
        }
    }

    if (nameStart == nameEnd) return "";

    std::string name = hover.substr(nameStart, nameEnd - nameStart);
    // Strip leading ::
    if (name.size() >= 2 && name[0] == ':' && name[1] == ':') {
        name = name.substr(2);
    }
    return name;
}

/// Determine whether a semantic token modifier string contains the given modifier.
/// Modifier strings from rust-analyzer are comma-separated, e.g. "declaration,readonly".
inline bool hasModifier(const std::string& modifiers, const std::string& modifier) {
    return modifiers.find(modifier) != std::string::npos;
}

} // namespace topo::check

#endif // TOPO_CHECK_RUSTLSPUTILS_H
