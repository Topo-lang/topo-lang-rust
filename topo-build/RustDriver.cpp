#include "RustDriver.h"

#include "topo/Platform/Platform.h"
#include "topo/Platform/Process.h"
#include "topo/Platform/ToolResolution.h"

// toml++ as header-only, no-exception mode (LLVM is built with -fno-rtti)
#define TOML_HEADER_ONLY 1
#define TOML_EXCEPTIONS 0
#include <toml++/toml.hpp>

#include "topo/Platform/SharedLibrary.h"

#include <array>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <sstream>
#include <vector>

namespace fs = std::filesystem;

namespace topo::build {

/// Resolve the Cargo target directory for the given project.
/// Checks CARGO_TARGET_DIR env var first, falls back to projectDir/"target".
static fs::path resolveCargoTargetDir(const fs::path& projectDir) {
    if (const char* envDir = std::getenv("CARGO_TARGET_DIR")) {
        fs::path p(envDir);
        if (!p.empty()) return p;
    }
    return projectDir / "target";
}

/// Check LLVM version compatibility between rustc and the project's LLVM.
static void checkRustcLLVMVersion(bool verbose) {
    namespace plat = platform;

    auto result = plat::runProcessCapture("rustc", {"--version", "--verbose"}, verbose);
    if (result.exitCode != 0) {
        std::cerr << "warning: could not query rustc version\n";
        return;
    }

    std::string rustcLLVM;
    {
        std::istringstream iss(result.stdoutOutput);
        std::string line;
        while (std::getline(iss, line)) {
            auto pos = line.find("LLVM version:");
            if (pos != std::string::npos) {
                rustcLLVM = line.substr(pos + 14);
                while (!rustcLLVM.empty() && rustcLLVM[0] == ' ')
                    rustcLLVM.erase(rustcLLVM.begin());
                break;
            }
        }
    }

    if (rustcLLVM.empty()) {
        std::cerr << "warning: could not determine rustc LLVM version\n";
        return;
    }

    int rustcMajor = 0;
    try {
        rustcMajor = std::stoi(rustcLLVM);
    } catch (...) {
        std::cerr << "warning: could not parse rustc LLVM version '" << rustcLLVM << "'\n";
        return;
    }

    // Read project LLVM version from .llvm-version file
    int projectMajor = 0;
    {
        // Search for .llvm-version relative to executable or project dir
        std::vector<fs::path> searchPaths = {
            fs::current_path() / "topo-llvm" / ".llvm-version",
            fs::current_path() / ".llvm-version",
        };
        // Also try relative to executable directory
        std::string exeDir = topo::platform::getExecutableDir();
        if (!exeDir.empty()) {
            searchPaths.insert(searchPaths.begin(), fs::path(exeDir).parent_path() / "topo-llvm" / ".llvm-version");
        }
        for (const auto& p : searchPaths) {
            if (fs::exists(p)) {
                std::ifstream vf(p);
                std::string ver;
                if (std::getline(vf, ver) && !ver.empty()) {
                    try {
                        projectMajor = std::stoi(ver);
                    } catch (...) {
                        if (verbose) std::cerr << "      warning: could not parse LLVM version from '" << ver << "'\n";
                    }
                }
                break;
            }
        }
    }
    if (projectMajor == 0) {
        if (verbose) std::cerr << "      could not determine project LLVM version\n";
        return;
    }
    int diff = std::abs(rustcMajor - projectMajor);
    if (diff > 1) {
        std::cerr << "WARNING: rustc uses LLVM " << rustcMajor << " but project uses LLVM " << projectMajor
                  << " (difference > 1). Bitcode compatibility issues possible.\n";
    } else if (verbose) {
        std::cerr << "      rustc LLVM " << rustcMajor << ", project LLVM " << projectMajor << " — OK\n";
    }
}

/// Default cargo profile names probed when the caller does not pass an
/// explicit profile. The driver currently invokes ``cargo rustc`` without
/// ``--release`` so ``debug`` matches in practice, but listing the standard
/// profiles here lets a user who set ``CARGO_BUILD_PROFILE=release`` (or who
/// runs against a pre-built ``target/release`` tree) still discover the
/// artifact instead of hitting "could not find" with a misleading
/// ``target/debug`` mention.
static const std::array<const char*, 3> kCargoDefaultProfiles{"debug", "release", "dev"};

/// Locate a cargo build artifact across the standard profiles + any
/// user-set ``CARGO_BUILD_PROFILE`` and an optional explicit ``--target
/// <triple>`` segment.
///
/// Search order:
///   1. ``<target>/<CARGO_BUILD_PROFILE>/deps`` (or its triple-prefixed
///      sibling if ``CARGO_BUILD_TARGET`` is set).
///   2. ``<target>/debug/deps`` then ``<target>/release/deps`` so the
///      common-case profile lookup succeeds even if the caller did not
///      pass a profile name.
///   3. Any other ``<target>/<profile>/deps`` directory present on
///      disk (cargo custom profiles land here too).
///
/// ``predicate`` decides whether a directory entry is the artifact the
/// caller wants; we keep the per-artifact regexes local to each caller
/// (.bc / .rlib have slightly different filename rules).
static std::string findCargoArtifact(const fs::path& projectDir,
                                     const std::function<bool(const fs::directory_entry&)>& predicate) {
    fs::path targetDir = resolveCargoTargetDir(projectDir);
    if (!fs::exists(targetDir)) return "";

    // Build the search list: explicit env first, then the default
    // profiles, then any other profile directory present on disk.
    std::vector<fs::path> depsDirs;
    auto addCandidate = [&](const fs::path& candidate) {
        for (const auto& existing : depsDirs) {
            if (existing == candidate) return;
        }
        depsDirs.push_back(candidate);
    };

    // ``CARGO_BUILD_TARGET`` inserts a ``<triple>/`` segment.
    fs::path triplePrefix;
    if (const char* triple = std::getenv("CARGO_BUILD_TARGET")) {
        if (*triple) triplePrefix = triple;
    }
    auto profileRoot = [&](const std::string& profile) {
        fs::path root = triplePrefix.empty() ? targetDir : (targetDir / triplePrefix);
        return root / profile / "deps";
    };

    if (const char* envProfile = std::getenv("CARGO_BUILD_PROFILE")) {
        if (*envProfile) addCandidate(profileRoot(envProfile));
    }
    for (const char* p : kCargoDefaultProfiles) {
        addCandidate(profileRoot(p));
    }

    // Also probe any other ``target/<profile>/deps`` already on disk —
    // catches user-defined cargo profiles without prior knowledge.
    fs::path scanRoot = triplePrefix.empty() ? targetDir : (targetDir / triplePrefix);
    std::error_code scanEc;
    if (fs::exists(scanRoot, scanEc)) {
        for (const auto& entry : fs::directory_iterator(scanRoot, scanEc)) {
            if (!entry.is_directory()) continue;
            fs::path candidate = entry.path() / "deps";
            if (fs::exists(candidate)) addCandidate(candidate);
        }
    }

    for (const auto& depsDir : depsDirs) {
        if (!fs::exists(depsDir)) continue;
        std::error_code ec;
        for (const auto& entry : fs::directory_iterator(depsDir, ec)) {
            if (!entry.is_regular_file()) continue;
            if (predicate(entry)) return entry.path().string();
        }
    }
    return "";
}

/// Find the .bc file generated by cargo rustc in the target directory.
/// Searches all known cargo profiles so a ``--release`` build (or a
/// user-defined profile) is still discoverable.
///
/// Cargo names bitcode files `<crate>-<hash>.bc`. The match anchors on
/// the literal `<crate>-` prefix so a crate named `topo` does NOT also
/// match `topo_app-<hash>.bc` (or `topo_runtime`, `topo_check`, etc.)
/// in the same `deps/` directory — the previous prefix-only check
/// silently aliased overlapping crate names.
static std::string findRustBitcode(const fs::path& projectDir, const std::string& crateName) {
    std::string normalizedCrate = crateName;
    for (auto& c : normalizedCrate) {
        if (c == '-') c = '_';
    }
    std::string anchor = normalizedCrate + "-";
    return findCargoArtifact(projectDir, [&](const fs::directory_entry& entry) {
        std::string name = entry.path().filename().string();
        return name.compare(0, anchor.size(), anchor) == 0 &&
               entry.path().extension() == ".bc";
    });
}

/// Find the .rlib file generated by cargo rustc in the target directory.
/// Searches all known cargo profiles for the same reason as findRustBitcode.
///
/// Cargo names rlibs `lib<crate>-<hash>.rlib`. The anchor includes the
/// trailing `-` so e.g. crate `topo` does not match
/// `libtopo_app-<hash>.rlib` (see findRustBitcode for the same
/// rationale).
static std::string findRustRlib(const fs::path& projectDir, const std::string& crateName) {
    std::string normalizedCrate = crateName;
    for (auto& c : normalizedCrate) {
        if (c == '-') c = '_';
    }
    std::string anchor = "lib" + normalizedCrate + "-";
    return findCargoArtifact(projectDir, [&](const fs::directory_entry& entry) {
        std::string name = entry.path().filename().string();
        return name.compare(0, anchor.size(), anchor) == 0 &&
               entry.path().extension() == ".rlib";
    });
}

/// Extract crate name from Cargo.toml in the project directory.
static std::string readCrateName(const fs::path& projectDir) {
    fs::path cargoToml = projectDir / "Cargo.toml";
    if (!fs::exists(cargoToml)) return "";

    toml::parse_result result = toml::parse_file(cargoToml.string());
    if (!result) return "";

    if (auto name = result.table()["package"]["name"].value<std::string>()) return *name;
    return "";
}

/// Return true if the project's Cargo.toml declares a `topo` dependency
/// (under [dependencies], [dev-dependencies], or [build-dependencies]).
/// Only when this is true does it make sense to pass `--features topo/all-runtime`
/// to cargo — otherwise cargo errors with "package does not contain this feature".
static bool projectDependsOnTopoRuntime(const fs::path& projectDir) {
    fs::path cargoToml = projectDir / "Cargo.toml";
    if (!fs::exists(cargoToml)) return false;

    toml::parse_result result = toml::parse_file(cargoToml.string());
    if (!result) return false;

    const auto& root = result.table();
    for (const char* section : {"dependencies", "dev-dependencies", "build-dependencies"}) {
        if (auto* deps = root[section].as_table()) {
            if (deps->contains("topo")) return true;
        }
    }
    return false;
}

DriverResult compileRust(const BuildConfig& cfg) {
    namespace plat = platform;
    DriverResult result;

    std::cerr << "[3/7] Compiling Rust project via cargo rustc...\n";

    checkRustcLLVMVersion(cfg.verbose);

    fs::path projectDir = fs::current_path();
    std::string crateName = readCrateName(projectDir);
    if (crateName.empty()) {
        std::cerr << "error: could not read crate name from Cargo.toml\n";
        result.exitCode = 1;
        return result;
    }

    std::vector<std::string> cargoArgs = {"rustc",
                                          "--lib",
                                          "--manifest-path",
                                          (projectDir / "Cargo.toml").string()};
    // Only request the `topo/all-runtime` feature when the project actually
    // depends on the `topo` runtime crate. Passing it unconditionally fails
    // every rust project that does not opt into the runtime (rust_basic,
    // *_rust benchmarks, user projects).
    if (projectDependsOnTopoRuntime(projectDir)) {
        cargoArgs.emplace_back("--features");
        cargoArgs.emplace_back("topo/all-runtime");
    }
    cargoArgs.emplace_back("--");
    cargoArgs.emplace_back("--emit=llvm-bc");
    cargoArgs.emplace_back("-Csymbol-mangling-version=v0");
    cargoArgs.emplace_back("-Copt-level=1");
    cargoArgs.emplace_back("-Cno-prepopulate-passes");

    auto cargoResult = plat::runProcess(cfg.cargoPath, cargoArgs, cfg.verbose);
    if (cargoResult.exitCode != 0) {
        std::cerr << "error: cargo rustc failed (exit " << cargoResult.exitCode << ")\n";
        result.exitCode = 1;
        return result;
    }

    std::string bcFile = findRustBitcode(projectDir, crateName);
    if (bcFile.empty()) {
        std::cerr << "error: could not find generated .bc file under "
                  << resolveCargoTargetDir(projectDir).string()
                  << "/<profile>/deps/ for crate '" << crateName
                  << "' (probed debug, release, CARGO_BUILD_PROFILE, and "
                     "any other profile dir present)\n";
        result.exitCode = 1;
        return result;
    }

    result.outputFiles.push_back(bcFile);
    std::cerr << "      bitcode: " << bcFile << "\n";

    std::string rlibFile = findRustRlib(projectDir, crateName);
    if (rlibFile.empty()) {
        std::cerr << "error: could not find generated .rlib file under "
                  << resolveCargoTargetDir(projectDir).string()
                  << "/<profile>/deps/ for crate '" << crateName
                  << "' (probed debug, release, CARGO_BUILD_PROFILE, and "
                     "any other profile dir present)\n";
        result.exitCode = 1;
        return result;
    }
    result.outputFiles.push_back(rlibFile);
    std::cerr << "      rlib: " << rlibFile << "\n";

    return result;
}

DriverResult linkRust(const BuildConfig& cfg,
                      const std::string& optIRPath,
                      const fs::path& tempDir,
                      const std::string& rlibPath) {
    namespace plat = platform;
    DriverResult result;

    std::cerr << "[7/7] Linking Rust project...\n";

    // Shared optimization flags
    std::vector<std::string> optArgs = {"-O" + std::to_string(static_cast<int>(cfg.optLevel))};
    if (cfg.buildMode == BuildMode::Aggressive) {
        optArgs.push_back("-flto=thin");
        if constexpr (!plat::IsMacOS) {
            optArgs.push_back("-fuse-ld=lld");
        }
    }

    // Step 7a: compile optimized IR → object file
    std::string objPath = (tempDir / ("optimized" + std::string(plat::ObjectFileSuffix))).string();
    {
        std::vector<std::string> compileArgs = {"-c"};
        compileArgs.insert(compileArgs.end(), optArgs.begin(), optArgs.end());
        compileArgs.push_back(optIRPath);
        compileArgs.push_back("-o");
        compileArgs.push_back(objPath);
        result.exitCode = plat::runProcess(cfg.hostCompilerPath, compileArgs, cfg.verbose).exitCode;
    }
    if (result.exitCode != 0) {
        std::cerr << "error: IR compilation to object failed\n";
        return result;
    }

    // Read crate name early — needed for rlib naming and --extern
    fs::path projectDir = fs::current_path();
    std::string crateName = readCrateName(projectDir);
    std::string normalizedCrate = crateName;
    for (auto& c : normalizedCrate) {
        if (c == '-') c = '_';
    }

    // Step 7b: replace .o inside rlib with optimized object
    // rustc requires rlib files to be named lib<crate>.rlib
    fs::path modifiedRlib = tempDir / ("lib" + normalizedCrate + ".rlib");
    {
        std::string llvmAr = plat::resolveLLVMTool("llvm-ar");

        // List rlib members to identify .rmeta and .o files
        auto listResult = plat::runProcessCapture(llvmAr, {"t", rlibPath}, cfg.verbose);
        if (listResult.exitCode != 0) {
            std::cerr << "error: llvm-ar t failed on " << rlibPath << "\n";
            result.exitCode = 1;
            return result;
        }

        // Parse member names — find the .rmeta entry
        std::string rmetaFile;
        {
            std::istringstream iss(listResult.stdoutOutput);
            std::string line;
            while (std::getline(iss, line)) {
                // Trim trailing whitespace
                while (!line.empty() && (line.back() == '\r' || line.back() == '\n' || line.back() == ' '))
                    line.pop_back();
                if (line.empty()) continue;
                if (line.find(".rmeta") != std::string::npos) {
                    rmetaFile = line;
                }
            }
        }
        if (rmetaFile.empty()) {
            std::cerr << "error: no .rmeta found in rlib " << rlibPath << "\n";
            result.exitCode = 1;
            return result;
        }

        if (cfg.verbose) {
            std::cerr << "      rlib members: rmeta=" << rmetaFile << "\n";
        }

        // Extract .rmeta into tempDir (llvm-ar x extracts to cwd)
        auto extractResult = plat::runProcessCapture(llvmAr, {"x", rlibPath, rmetaFile}, tempDir.string(), cfg.verbose);
        if (extractResult.exitCode != 0) {
            std::cerr << "error: llvm-ar x failed to extract " << rmetaFile << "\n";
            result.exitCode = 1;
            return result;
        }

        // Build modified rlib: rmeta + optimized.o
        fs::path extractedRmeta = tempDir / rmetaFile;
        auto buildResult = plat::runProcessCapture(
            llvmAr, {"rcs", modifiedRlib.string(), extractedRmeta.string(), objPath}, cfg.verbose);
        if (buildResult.exitCode != 0) {
            std::cerr << "error: llvm-ar rcs failed to build modified rlib\n";
            result.exitCode = 1;
            return result;
        }

        std::cerr << "      replaced .o in rlib with optimized object\n";
    }

    // Step 7c: link via rustc with modified rlib
    std::vector<std::string> rustcArgs = {"--edition", "2021", "--crate-type"};

    switch (cfg.outputType) {
    case OutputType::Exe: rustcArgs.push_back("bin"); break;
    case OutputType::Shared: rustcArgs.push_back("cdylib"); break;
    case OutputType::Static: rustcArgs.push_back("staticlib"); break;
    }

    // Optimize the entry source (main.rs) to match the library optimization level
    rustcArgs.push_back("-Copt-level=" + std::to_string(static_cast<int>(cfg.optLevel)));

    // Dev mode emits DWARF for main.rs so `topo debug` / lldb can resolve
    // breakpoints + locals inside `fn main()` and other entry-source bodies.
    // Mirrors the cpp driver's `-g` insertion in CppDriver.cpp:46 — without
    // it, `topo debug query 'sum(x)'` against a stock `topo build` rust
    // binary fails with "no breakpoint locations resolved" because the
    // entry-source side of the link has no debug info even when the lib
    // half does. Aggressive mode strips for release artifacts.
    if (cfg.buildMode != BuildMode::Aggressive) {
        rustcArgs.push_back("-Cdebuginfo=2");
    }

    // Use modified rlib via --extern
    rustcArgs.push_back("--extern");
    rustcArgs.push_back(normalizedCrate + "=" + modifiedRlib.string());

    for (const auto& dir : cfg.linkDirs)
        rustcArgs.push_back("-Clink-arg=-L" + dir);
    for (const auto& lib : cfg.linkLibs)
        rustcArgs.push_back("-Clink-arg=-l" + lib);

    // Topo runtime libraries are C++ — rustc needs explicit C++ stdlib linkage
    if (!cfg.linkLibs.empty()) {
        if constexpr (plat::IsMacOS) {
            rustcArgs.push_back("-Clink-arg=-lc++");
        } else {
            rustcArgs.push_back("-Clink-arg=-lstdc++");
        }
    }

    rustcArgs.push_back("-o");
    rustcArgs.push_back(cfg.outputPath);

    // Determine the entry source file
    fs::path entryRs;
    if (cfg.outputType == OutputType::Exe) {
        // Look for user's main.rs (Cargo convention)
        fs::path userMain = projectDir / "src" / "main.rs";
        if (fs::exists(userMain)) {
            entryRs = userMain;
        }
    }

    if (entryRs.empty()) {
        // Write a minimal dummy for library outputs or when no main.rs exists
        entryRs = tempDir / "topo_link_dummy.rs";
        std::ofstream ofs(entryRs);
        if (cfg.outputType == OutputType::Exe) {
            ofs << "fn main() {}\n";
        } else {
            ofs << "// topo link stub\n";
        }
    }
    rustcArgs.push_back(entryRs.string());

    result.exitCode = plat::runProcess("rustc", rustcArgs, cfg.verbose).exitCode;

    return result;
}

} // namespace topo::build
