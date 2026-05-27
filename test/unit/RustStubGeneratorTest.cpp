// Unit tests for RustStubGenerator — function body finding and stubbing.

#include "analysis/stub/RustStubGenerator.h"

#include <gtest/gtest.h>
#include <filesystem>
#include <fstream>
#include <sstream>
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

// --- findFunctionBodyStart tests ---

TEST(RustStubGenerator, FindSimpleFunction) {
    std::string source = "fn add(a: i32, b: i32) -> i32 { a + b }\n";
    size_t pos = RustStubGenerator::findFunctionBodyStart(source, "add");
    ASSERT_NE(pos, std::string::npos);
    EXPECT_EQ(source[pos], '{');
}

TEST(RustStubGenerator, FindFunctionWithReturnType) {
    std::string source = "fn compute(x: i32) -> Vec<i32> { vec![x] }\n";
    size_t pos = RustStubGenerator::findFunctionBodyStart(source, "compute");
    ASSERT_NE(pos, std::string::npos);
    EXPECT_EQ(source[pos], '{');
}

TEST(RustStubGenerator, FindFunctionWithGenerics) {
    std::string source = "fn convert<T: Into<String>>(val: T) -> String { val.into() }\n";
    size_t pos = RustStubGenerator::findFunctionBodyStart(source, "convert");
    ASSERT_NE(pos, std::string::npos);
    EXPECT_EQ(source[pos], '{');
}

TEST(RustStubGenerator, FindFunctionWithBoundedGenerics) {
    std::string source =
        "fn merge<K: Ord, V: Clone>(a: Map<K, V>, b: Map<K, V>) -> Map<K, V> {\n"
        "    a.into_iter().chain(b).collect()\n"
        "}\n";
    size_t pos = RustStubGenerator::findFunctionBodyStart(source, "merge");
    ASSERT_NE(pos, std::string::npos);
    EXPECT_EQ(source[pos], '{');
}

TEST(RustStubGenerator, FindPubFunction) {
    std::string source = "pub fn serve() { loop {} }\n";
    size_t pos = RustStubGenerator::findFunctionBodyStart(source, "serve");
    ASSERT_NE(pos, std::string::npos);
    EXPECT_EQ(source[pos], '{');
}

TEST(RustStubGenerator, FindFunctionWithNestedBraces) {
    std::string source =
        "fn process(x: i32) -> i32 {\n"
        "    if x > 0 {\n"
        "        for i in 0..x {\n"
        "            println!(\"{}\", i);\n"
        "        }\n"
        "    }\n"
        "    x\n"
        "}\n";
    size_t pos = RustStubGenerator::findFunctionBodyStart(source, "process");
    ASSERT_NE(pos, std::string::npos);
    EXPECT_EQ(source[pos], '{');
}

TEST(RustStubGenerator, FunctionNotFound) {
    std::string source = "fn foo() { }\n";
    size_t pos = RustStubGenerator::findFunctionBodyStart(source, "nonexistent");
    EXPECT_EQ(pos, std::string::npos);
}

TEST(RustStubGenerator, DoesNotMatchSubstring) {
    std::string source =
        "fn foobar() {\n"
        "    // body\n"
        "}\n";
    // "foo" should not match "foobar"
    size_t pos = RustStubGenerator::findFunctionBodyStart(source, "foo");
    EXPECT_EQ(pos, std::string::npos);
}

// --- findMatchingBrace tests ---

TEST(RustStubGenerator, MatchingBraceSimple) {
    std::string source = "{ 42 }";
    size_t end = RustStubGenerator::findMatchingBrace(source, 0);
    ASSERT_NE(end, std::string::npos);
    EXPECT_EQ(end, source.size() - 1);
}

TEST(RustStubGenerator, MatchingBraceNested) {
    std::string source = "{ if x { y(); } z }";
    size_t end = RustStubGenerator::findMatchingBrace(source, 0);
    ASSERT_NE(end, std::string::npos);
    EXPECT_EQ(source[end], '}');
    EXPECT_EQ(end, source.size() - 1);
}

TEST(RustStubGenerator, MatchingBraceWithRustStrings) {
    std::string source = "{ let s = \"}\"; s }";
    size_t end = RustStubGenerator::findMatchingBrace(source, 0);
    ASSERT_NE(end, std::string::npos);
    EXPECT_EQ(source[end], '}');
    EXPECT_EQ(end, source.size() - 1);
}

TEST(RustStubGenerator, MatchingBraceLineComment) {
    std::string source = "{ // }\n  1 }";
    size_t end = RustStubGenerator::findMatchingBrace(source, 0);
    ASSERT_NE(end, std::string::npos);
    EXPECT_EQ(source[end], '}');
    EXPECT_EQ(end, source.size() - 1);
}

TEST(RustStubGenerator, MatchingBraceBlockComment) {
    std::string source = "{ /* } */ 1 }";
    size_t end = RustStubGenerator::findMatchingBrace(source, 0);
    ASSERT_NE(end, std::string::npos);
    EXPECT_EQ(source[end], '}');
    EXPECT_EQ(end, source.size() - 1);
}

TEST(RustStubGenerator, MatchingBraceUnmatched) {
    std::string source = "{ return 1;";
    size_t end = RustStubGenerator::findMatchingBrace(source, 0);
    EXPECT_EQ(end, std::string::npos);
}

// --- isUnitReturn tests ---

