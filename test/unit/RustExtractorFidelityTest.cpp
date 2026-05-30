// RustExtractorFidelityTest.cpp
//
// Golden fidelity tests for topo-extract-rust.
//
// For each fixture directory under RUST_FIDELITY_FIXTURES_DIR:
//   - input.rs       — Rust source file to extract from
//   - request.json   — stdin template for the extractor; the `@INPUT@`
//                      token is substituted with the absolute path of
//                      input.rs at test time so the subprocess does not
//                      depend on the caller's cwd
//   - expected.json  — golden TranspileModule output to compare against
//
// The driver spawns `topo-extract-rust` (path baked in via
// RUST_EXTRACTOR_BINARY) with piped stdin and stdout, writes the resolved
// request JSON to the child's stdin and CLOSES the write end so the child
// observes EOF on std::io::stdin().read_to_string and proceeds to emit its
// TranspileModule on stdout.
//
// Why a local subprocess helper?
// ------------------------------
// topo::platform::PipedProcess was designed for bidirectional JSON-RPC
// framing (LSP/clangd) and intentionally keeps stdin and stdout open until
// stop() closes them both. That creates a deadlock for one-shot
// request/response children like the extractor: the parent cannot close
// stdin (to signal EOF so the child proceeds to serialize output) while
// still holding stdout open for reading. Rather than extending Platform,
// this test uses a self-contained POSIX/Windows subprocess helper scoped to
// the test binary. See `runExtractorOnce`. The same pattern is used by
// topo-lang-cpp/test/unit/CppExtractorFidelityTest.cpp — the latent
// deadlock in TranspileDriver::extractFunctions is tracked as a separate
// open issue.
//
// Golden bootstrap / update semantics
// -----------------------------------
//   * If `expected.json` is missing, the driver writes the actual extractor
//     output to `expected.json` and the test PASSES. This is the first-run
//     bootstrap path; the developer inspects the written golden and commits
//     it. Subsequent runs do strict comparison.
//   * If `expected.json` exists, the driver performs field-level comparison
//     via `nlohmann::json::operator==` (object key order is irrelevant).
//     Before comparison both sides are normalized to sort the `functions`
//     array by qualifiedName, because the extractor stores collected
//     functions in a HashMap whose iteration order is non-deterministic.
//   * Setting `TOPO_FIDELITY_UPDATE=1` overwrites existing goldens from the
//     fresh output.
//
// A fixture directory containing a `SKIP.md` file is skipped with
// GTEST_SKIP() and the first line of SKIP.md is logged as the reason.

#include <gtest/gtest.h>
#include <nlohmann/json.hpp>

#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

#ifdef _WIN32
#    ifndef NOMINMAX
#        define NOMINMAX
#    endif
#    include <windows.h>
#else
#    include <spawn.h>
#    include <sys/wait.h>
#    include <unistd.h>
extern char** environ;
#endif

namespace fs = std::filesystem;
using json = nlohmann::json;

#ifndef RUST_FIDELITY_FIXTURES_DIR
#    error "RUST_FIDELITY_FIXTURES_DIR must be defined at compile time"
#endif
#ifndef RUST_EXTRACTOR_BINARY
#    error "RUST_EXTRACTOR_BINARY must be defined at compile time"
#endif

