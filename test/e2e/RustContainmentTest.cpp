// E2E tests for Rust containment checks.

#include "CheckRunner.h"

#include <gtest/gtest.h>
#include <cstdlib>

using namespace topo;

static std::string fixtureDir(const char* name) {
    return std::string(TOPO_TEST_FIXTURES_DIR) + "/" + name;
}

static bool hasExtractionUnavailable(const CheckRunner& runner) {
    for (const auto& [name, result] : runner.lastResults()) {
        for (const auto& d : result.diagnostics) {
            if (d.message.find("import extraction unavailable") != std::string::npos ||
                d.message.find("symbol extraction unavailable") != std::string::npos) {
                return true;
            }
        }
    }
    return false;
}

static bool hasRustAnalyzer() {
#ifdef _WIN32
    int ret = std::system("rust-analyzer --version > NUL 2>&1");
#else
    int ret = std::system("rust-analyzer --version > /dev/null 2>&1");
#endif
    return ret == 0;
}

TEST(RustContainment, ContainmentRustFail) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_fail");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentRustExternalOk) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_external_ok");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 0);
}

TEST(RustContainment, ContainmentUnsafeBlock) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_unsafe_block");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentRawPointer) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_raw_pointer");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentFfi) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_ffi");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentCommand) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_command");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentNetwork) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_network");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentFile) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_file");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentStaticMut) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_static_mut");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentSafeCode) {
    if (!hasRustAnalyzer()) GTEST_SKIP() << "rust-analyzer not available";
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_safe_code");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 0);
}

// --- M3 Group C: adversarial coverage for ptr/mem family + unsafe trait/impl ---
//
// Each fixture exercises one escape pattern that previously lacked coverage:
// catalog gaps (#8) and unsafe trait/impl detection (#9).

TEST(RustContainment, ContainmentTransmuteCopy) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_transmute_copy");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentZeroedTyped) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_zeroed_typed");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentBoxFromRaw) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_box_from_raw");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentArcGetMutUnchecked) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_arc_get_mut_unchecked");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentSliceFromRawParts) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_slice_from_raw_parts");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentUnsafeTraitDecl) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_unsafe_trait_decl");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentUnsafeImplSend) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_unsafe_impl_send");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 1);
}

TEST(RustContainment, ContainmentSafePtrUse) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_safe_ptr_use");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 0);
}

TEST(RustContainment, ContainmentSafeBox) {
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_rust_safe_box");
    cfg.checkName = "containment";
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 0);
}

// --- L2 deep-mode coverage ---
// These force cfg.deepMode = true so the RustAnalyzerBridge +
// RustSafetyAnalyzer path actually runs through checkContainment.  The
// tests above all exercise L1 only — see
// checker-l2-synthetic-caller-attribution.md for why this matters.

class RustContainmentL2 : public ::testing::Test {
protected:
    void SetUp() override {
        if (!hasRustAnalyzer()) {
            GTEST_SKIP() << "rust-analyzer unavailable — L2 deep containment "
                            "tests need rust-analyzer to run.";
        }
    }
};

TEST_F(RustContainmentL2, ContainmentL2ExternalOk) {
    // `read_file` is declared external and calls `std::fs::read_to_string`.
    // This test primarily verifies that the L2 pipeline runs cleanly for
    // Rust under deep mode.  Note: with the current safe-patterns
    // whitelist, rust-analyzer's hover classifies `std::fs::*` as a safe
    // stdlib call so 0 call sites reach checkContainment — the test
    // therefore proves "L2 ran without crashing" rather than attribution
    // correctness.  The canonical attribution regression guard is Java's
    // `JavaContainmentL2.ContainmentL2ExternalOk`; Rust's whitelist
    // coverage gap is tracked separately.
    CheckConfig cfg;
    cfg.projectDir = fixtureDir("containment_external_ok");
    cfg.checkName = "containment";
    cfg.deepMode = true;
    CheckRunner runner(cfg);
    ASSERT_TRUE(runner.loadConfig());
    int rc = runner.run();
    if (hasExtractionUnavailable(runner))
        GTEST_SKIP() << "extraction unavailable (Rust toolchain not functional)";
    EXPECT_EQ(rc, 0);
}