TEST(RustStubGenerator, UnitReturnNoArrow) {
    std::string source = "fn process() {";
    size_t bodyPos = source.find('{');
    EXPECT_TRUE(RustStubGenerator::isUnitReturn(source, bodyPos));
}

TEST(RustStubGenerator, TypedReturnI32) {
    std::string source = "fn compute(x: i32) -> i32 {";
    size_t bodyPos = source.find('{');
    EXPECT_FALSE(RustStubGenerator::isUnitReturn(source, bodyPos));
}

TEST(RustStubGenerator, ExplicitUnitReturn) {
    std::string source = "fn work() -> () {";
    size_t bodyPos = source.find('{');
    EXPECT_TRUE(RustStubGenerator::isUnitReturn(source, bodyPos));
}

// --- Integration: stub + restore via temp file ---

class RustStubGeneratorFileTest : public ::testing::Test {
protected:
    void SetUp() override {
        tempDir_ = fs::temp_directory_path() / ("topo_rust_check_test_" + std::to_string(topo_getpid()));
        fs::create_directories(tempDir_);
    }

    void TearDown() override {
        std::error_code ec;
        fs::remove_all(tempDir_, ec);
    }

    std::string writeTempFile(const std::string& name, const std::string& content) {
        auto path = tempDir_ / name;
        // Binary mode: don't let Windows text-mode I/O insert \r before \n —
        // RustStubGenerator reads/writes the file as raw bytes, so the fixture
        // must too.
        std::ofstream ofs(path, std::ios::binary);
        ofs << content;
        return path.string();
    }

    std::string readTempFile(const std::string& path) {
        std::ifstream ifs(path, std::ios::binary);
        std::ostringstream ss;
        ss << ifs.rdbuf();
        return ss.str();
    }

    fs::path tempDir_;
};

TEST_F(RustStubGeneratorFileTest, StubAndRestore) {
    std::string original =
        "fn compute(x: i32) -> i32 {\n"
        "    let y = x * 2;\n"
        "    y + 1\n"
        "}\n";
    std::string path = writeTempFile("test.rs", original);

    RustStubGenerator gen;
    auto result = gen.stubFunction(path, "compute");

    ASSERT_TRUE(result.success) << result.error;
    EXPECT_EQ(result.originalContent, original);

    // File should now contain stub with Default::default()
    std::string modified = readTempFile(path);
    EXPECT_NE(modified, original);
    EXPECT_NE(modified.find("Default::default()"), std::string::npos);

    // Restore
    EXPECT_TRUE(gen.restoreFile(path, result));
    std::string restored = readTempFile(path);
    EXPECT_EQ(restored, original);
}

TEST_F(RustStubGeneratorFileTest, StubUnitFunction) {
    std::string original =
        "fn process() {\n"
        "    do_something();\n"
        "}\n";
    std::string path = writeTempFile("test.rs", original);

    RustStubGenerator gen;
    auto result = gen.stubFunction(path, "process");

    ASSERT_TRUE(result.success) << result.error;

    std::string modified = readTempFile(path);
    EXPECT_NE(modified.find("{ }"), std::string::npos);
    // Should NOT contain "return" or "Default"
    EXPECT_EQ(modified.find("return"), std::string::npos);
    EXPECT_EQ(modified.find("Default"), std::string::npos);
}

TEST_F(RustStubGeneratorFileTest, StubTypedFunction) {
    std::string original =
        "fn get_value() -> String {\n"
        "    String::from(\"hello\")\n"
        "}\n";
    std::string path = writeTempFile("test.rs", original);

    RustStubGenerator gen;
    auto result = gen.stubFunction(path, "get_value");

    ASSERT_TRUE(result.success) << result.error;

    std::string modified = readTempFile(path);
    EXPECT_NE(modified.find("Default::default()"), std::string::npos);
}

TEST_F(RustStubGeneratorFileTest, StubFunctionNotFound) {
    std::string original = "fn foo() { }\n";
    std::string path = writeTempFile("test.rs", original);

    RustStubGenerator gen;
    auto result = gen.stubFunction(path, "nonexistent");

    EXPECT_FALSE(result.success);
    EXPECT_FALSE(result.error.empty());
}

TEST_F(RustStubGeneratorFileTest, StubNestedBraces) {
    std::string original =
        "fn complex(x: i32) -> i32 {\n"
        "    if x > 0 {\n"
        "        for i in 0..x {\n"
        "            println!(\"{}\", i);\n"
        "        }\n"
        "    }\n"
        "    x\n"
        "}\n";
    std::string path = writeTempFile("test.rs", original);

    RustStubGenerator gen;
    auto result = gen.stubFunction(path, "complex");

    ASSERT_TRUE(result.success) << result.error;

    std::string modified = readTempFile(path);
    EXPECT_NE(modified.find("Default::default()"), std::string::npos);
    // Original nested braces should be gone
    EXPECT_EQ(modified.find("for i in"), std::string::npos);
}

TEST_F(RustStubGeneratorFileTest, StubGenericFunction) {
    std::string original =
        "fn convert<T: Into<String>>(val: T) -> String {\n"
        "    val.into()\n"
        "}\n";
    std::string path = writeTempFile("test.rs", original);

    RustStubGenerator gen;
    auto result = gen.stubFunction(path, "convert");

    ASSERT_TRUE(result.success) << result.error;

    std::string modified = readTempFile(path);
    EXPECT_NE(modified.find("Default::default()"), std::string::npos);
    // Generic signature should be preserved
    EXPECT_NE(modified.find("fn convert<T: Into<String>>"), std::string::npos);
}
