// E2E tests for Rust purity checks.
//
// Exercises the full topo-check pipeline for parallel-stage purity:
//   .topo parsing → symbol table → file scan → RustSymbolAccessExtractor
//   → checkPurity → diagnostics.

#include "CheckRunner.h"

#include <gtest/gtest.h>

using namespace topo;

static std::string fixtureDir(const char* name) {
    return std::string(TOPO_TEST_FIXTURES_DIR) + "/" + name;
}

TEST(RustPurity, Pass01_NoGlobalWrites) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("purity_rust_pass_01");
    cfg.checkName = "purity";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);
}

TEST(RustPurity, Pass02_LocalsOnly) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("purity_rust_pass_02");
    cfg.checkName = "purity";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);
}

TEST(RustPurity, Pass03_SequentialStagesAllowGlobalWrites) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("purity_rust_pass_03");
    cfg.checkName = "purity";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);
}

TEST(RustPurity, Pass04_ModeOffSuppressesViolations) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("purity_rust_pass_04");
    cfg.checkName = "purity";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 0);
}

TEST(RustPurity, Fail01_ParallelStaticMutWrite) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("purity_rust_fail_01");
    cfg.checkName = "purity";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 1);
    auto& results = runner.lastResults();
    ASSERT_EQ(results.size(), 1u);
    EXPECT_GE(results[0].second.errorCount, 1);
    bool foundCounter = false;
    for (const auto& d : results[0].second.diagnostics) {
        if (d.message.find("COUNTER") != std::string::npos) foundCounter = true;
    }
    EXPECT_TRUE(foundCounter) << "Expected `COUNTER` to appear in the violation message";
}

TEST(RustPurity, Fail02_ParallelStaticMutAssign) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("purity_rust_fail_02");
    cfg.checkName = "purity";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 1);
    auto& results = runner.lastResults();
    ASSERT_EQ(results.size(), 1u);
    EXPECT_GE(results[0].second.errorCount, 1);
}

TEST(RustPurity, Fail03_CompoundAssignment) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("purity_rust_fail_03");
    cfg.checkName = "purity";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 1);
    auto& results = runner.lastResults();
    ASSERT_EQ(results.size(), 1u);
    // tick() and monitor() each write — expect both violations.
    EXPECT_GE(results[0].second.errorCount, 2);
}

TEST(RustPurity, Fail04_MultipleParallelViolations) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("purity_rust_fail_04");
    cfg.checkName = "purity";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    EXPECT_EQ(runner.run(), 1);
    auto& results = runner.lastResults();
    ASSERT_EQ(results.size(), 1u);
    // Three parallel functions each write a distinct global.
    EXPECT_GE(results[0].second.errorCount, 3);
}
