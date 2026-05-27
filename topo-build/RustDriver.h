#ifndef TOPO_BUILD_RUSTDRIVER_H
#define TOPO_BUILD_RUSTDRIVER_H

#include "topo/Build/BuildConfig.h"

#include <filesystem>

namespace topo::build {

/// Compile Rust project via cargo rustc → .bc files.
DriverResult compileRust(const BuildConfig& cfg);

/// Link optimized IR → final binary via rustc.
/// rlibPath: the .rlib from cargo rustc; its .o is replaced with the optimized one.
DriverResult linkRust(const BuildConfig& cfg,
                      const std::string& optIRPath,
                      const std::filesystem::path& tempDir,
                      const std::string& rlibPath);

} // namespace topo::build

#endif // TOPO_BUILD_RUSTDRIVER_H
