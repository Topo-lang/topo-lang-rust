// RustSafetyAnalyzer — L2 whitelist-based containment analysis engine.
//
// Uses rust-analyzer semantic tokens to find call sites, resolves each target
// via hover, and checks qualified names against RustSafePatterns whitelist.
// Non-whitelisted, non-.topo-declared calls are fed through the standard
// checkContainment() pipeline for external/non-external filtering.

#include "RustSafetyAnalyzer.h"
#include "RustAnalyzerBridge.h"
#include "RustUnsafeCatalog.h"

#include <cctype>
#include <set>
#include <string>

namespace {

/// Extract qualified function name from rust-analyzer hover markdown.
/// rust-analyzer returns markdown like:
///   "```rust\npub fn std::vec::Vec::push(&mut self, value: T)\n```"
///   "```rust\nfn mymod::process(data: &[u8]) -> Result<(), Error>\n```"
///   "```rust\npub fn foo::bar()\n```"
///
/// Rust uses `::` natively, so no path separator conversion needed.
std::string extractQualifiedName(const std::string& hover) {
    // Look for content inside ```rust ... ``` blocks
    std::string content = hover;
    auto codeStart = hover.find("```rust\n");
    if (codeStart != std::string::npos) {
        codeStart += 8; // skip "```rust\n"
        auto codeEnd = hover.find("\n```", codeStart);
        if (codeEnd != std::string::npos) {
            content = hover.substr(codeStart, codeEnd - codeStart);
        }
    }

    // Find the opening paren of the parameter list
    auto parenPos = content.find('(');
    if (parenPos == std::string::npos) return "";

    // Work backwards from the paren to find the function name
    // Skip whitespace before paren
    size_t end = parenPos;
    while (end > 0 && content[end - 1] == ' ') --end;
    if (end == 0) return "";

    // Scan backwards for the qualified name (alphanumeric, ::, _, <, >)
    size_t start = end;
    while (start > 0) {
        char c = content[start - 1];
        if (std::isalnum(static_cast<unsigned char>(c)) || c == '_' || c == ':') {
            --start;
        } else {
            break;
        }
    }

    std::string name = content.substr(start, end - start);
    // Strip leading ::
    if (name.size() >= 2 && name[0] == ':' && name[1] == ':') {
        name = name.substr(2);
    }
    return name;
}

} // anonymous namespace

namespace topo::check {

RustSafetyAnalyzer::RustSafetyAnalyzer(lsp::RustAnalyzerBridge& bridge, const RustSafePatterns& patterns)
    : bridge_(bridge), patterns_(patterns) {}

CheckResult RustSafetyAnalyzer::analyze(const SymbolTable& symbols,
                                        const std::vector<std::string>& sourceFiles,
                                        const ContainmentConfig& config) {
    CheckResult result;
    if (!config.isEnabled()) return result;
    if (!bridge_.isAvailable()) {
        CheckDiagnostic d;
        d.severity = Severity::Warning;
        d.check = "containment-l2";
        d.message = "rust-analyzer unavailable — falling back to L1 regex scanning";
        result.addDiagnostic(std::move(d));
        return result;
    }

    // Collect all L2-detected call sites, then run through checkContainment.
    // L2 does not re-check imports (L1 handles that).
    std::vector<DetectedCallSite> callSites;
    std::vector<HostImport> imports;

    int filesWithEmptyTokens = 0;
    for (const auto& file : sourceFiles) {
        if (!analyzeFile(file, symbols, config, callSites)) {
            ++filesWithEmptyTokens;
        }
    }

    // Principle 16: if rust-analyzer produced no tokens for any file, do not
    // pretend L2 ran. Surface a loud warning and let CheckRunner fall through
    // to L1.
    if (!sourceFiles.empty() && filesWithEmptyTokens == static_cast<int>(sourceFiles.size())) {
        CheckDiagnostic d;
        d.severity = Severity::Warning;
        d.check = "containment-l2";
        d.message = "rust-analyzer returned no semantic tokens for any of " +
                    std::to_string(sourceFiles.size()) +
                    " source file(s) — L2 cannot run, falling back to L1";
        result.addDiagnostic(std::move(d));
        return result;
    }

    // No preprocessor scanning for Rust (no macro preprocessor like C/C++).

    // Deduplicate: same file+line call sites
    {
        std::set<std::pair<std::string, int>> seen;
        std::vector<DetectedCallSite> deduped;
        for (auto& site : callSites) {
            auto key = std::make_pair(site.file + "::" + site.calleePattern, site.line);
            if (seen.insert(key).second) {
                deduped.push_back(std::move(site));
            }
        }
        callSites = std::move(deduped);
    }

    // Use the standard containment check with L2-resolved call sites
    checkContainment(symbols, imports, callSites, config, result);

    // Surface partial-extraction warning if some (but not all) files lacked tokens.
    if (filesWithEmptyTokens > 0) {
        CheckDiagnostic d;
        d.severity = Severity::Warning;
        d.check = "containment-l2";
        d.message = "rust-analyzer returned no semantic tokens for " +
                    std::to_string(filesWithEmptyTokens) + " of " +
                    std::to_string(sourceFiles.size()) +
                    " source file(s) — those files were not analyzed at L2";
        result.addDiagnostic(std::move(d));
    }

    // Mark as real L2 result so CheckRunner does not fall through to L1.
    // The marker carries honest extraction stats so users can distinguish
    // "L2 ran cleanly" from "L2 ran but found nothing because LSP failed".
    {
        CheckDiagnostic d;
        d.severity = Severity::Note;
        d.check = "containment";
        d.message = "L2 deep analysis completed (" +
                    std::to_string(static_cast<int>(sourceFiles.size()) - filesWithEmptyTokens) +
                    "/" + std::to_string(sourceFiles.size()) + " file(s), " +
                    std::to_string(callSites.size()) + " call site(s))";
        result.addDiagnostic(std::move(d));
    }

    return result;
}

bool RustSafetyAnalyzer::analyzeFile(const std::string& filePath,
                                      const SymbolTable& symbols,
                                      const ContainmentConfig& /*config*/,
                                      std::vector<DetectedCallSite>& callSites) {
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
        return false;
    }

