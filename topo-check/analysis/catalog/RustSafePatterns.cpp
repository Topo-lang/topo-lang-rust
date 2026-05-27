#include "RustSafePatterns.h"

#define TOML_HEADER_ONLY 1
#define TOML_EXCEPTIONS 0
#include <toml++/toml.hpp>

#include <filesystem>
#include <iostream>

namespace fs = std::filesystem;

namespace topo::check {

bool RustSafePatterns::load(const std::string& tomlPath) {
    toml::parse_result result = toml::parse_file(tomlPath);
    if (!result) {
        std::cerr << "RustSafePatterns: failed to parse " << tomlPath << ": "
                  << result.error() << "\n";
        return false;
    }
    const auto& tbl = result.table();

    // [constructs].safe
    if (auto arr = tbl.at_path("constructs.safe").as_array()) {
        for (const auto& elem : *arr) {
            if (auto s = elem.value<std::string>()) safeConstructs_.insert(*s);
        }
    }
    // [constructs].unsafe
    if (auto arr = tbl.at_path("constructs.unsafe").as_array()) {
        for (const auto& elem : *arr) {
            if (auto s = elem.value<std::string>()) unsafeConstructs_.insert(*s);
        }
    }
    // [stdlib].safe — array of qualified names
    if (auto arr = tbl.at_path("stdlib.safe").as_array()) {
        for (const auto& elem : *arr) {
            if (auto s = elem.value<std::string>()) safeStdlib_.insert(*s);
        }
    }

    loaded_ = true;
    return true;
}

bool RustSafePatterns::loadDefault() {
    // Try environment variable first
    if (const char* dir = std::getenv("TOPO_PATTERNS_DIR")) {
        fs::path p = fs::path(dir) / "RustSafePatterns.toml";
        if (fs::exists(p)) return load(p.string());
    }
    // For development, try the source tree location.
    // The catalog lives under topo-check/analysis/catalog/ in the current
    // layout (per the topo-<tool>/ subdirectory convention documented in
    // the project README); the legacy analysis/catalog/ path is kept as a
    // fallback so older checkouts still resolve.
    fs::path candidates[] = {
        fs::path(TOPO_SOURCE_DIR) / "topo-lang-rust" / "topo-check" / "analysis" / "catalog" / "RustSafePatterns.toml",
        fs::path(TOPO_SOURCE_DIR) / "topo-lang-rust" / "analysis" / "catalog" / "RustSafePatterns.toml",
    };
    for (const auto& p : candidates) {
        if (fs::exists(p)) return load(p.string());
    }
    return false;
}

bool RustSafePatterns::isConstructSafe(const std::string& keyword) const {
    return safeConstructs_.count(keyword) > 0;
}

bool RustSafePatterns::isConstructUnsafe(const std::string& keyword) const {
    return unsafeConstructs_.count(keyword) > 0;
}

bool RustSafePatterns::isStdlibSymbolSafe(const std::string& qualifiedName) const {
    // Exact match first
    if (safeStdlib_.count(qualifiedName)) return true;
    // Prefix match: "std::vec::Vec::push" -> check "std::vec::Vec"
    // Members of safe types are safe
    auto pos = qualifiedName.rfind("::");
    while (pos != std::string::npos && pos > 0) {
        std::string prefix = qualifiedName.substr(0, pos);
        if (safeStdlib_.count(prefix)) return true;
        pos = prefix.rfind("::");
    }
    return false;
}

} // namespace topo::check