namespace {

// ---------------------------------------------------------------------------
// Subprocess helper — launch topo-extract-rust with piped stdin/stdout.
// Writes `input` to the child's stdin and closes it, then drains stdout.
// Returns true on a clean spawn + wait (even if the child exited non-zero);
// outOutput receives stdout bytes and outErr a human-readable diagnostic.
// ---------------------------------------------------------------------------
bool runExtractorOnce(const std::string& binary,
                      const std::string& input,
                      std::string& outOutput,
                      std::string& outErr) {
    outOutput.clear();
    outErr.clear();

#ifdef _WIN32
    SECURITY_ATTRIBUTES sa{};
    sa.nLength = sizeof(sa);
    sa.bInheritHandle = TRUE;

    HANDLE stdinRead = nullptr, stdinWrite = nullptr;
    HANDLE stdoutRead = nullptr, stdoutWrite = nullptr;

    if (!CreatePipe(&stdinRead, &stdinWrite, &sa, 0)) {
        outErr = "CreatePipe(stdin) failed";
        return false;
    }
    if (!SetHandleInformation(stdinWrite, HANDLE_FLAG_INHERIT, 0)) {
        CloseHandle(stdinRead);
        CloseHandle(stdinWrite);
        outErr = "SetHandleInformation(stdin) failed";
        return false;
    }
    if (!CreatePipe(&stdoutRead, &stdoutWrite, &sa, 0)) {
        CloseHandle(stdinRead);
        CloseHandle(stdinWrite);
        outErr = "CreatePipe(stdout) failed";
        return false;
    }
    if (!SetHandleInformation(stdoutRead, HANDLE_FLAG_INHERIT, 0)) {
        CloseHandle(stdinRead);
        CloseHandle(stdinWrite);
        CloseHandle(stdoutRead);
        CloseHandle(stdoutWrite);
        outErr = "SetHandleInformation(stdout) failed";
        return false;
    }

    STARTUPINFOA si{};
    si.cb = sizeof(si);
    si.dwFlags = STARTF_USESTDHANDLES;
    si.hStdInput = stdinRead;
    si.hStdOutput = stdoutWrite;
    si.hStdError = GetStdHandle(STD_ERROR_HANDLE);

    PROCESS_INFORMATION pi{};
    std::string cmdLine =
        binary.find(' ') != std::string::npos ? ("\"" + binary + "\"") : binary;
    std::vector<char> cmdBuf(cmdLine.begin(), cmdLine.end());
    cmdBuf.push_back('\0');

    BOOL ok = CreateProcessA(binary.c_str(),
                             cmdBuf.data(),
                             nullptr,
                             nullptr,
                             TRUE,
                             0,
                             nullptr,
                             nullptr,
                             &si,
                             &pi);
    CloseHandle(stdinRead);
    CloseHandle(stdoutWrite);
    if (!ok) {
        CloseHandle(stdinWrite);
        CloseHandle(stdoutRead);
        outErr = "CreateProcess failed for " + binary;
        return false;
    }

    DWORD written = 0;
    if (!input.empty()) {
        WriteFile(stdinWrite,
                  input.data(),
                  static_cast<DWORD>(input.size()),
                  &written,
                  nullptr);
    }
    CloseHandle(stdinWrite); // signal EOF to child

    char buf[4096];
    DWORD bytesRead = 0;
    while (ReadFile(stdoutRead, buf, sizeof(buf), &bytesRead, nullptr) &&
           bytesRead > 0) {
        outOutput.append(buf, buf + bytesRead);
    }
    CloseHandle(stdoutRead);

    WaitForSingleObject(pi.hProcess, 30000);
    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);
    return true;
#else
    int inPipe[2] = {-1, -1};
    int outPipe[2] = {-1, -1};
    if (pipe(inPipe) != 0) {
        outErr = "pipe(stdin) failed";
        return false;
    }
    if (pipe(outPipe) != 0) {
        ::close(inPipe[0]);
        ::close(inPipe[1]);
        outErr = "pipe(stdout) failed";
        return false;
    }

    posix_spawn_file_actions_t actions;
    posix_spawn_file_actions_init(&actions);
    posix_spawn_file_actions_adddup2(&actions, inPipe[0], STDIN_FILENO);
    posix_spawn_file_actions_adddup2(&actions, outPipe[1], STDOUT_FILENO);
    posix_spawn_file_actions_addclose(&actions, inPipe[0]);
    posix_spawn_file_actions_addclose(&actions, inPipe[1]);
    posix_spawn_file_actions_addclose(&actions, outPipe[0]);
    posix_spawn_file_actions_addclose(&actions, outPipe[1]);

    std::vector<char*> argv;
    argv.push_back(const_cast<char*>(binary.c_str()));
    argv.push_back(nullptr);

    pid_t pid = -1;
    int err = posix_spawn(
        &pid, binary.c_str(), &actions, nullptr, argv.data(), environ);
    posix_spawn_file_actions_destroy(&actions);

    ::close(inPipe[0]);
    ::close(outPipe[1]);

    if (err != 0) {
        ::close(inPipe[1]);
        ::close(outPipe[0]);
        outErr = "posix_spawn failed for " + binary +
                 " (errno " + std::to_string(err) + ")";
        return false;
    }

    const char* p = input.data();
    size_t remaining = input.size();
    while (remaining > 0) {
        ssize_t n = ::write(inPipe[1], p, remaining);
        if (n <= 0) break;
        p += n;
        remaining -= static_cast<size_t>(n);
    }
    ::close(inPipe[1]); // signal EOF to child

    char buf[4096];
    while (true) {
        ssize_t n = ::read(outPipe[0], buf, sizeof(buf));
        if (n <= 0) break;
        outOutput.append(buf, buf + n);
    }
    ::close(outPipe[0]);

