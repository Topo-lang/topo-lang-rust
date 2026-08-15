// Unit tests for RustSymbolExtractor — L1 regex symbol extraction from Rust
// source files.

#include "analysis/extract/RustSymbolExtractor.h"

#include <gtest/gtest.h>
#include <filesystem>
#include <fstream>
#include <string>

namespace fs = std::filesystem;
using namespace topo::check;

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

class RustSymbolExtractorTest : public ::testing::Test {
protected:
    void SetUp() override {
        tempDir_ = fs::temp_directory_path() / ("topo_rust_extractor_test_" + std::to_string(topo_getpid()));
        fs::create_directories(tempDir_);
    }

    void TearDown() override {
        std::error_code ec;
        fs::remove_all(tempDir_, ec);
    }

    std::string writeTempFile(const std::string& name, const std::string& content) {
        auto path = tempDir_ / name;
        std::ofstream ofs(path);
        ofs << content;
        return path.string();
    }

    fs::path tempDir_;
};

// Helper: find a symbol by qualifiedName in a vector.
static const HostSymbol* findByQualified(const std::vector<HostSymbol>& syms, const std::string& qname) {
    for (const auto& s : syms) {
        if (s.qualifiedName == qname) return &s;
    }
    return nullptr;
}

// Regression: a single-line `pub struct Cart { ... }` opens AND closes its
// body brace on one line. The struct scope must not stay pushed — a leaked
// entry (depth below 0, unpoppable) doubled the prefix of every later impl
// method: `Cart::Cart::new` instead of `Cart::new`.
TEST_F(RustSymbolExtractorTest, SingleLineStructDoesNotLeakScope) {
    auto path = writeTempFile("one_liner.rs",
                              "pub struct Cart { capacity: i32 }\n"
                              "\n"
                              "impl Cart {\n"
                              "    pub fn new(capacity: i32) -> Cart { Cart { capacity } }\n"
                              "    pub fn capacity(&self) -> i32 { self.capacity }\n"
                              "}\n");

    RustSymbolExtractor extractor;
    auto syms = extractor.extractSymbols(path);

    auto* ctor = findByQualified(syms, "Cart::new");
    ASSERT_NE(ctor, nullptr);
    EXPECT_EQ(ctor->kind, HostSymbolKind::StaticMethod);
    EXPECT_EQ(findByQualified(syms, "Cart::Cart::new"), nullptr)
        << "one-liner struct scope leaked into the impl prefix";

    auto* method = findByQualified(syms, "Cart::capacity");
    ASSERT_NE(method, nullptr);
    EXPECT_EQ(method->kind, HostSymbolKind::Method);
}

// Companion guard: a multi-line struct still scopes normally and the
// enclosing mod still qualifies both the type and its impl methods.
TEST_F(RustSymbolExtractorTest, MultiLineStructScopesNormally) {
    auto path = writeTempFile("multi_line.rs",
                              "mod shop {\n"
                              "    pub struct Order {\n"
                              "        id: i64,\n"
                              "    }\n"
                              "    impl Order {\n"
                              "        pub fn id(&self) -> i64 { self.id }\n"
                              "    }\n"
                              "    pub fn checkout() {}\n"
                              "}\n");

    RustSymbolExtractor extractor;
    auto syms = extractor.extractSymbols(path);

    EXPECT_NE(findByQualified(syms, "shop::Order"), nullptr);
    EXPECT_NE(findByQualified(syms, "shop::Order::id"), nullptr);

    auto* fn = findByQualified(syms, "shop::checkout");
    ASSERT_NE(fn, nullptr);
    EXPECT_EQ(fn->kind, HostSymbolKind::Function);
}

// Same one-liner rule for enums: a later free function must stay top-level.
TEST_F(RustSymbolExtractorTest, SingleLineEnumDoesNotLeakScope) {
    auto path = writeTempFile("one_liner_enum.rs",
                              "pub enum State { Idle, Busy }\n"
                              "\n"
                              "pub fn step() {}\n");

    RustSymbolExtractor extractor;
    auto syms = extractor.extractSymbols(path);

    auto* fn = findByQualified(syms, "step");
    ASSERT_NE(fn, nullptr);
    EXPECT_EQ(fn->kind, HostSymbolKind::Function);
    EXPECT_EQ(findByQualified(syms, "State::step"), nullptr)
        << "one-liner enum scope leaked onto a later free function";
}
