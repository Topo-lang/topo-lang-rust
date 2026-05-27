// E2E tests for Rust completeness checks.

#include "CheckRunner.h"

#include <gtest/gtest.h>
#include <cstdlib>

using namespace topo;

static std::string fixtureDir(const char* name) {
    return std::string(TOPO_TEST_FIXTURES_DIR) + "/" + name;
}

TEST(RustCompleteness, CompletenessPassFixture) {
#ifdef _WIN32
    int ret = std::system("rust-analyzer --version > NUL 2>&1");
#else
    int ret = std::system("rust-analyzer --version > /dev/null 2>&1");
#endif
    if (ret != 0) {
        GTEST_SKIP() << "rust-analyzer not available, skipping Rust completeness test";
    }

    CheckConfig cfg;
    cfg.projectDir = fixtureDir("completeness_pass");
    cfg.checkName = "completeness";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    for (const auto& [name, result] : runner.lastResults()) {
        for (const auto& d : result.diagnostics) {
            if (d.message.find("symbol extraction unavailable") != std::string::npos) {
                GTEST_SKIP() << "symbol extraction unavailable (Rust toolchain not functional)";
            }
        }
    }
    EXPECT_EQ(rc, 0);
}

TEST(RustCompleteness, CompletenessFailFixture) {
#ifdef _WIN32
    int ret = std::system("rust-analyzer --version > NUL 2>&1");
#else
    int ret = std::system("rust-analyzer --version > /dev/null 2>&1");
#endif
    if (ret != 0) {
        GTEST_SKIP() << "rust-analyzer not available, skipping Rust completeness test";
    }

    CheckConfig cfg;
    cfg.projectDir = fixtureDir("completeness_fail");
    cfg.checkName = "completeness";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    for (const auto& [name, result] : runner.lastResults()) {
        for (const auto& d : result.diagnostics) {
            if (d.message.find("symbol extraction unavailable") != std::string::npos) {
                GTEST_SKIP() << "symbol extraction unavailable (Rust toolchain not functional)";
            }
        }
    }
    EXPECT_EQ(rc, 1);
}
