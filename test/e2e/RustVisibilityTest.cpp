// E2E tests for Rust visibility checks.
//
// Exercises the full topo-check pipeline for private/internal visibility:
//   .topo parsing → symbol table → file scan → RustCallEdgeExtractor
//   → checkVisibilityConsistency → diagnostics.

#include "CheckRunner.h"

#include <gtest/gtest.h>

using namespace topo;

static std::string fixtureDir(const char* name) {
    return std::string(TOPO_TEST_FIXTURES_DIR) + "/" + name;
}

TEST(RustVisibility, Pass01_PublicToPublic) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("visibility_rust_pass_01");
    cfg.checkName = "visibility";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);
}

TEST(RustVisibility, Pass02_SameModulePrivate) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("visibility_rust_pass_02");
    cfg.checkName = "visibility";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);
}

TEST(RustVisibility, Pass03_MixedVisibilityAllLegal) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("visibility_rust_pass_03");
    cfg.checkName = "visibility";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);
}

TEST(RustVisibility, Pass04_ModeOffSuppressesViolations) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("visibility_rust_pass_04");
    cfg.checkName = "visibility";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);
}

TEST(RustVisibility, Fail01_CrossModulePrivate) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("visibility_rust_fail_01");
    cfg.checkName = "visibility";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 1);
    auto& results = runner.lastResults();
    ASSERT_EQ(results.size(), 1u);
    EXPECT_GE(results[0].second.errorCount, 1);
    bool foundHelper = false;
    for (const auto& d : results[0].second.diagnostics) {
        if (d.message.find("app::helper") != std::string::npos) foundHelper = true;
    }
    EXPECT_TRUE(foundHelper) << "Expected `app::helper` to appear in the violation message";
}

TEST(RustVisibility, Fail02_InternalCalledByExternal) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("visibility_rust_fail_02");
    cfg.checkName = "visibility";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 1);
    auto& results = runner.lastResults();
    ASSERT_EQ(results.size(), 1u);
    EXPECT_GE(results[0].second.errorCount, 1);
    bool foundInternal = false;
    for (const auto& d : results[0].second.diagnostics) {
        if (d.message.find("initInternal") != std::string::npos) foundInternal = true;
    }
    EXPECT_TRUE(foundInternal) << "Expected `initInternal` to appear in the violation message";
}

TEST(RustVisibility, Fail03_MultiplePrivateViolations) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("visibility_rust_fail_03");
    cfg.checkName = "visibility";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 1);
    auto& results = runner.lastResults();
    ASSERT_EQ(results.size(), 1u);
    // `consumer::drive` calls both `lib::alpha` and `lib::beta`.
    EXPECT_GE(results[0].second.errorCount, 2);
}

TEST(RustVisibility, Fail04_NestedModulePrivateReach) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("visibility_rust_fail_04");
    cfg.checkName = "visibility";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 1);
    auto& results = runner.lastResults();
    ASSERT_EQ(results.size(), 1u);
    EXPECT_GE(results[0].second.errorCount, 1);
    bool foundFlush = false;
    for (const auto& d : results[0].second.diagnostics) {
        if (d.message.find("flushBuffers") != std::string::npos) foundFlush = true;
    }
    EXPECT_TRUE(foundFlush) << "Expected `flushBuffers` in the violation message";
}