    int status = 0;
    waitpid(pid, &status, 0);
    return true;
#endif
}

// ---------------------------------------------------------------------------
// File I/O helpers
// ---------------------------------------------------------------------------

std::string readFile(const fs::path& path) {
    std::ifstream ifs(path, std::ios::binary);
    if (!ifs) return {};
    std::ostringstream oss;
    oss << ifs.rdbuf();
    return oss.str();
}

void writeFile(const fs::path& path, const std::string& text) {
    std::error_code ec;
    fs::create_directories(path.parent_path(), ec);
    std::ofstream ofs(path, std::ios::binary);
    ofs << text;
}

bool envBool(const char* name) {
    const char* v = std::getenv(name);
    if (!v || !*v) return false;
    std::string s(v);
    return !(s == "0" || s == "false" || s == "FALSE");
}

// ---------------------------------------------------------------------------
// Request template substitution: resolve @INPUT@ to the absolute path of
// the fixture's input.rs file so the extractor is cwd-independent.
// ---------------------------------------------------------------------------
std::string resolveRequest(const std::string& tmpl, const fs::path& inputPath) {
    const std::string placeholder = "@INPUT@";
    std::string absPath = fs::absolute(inputPath).string();

#ifdef _WIN32
    // JSON strings need backslashes doubled; the request is embedded raw in
    // a JSON string literal.
    std::string escaped;
    escaped.reserve(absPath.size());
    for (char c : absPath) {
        if (c == '\\')
            escaped.append("\\\\");
        else
            escaped.push_back(c);
    }
    absPath = std::move(escaped);
#endif

    std::string out = tmpl;
    size_t pos = 0;
    while ((pos = out.find(placeholder, pos)) != std::string::npos) {
        out.replace(pos, placeholder.size(), absPath);
        pos += absPath.size();
    }
    return out;
}

// Normalize TranspileModule JSON so comparisons are order-insensitive on the
// function list. The rust extractor collects functions into a HashMap whose
// iteration order is non-deterministic.
void normalizeModule(json& module) {
    auto sortByQName = [](json& arr) {
        std::vector<json> items(arr.begin(), arr.end());
        std::sort(items.begin(), items.end(), [](const json& a, const json& b) {
            const std::string an = a.value("qualifiedName", std::string{});
            const std::string bn = b.value("qualifiedName", std::string{});
            return an < bn;
        });
        arr = json::array();
        for (auto& it : items) arr.push_back(std::move(it));
    };

    if (module.is_object() && module.contains("functions") &&
        module["functions"].is_array()) {
        sortByQName(module["functions"]);
    }
    if (module.is_object() && module.contains("types") &&
        module["types"].is_array()) {
        sortByQName(module["types"]);
    }
}

// Short diff hint on mismatch — summarize top-level keys and first function
// qualified names so failures are self-describing without a full dump.
std::string diffHint(const json& expected, const json& actual) {
    std::ostringstream oss;
    auto keys = [](const json& j) -> std::string {
        if (!j.is_object()) return j.type_name();
        std::string s;
        bool first = true;
        for (auto it = j.begin(); it != j.end(); ++it) {
            if (!first) s += ", ";
            s += it.key();
            first = false;
        }
        return s;
    };
    auto fnNames = [](const json& j) -> std::string {
        if (!j.is_object() || !j.contains("functions") ||
            !j["functions"].is_array())
            return "<none>";
        std::string s;
        bool first = true;
        for (const auto& fn : j["functions"]) {
            if (!first) s += ", ";
            s += fn.value("qualifiedName", std::string{"?"});
            first = false;
        }
        return s;
    };
    oss << "\n  expected keys: " << keys(expected);
    oss << "\n  actual   keys: " << keys(actual);
    oss << "\n  expected functions: " << fnNames(expected);
    oss << "\n  actual   functions: " << fnNames(actual);
    return oss.str();
}

// ---------------------------------------------------------------------------
// Fixture discovery and per-fixture driver
// ---------------------------------------------------------------------------
std::vector<fs::path> discoverFixtures() {
    std::vector<fs::path> result;
    fs::path root(RUST_FIDELITY_FIXTURES_DIR);
    std::error_code ec;
    if (!fs::exists(root, ec) || !fs::is_directory(root, ec)) return result;
    for (const auto& entry : fs::directory_iterator(root, ec)) {
        if (!entry.is_directory()) continue;
        const auto name = entry.path().filename().string();
        if (name.empty() || name[0] == '.') continue;
        result.push_back(entry.path());
    }
    std::sort(result.begin(), result.end());
    return result;
}