    // Fetch the document outline once so every call site can be attributed
    // to its real enclosing function. Without this the synthetic
    // `<l2:file:line>` placeholder breaks isExternalCaller() for every L2
    // call site (synthetic caller attribution would otherwise be wrong).
    auto docSymbols = bridge_.getDocumentSymbols(filePath);

    // 3. For each function/method call token, resolve and check
    for (const auto& token : tokens) {
        // Only interested in function/method references (not declarations/definitions)
        if (token.type != "function" && token.type != "method") continue;
        if (token.modifiers.find("declaration") != std::string::npos ||
            token.modifiers.find("definition") != std::string::npos) continue;

        // Resolve the call target via hover
        auto hover = bridge_.getHoverAt(filePath, token.line, token.column);
        if (!hover) continue;

        // Extract qualified name from hover response
        std::string qualifiedName = extractQualifiedName(*hover);
        if (qualifiedName.empty()) continue;

        // Check if this is a safe stdlib call
        if (patterns_.isStdlibSymbolSafe(qualifiedName)) continue;

        // Check if this is a .topo-declared function (project code calling project code is OK)
        bool isDeclared = false;
        for (const auto& [name, fn] : symbols.functions()) {
            if (fn.qualifiedName == qualifiedName || fn.simpleName == qualifiedName) {
                isDeclared = true;
                break;
            }
        }
        if (isDeclared) continue;

        // Resolve the enclosing function via documentSymbol so external
        // functions are recognized by isExternalCaller(). The synthetic
        // placeholder survives only as a last-resort fallback when the
        // outline is empty.
        std::string callerQN = lsp::LSPBridge::findEnclosingFunction(
            docSymbols, token.line, "::");
        if (callerQN.empty()) {
            callerQN = "<l2:" + filePath + ":" +
                       std::to_string(token.line + 1) + ">";
        }

        // Not in whitelist and not .topo-declared -> report as unsafe
        DetectedCallSite site;
        site.calleePattern = qualifiedName;
        site.callerQualifiedName = callerQN;
        site.capability = std::nullopt;
        // Classify via RustUnsafeCatalog; default to System for unknown external calls
        auto catalogLevel = RustUnsafeCatalog::classifyCall(qualifiedName);
        site.unsafeLevel = (catalogLevel != UnsafeLevel::Safe) ? catalogLevel : UnsafeLevel::System;
        site.file = filePath;
        site.line = token.line + 1;  // semantic tokens are 0-based, diagnostics are 1-based
        callSites.push_back(std::move(site));
    }

    return true;
}

} // namespace topo::check
