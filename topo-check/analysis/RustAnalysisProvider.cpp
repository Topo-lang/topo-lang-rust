#include "RustAnalysisProvider.h"
#include "RustCallEdgeExtractor.h"
#include "RustCallSiteExtractor.h"
#include "RustSymbolAccessExtractor.h"
#include "RustSymbolExtractor.h"
#include "RustImportExtractor.h"
#include "RustLSPCallSiteExtractor.h"
#include "RustSafePatterns.h"
#include "RustSafetyAnalyzer.h"
#include "RustAnalyzerBridge.h"

#include <algorithm>
#include <filesystem>
#include <iostream>
#include <set>

namespace fs = std::filesystem;

namespace topo::check {

RustAnalysisProvider::~RustAnalysisProvider() {
    shutdownLSP();
}

std::unique_ptr<SymbolExtractor> RustAnalysisProvider::createSymbolExtractor() {
    // Always use regex extractor for L1 symbol extraction.
    // rust-analyzer may respond to workspace/symbol probes before semantic tokens
    // are ready, making isLSPReady() unreliable for this purpose.
    // L2 deep containment manages its own LSP lifecycle in runDeepContainment().
    return std::make_unique<RustSymbolExtractor>();
}

std::unique_ptr<ImportExtractor> RustAnalysisProvider::createImportExtractor() {
    return std::make_unique<RustImportExtractor>();
}

std::unique_ptr<CallSiteExtractor> RustAnalysisProvider::createCallSiteExtractor() {
    return std::make_unique<RustCallSiteExtractor>();
}

std::unique_ptr<CallEdgeExtractor> RustAnalysisProvider::createCallEdgeExtractor() {
    // Call-edge extraction is L1-only (regex over source). L2 deep
    // containment (runDeepContainment below) is the only path that
    // engages rust-analyzer; no L1/L2 merging happens for call edges.
    return std::make_unique<RustCallEdgeExtractor>();
}

std::unique_ptr<SymbolAccessExtractor> RustAnalysisProvider::createSymbolAccessExtractor() {
    return std::make_unique<RustSymbolAccessExtractor>();
}

std::vector<std::string> RustAnalysisProvider::collectSourceFiles(
    const std::string& projectDir,
    const std::vector<std::string>& /*includeDirs*/) const {
    std::vector<std::string> files;
    std::vector<fs::path> searchDirs = {
        fs::path(projectDir) / "src",
        fs::path(projectDir)};
    // Path-segment names that are never user source: cargo's build
    // tree (incl. build-script-generated .rs files inside
    // target/debug/build/<crate>-<hash>/out/), Topo build trees,
    // vendor / git / venv. Matching by leaf name keeps the check
    // O(1) per directory and works whether the directory sits at
    // the project root or nested under it.
    static const std::set<std::string> kSkipDirs{
        "target", "build", "build-no-llvm",
        "node_modules", "vendor", ".git", ".venv", "__pycache__"};
    std::set<std::string> seen;
    // `directory_options::skip_permission_denied` keeps the iterator
    // moving past unreadable subtrees; symlinks are NOT followed so a
    // symlink loop inside the project cannot hang the checker. The
    // skip is done with `disable_recursion_pending()` once we enter a
    // forbidden directory.
    for (const auto& dir : searchDirs) {
        if (!fs::exists(dir)) continue;
        std::error_code ec;
        fs::recursive_directory_iterator it(
            dir,
            fs::directory_options::skip_permission_denied,
            ec);
        if (ec) continue;
        for (; it != fs::recursive_directory_iterator(); it.increment(ec)) {
            if (ec) { ec.clear(); continue; }
            const auto& entry = *it;
            // Skip forbidden directories wholesale. The iterator
            // already yields the directory entry itself, so we
            // suppress descent before the children are visited.
            if (entry.is_directory(ec) && kSkipDirs.count(entry.path().filename().string())) {
                it.disable_recursion_pending();
                continue;
            }
            // Do not follow symlinks (loop guard).
            if (entry.is_symlink(ec)) {
                if (entry.is_directory(ec)) it.disable_recursion_pending();
                continue;
            }
            if (entry.path().extension() == ".rs") {
                std::string path = entry.path().string();
                if (seen.insert(path).second)
                    files.push_back(path);
            }
        }
    }
    std::sort(files.begin(), files.end());
    return files;
}

bool RustAnalysisProvider::initLSP(const std::string& projectDir, bool verbose) {
    if (bridge_ && bridge_->isAvailable()) return true;

    bridge_ = std::make_unique<lsp::RustAnalyzerBridge>();
    std::string rootUri = "file://" + fs::canonical(projectDir).string();

    if (!bridge_->start("", rootUri)) {
        bridge_.reset();
        return false;
    }

    if (!bridge_->waitForIndex(std::chrono::milliseconds{30000})) {
        std::cerr << "[topo-lsp] rust-analyzer index not ready after 30s, analysis may be incomplete\n";
    }

    if (verbose) {
        std::cerr << "  RustAnalyzerBridge started\n";
    }
    return true;
}

void RustAnalysisProvider::shutdownLSP() {
    if (bridge_) {
        bridge_->stop();
        bridge_.reset();
    }
}

bool RustAnalysisProvider::isLSPReady() const {
    return bridge_ && bridge_->isAvailable();
}

std::optional<CheckResult> RustAnalysisProvider::runDeepContainment(
    const SymbolTable& symbols,
    const std::vector<std::string>& sourceFiles,
    const ContainmentConfig& config,
    const std::string& projectDir,
    bool verbose) {
    CheckResult result;

    RustSafePatterns patterns;
    if (!patterns.loadDefault()) {
        CheckDiagnostic d;
        d.severity = Severity::Warning;
        d.check = "containment-l2";
        d.message = "RustSafePatterns.toml not found — cannot run L2 analysis";
        result.addDiagnostic(std::move(d));
        return result;
    }

    if (!bridge_ || !bridge_->isAvailable()) {
        initLSP(projectDir, verbose);
    }
    if (!bridge_ || !bridge_->isAvailable()) {
        CheckDiagnostic d;
        d.severity = Severity::Warning;
        d.check = "containment-l2";
        d.message = "rust-analyzer unavailable — falling back to L1";
        result.addDiagnostic(std::move(d));
        return result;
    }

    RustSafetyAnalyzer analyzer(*bridge_, patterns);
    result = analyzer.analyze(symbols, sourceFiles, config);
    return result;
}

std::unique_ptr<LanguageAnalysisProvider> createRustAnalysisProvider() {
    return std::unique_ptr<LanguageAnalysisProvider>(new RustAnalysisProvider());
}

} // namespace topo::check