void runFixture(const fs::path& fixtureDir) {
    SCOPED_TRACE("fixture: " + fixtureDir.filename().string());

    fs::path skipMarker = fixtureDir / "SKIP.md";
    if (fs::exists(skipMarker)) {
        std::string reason = readFile(skipMarker);
        size_t nl = reason.find('\n');
        if (nl != std::string::npos) reason = reason.substr(0, nl);
        GTEST_SKIP() << "SKIP.md present: " << reason;
    }

    fs::path requestPath = fixtureDir / "request.json";
    fs::path expectedPath = fixtureDir / "expected.json";
    fs::path inputPath = fixtureDir / "input.rs";

    ASSERT_TRUE(fs::exists(requestPath))
        << "missing request.json at " << requestPath;
    ASSERT_TRUE(fs::exists(inputPath))
        << "missing input.rs at " << inputPath;

    std::string tmpl = readFile(requestPath);
    ASSERT_FALSE(tmpl.empty()) << "empty request.json at " << requestPath;
    std::string resolved = resolveRequest(tmpl, inputPath);

    json request;
    try {
        request = json::parse(resolved);
    } catch (const json::exception& e) {
        FAIL() << "failed to parse request.json after @INPUT@ substitution: "
               << e.what();
        return;
    }
    std::string requestStr = request.dump();

    std::string actualRaw;
    std::string runErr;
    bool ok =
        runExtractorOnce(RUST_EXTRACTOR_BINARY, requestStr, actualRaw, runErr);
    ASSERT_TRUE(ok) << "failed to run extractor: " << runErr;
    ASSERT_FALSE(actualRaw.empty())
        << "extractor produced no output (binary=" << RUST_EXTRACTOR_BINARY
        << ")";

    json actual;
    try {
        actual = json::parse(actualRaw);
    } catch (const json::exception& e) {
        FAIL() << "extractor output is not valid JSON: " << e.what()
               << "\n  raw: " << actualRaw;
        return;
    }
    normalizeModule(actual);

    const bool forceUpdate = envBool("TOPO_FIDELITY_UPDATE");
    if (forceUpdate || !fs::exists(expectedPath)) {
        writeFile(expectedPath, actual.dump(2) + "\n");
        SUCCEED() << "golden written to " << expectedPath
                  << (forceUpdate ? " (TOPO_FIDELITY_UPDATE=1)"
                                  : " (bootstrap)");
        return;
    }

    json expected;
    try {
        expected = json::parse(readFile(expectedPath));
    } catch (const json::exception& e) {
        FAIL() << "failed to parse expected.json: " << e.what();
        return;
    }
    normalizeModule(expected);

    EXPECT_EQ(expected, actual) << "TranspileModule JSON mismatch"
                                << diffHint(expected, actual);
}

void runNamed(const char* name) {
    fs::path root(RUST_FIDELITY_FIXTURES_DIR);
    runFixture(root / name);
}

} // namespace

// ---------------------------------------------------------------------------
// Per-fixture TEST declarations — one test per fixture for granular pass/fail.
// ---------------------------------------------------------------------------

TEST(RustExtractorFidelity, BasicFunction) { runNamed("01_basic_fn"); }
TEST(RustExtractorFidelity, TraitImpl) { runNamed("02_trait_impl"); }
TEST(RustExtractorFidelity, GenericFn) { runNamed("03_generic_fn"); }
TEST(RustExtractorFidelity, UsePath) { runNamed("04_use_path"); }
TEST(RustExtractorFidelity, ModTree) { runNamed("05_mod_tree"); }
TEST(RustExtractorFidelity, UnsafeFn) { runNamed("06_unsafe_fn"); }
TEST(RustExtractorFidelity, MacroRules) { runNamed("07_macro_rules"); }
TEST(RustExtractorFidelity, LifetimeParam) { runNamed("08_lifetime_param"); }
TEST(RustExtractorFidelity, ClosureBody) { runNamed("09_closure_body"); }
TEST(RustExtractorFidelity, MatchControlFlow) {
    runNamed("10_match_control_flow");
}
TEST(RustExtractorFidelity, AsyncFn) { runNamed("11_async_fn"); }
TEST(RustExtractorFidelity, StructImplMethods) {
    runNamed("12_struct_impl_methods");
}
// `fn id<T>(x: T) -> T` — a clean MVP type parameter: bare `T` recovered
// into templateParams, no bounds ⇒ source fidelity, empty unsupported.
// (The `'a` lifetime-records-unsupported path is covered by
// 08_lifetime_param, whose golden asserts inferred + the dropped-lifetime
// note with no templateParams key.)
TEST(RustExtractorFidelity, GenericTypeParam) {
    runNamed("13_generic_type_param");
}
// `pub struct Box<T> { value: T }` — the headline payoff of fixing the
// extractor's dropped item-struct types: a struct with generic type
// parameters now round-trips through TranspileType.templateParams. Without
// the extractor change the type was silently dropped (module.types stayed
// empty), so this fixture also pins the regression of that gap.
TEST(RustExtractorFidelity, GenericStruct) {
    runNamed("14_generic_struct");
}

