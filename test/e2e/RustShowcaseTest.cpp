// E2E test: the shipped showcase example must check clean end-to-end.
//
// Runs the full CheckRunner (default checkName = "all") over a copy of
// examples/showcase — the copy keeps checker cache artifacts out of the
// source tree. Deliberately no GTEST_SKIP path: extraction falls back to
// the in-process L1 path when rust-analyzer is unavailable, so the CI
// "skip != pass" re-run asserts can rely on this case always executing.

#include "CheckRunner.h"

#include <gtest/gtest.h>

#include <filesystem>
#include <string>

#ifdef _WIN32
#include <process.h>
static int topo_getpid() {
    return _getpid();
}
#else
#include <unistd.h>
static int topo_getpid() {
    return getpid();
}
#endif

namespace fs = std::filesystem;
using namespace topo;

TEST(RustShowcase, FullCheckPasses) {
    fs::path src = fs::path(TOPO_EXAMPLES_DIR) / "showcase";
    fs::path projectDir = fs::temp_directory_path() /
                          ("topo-rust-showcase_" + std::to_string(topo_getpid()));
    std::error_code ec;
    fs::remove_all(projectDir, ec);
    fs::copy(src, projectDir, fs::copy_options::recursive);

    CheckConfig cfg;
    cfg.projectDir = projectDir.string();
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);

    fs::remove_all(projectDir, ec);
}