// `pub struct Container<T = i32> { value: T }` — Rust default type-param
// captured as wire `default: TypeNode` rather than dropped + downgraded.
// Validates the end-to-end shape: bare type-param name, no bound, default
// present as a TypeNode (no surrounding `unsupported` note for the default).
TEST(RustExtractorFidelity, GenericStructDefault) {
    runNamed("15_generic_struct_default");
}

// `pub fn dump<T: Clone + std::fmt::Debug>(_: T)` — multi-bound captured as
// the wire `bounds: [TypeNode]` array (instead of legacy single `bound`).
TEST(RustExtractorFidelity, MultiBoundFn) {
    runNamed("16_multi_bound_fn");
}

// `pub struct Buffer<const N: usize> { data: [u8; N] }` — Rust const
// generic captured as kind="nontype" with `bound` carrying the value type
// (usize). Pre-fix the param dropped to `unsupported` and the struct
// downgraded; now the struct stays source fidelity and N round-trips.
TEST(RustExtractorFidelity, ConstGeneric) {
    runNamed("17_const_generic");
}

// `pub fn collect<T: Iterator<Item = u8>>(_it: T)` — associated-type
// binding captured as `assocBindings: [{name: "Item", type: u8}]` on the
// bound TypeNode rather than dropped + downgraded. Validates the end-to-
// end shape: bare type-param name with a parameterised `Iterator` bound
// whose last-segment `AssocType` binding is recovered (no `unsupported`
// note for the assoc clause).
TEST(RustExtractorFidelity, AssocTypeBound) {
    runNamed("18_assoc_type_bound");
}

// `pub struct Holder<'a, T: 'a> { value: &'a T }` — lifetime params on
// the wire: `<'a, T>` round-trips with a `kind="lifetime"` entry for `'a`
// (name stored sans-apostrophe) and a `T: 'a` entry whose bound TypeNode
// keeps the apostrophe (`["'a"]`) to disambiguate it from a regular
// trait path.
TEST(RustExtractorFidelity, LifetimeParamStruct) {
    runNamed("19_lifetime_param");
}

// `pub fn map<F>(_f: F) where F: for<'a> Fn(&'a u8) -> &'a u8 {}` —
// higher-ranked trait bound: the trait bound on F carries
// `hrtbLifetimes: ["a"]` (sans-apostrophe; the `'` is added at emit
// time). The parenthesised Fn-trait inputs flow into `templateArgs` and
// the output into a synthesised `assocBindings` entry named "Output" —
// RustEmitter detects this shape and re-renders parenthesised
// `Fn(&'a u8) -> &'a u8`; other host emitters render the angle-bracketed
// `Fn<...>` form and silently drop the HRTB prefix.
TEST(RustExtractorFidelity, HigherRankedTraitBound) {
    runNamed("20_hrtb");
}

// `pub struct Buf<const N: usize = 16> { data: [u8; N] }` — const
// generic default literal: the const-generic carries a literal default
// on the wire as a string `defaultValue: "16"` (separate from `default`,
// which is TypeNode-shaped and TypeParam-only). The default round-trips
// literal-verbatim.
TEST(RustExtractorFidelity, ConstGenericDefault) {
    runNamed("21_const_default");
}

// Sanity check: the fixture root must contain at least 10 entries.
TEST(RustExtractorFidelity, FixtureRootHasAtLeastTenEntries) {
    auto dirs = discoverFixtures();
    EXPECT_GE(dirs.size(), 10u)
        << "fixture acceptance: at least 10 extractor fidelity fixtures required;"
           " found "
        << dirs.size() << " in " << RUST_FIDELITY_FIXTURES_DIR;
}
