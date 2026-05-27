add_library(TopoRustAnalysis
    RustAnalysisProvider.cpp
    extract/RustCallEdgeExtractor.cpp
    extract/RustCallSiteExtractor.cpp
    extract/RustImportExtractor.cpp
    extract/RustLSPCallSiteExtractor.cpp
    extract/RustLSPImportExtractor.cpp
    extract/RustLSPSymbolExtractor.cpp
    extract/RustSymbolAccessExtractor.cpp
    extract/RustSymbolExtractor.cpp
    catalog/RustUnsafeCatalog.cpp
    catalog/RustSafePatterns.cpp
    catalog/RustSafetyAnalyzer.cpp
    stub/RustStubGenerator.cpp
)

find_path(TOMLPLUSPLUS_INCLUDE_DIR toml++/toml.hpp REQUIRED)

add_library(topo::lang-rust::TopoRustAnalysis ALIAS TopoRustAnalysis)

target_include_directories(TopoRustAnalysis
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-check/analysis>
    PRIVATE
        ${CMAKE_CURRENT_SOURCE_DIR}/extract
        ${CMAKE_CURRENT_SOURCE_DIR}/catalog
        ${CMAKE_CURRENT_SOURCE_DIR}/stub
        ${TOMLPLUSPLUS_INCLUDE_DIR}
)
# topo-core's include path comes via topo::core::TopoCheck PUBLIC
# propagation. TopoRustLSP carries the local Rust LSP bridge headers.
target_link_libraries(TopoRustAnalysis
    PUBLIC topo::core::TopoCheck TopoRustLSP
    PRIVATE topo::core::TopoPlatform)

# RustLSPCallSiteExtractor.cpp / RustLSPImportExtractor.cpp include
# "analysis/extract/CppCallSiteExtractor.h" / "analysis/extract/CppImportExtractor.h"
# from topo-lang-cpp's source tree. The "analysis/" prefix means we need the
# PARENT of TopoCppAnalysis's source dir on the include path.
# When embedded under the meta (or with topo-lang-cpp on CMAKE_PREFIX_PATH),
# topo::lang-cpp::TopoCppAnalysis exists and we pull its SOURCE_DIR's parent.
if(TARGET topo::lang-cpp::TopoCppAnalysis)
    get_target_property(_cpp_analysis_src
        topo::lang-cpp::TopoCppAnalysis SOURCE_DIR)
    if(_cpp_analysis_src)
        target_include_directories(TopoRustAnalysis PRIVATE
            "${_cpp_analysis_src}/..")
    endif()
endif()

# TOPO_SOURCE_DIR is used by RustSafePatterns.cpp for source-tree catalog
# discovery (with runtime fallback if not present). In standalone mode
# we point it at the topo-lang-rust source root; install-time consumers
# can override via -DTOPO_SOURCE_DIR=<your-checkout-root>. The runtime
# code returns false safely when the catalog can't be located.
target_compile_definitions(TopoRustAnalysis PRIVATE
    TOPO_SOURCE_DIR="${CMAKE_SOURCE_DIR}/.."
)
topo_set_compiler_flags(TopoRustAnalysis)

install(TARGETS TopoRustAnalysis
    EXPORT topo-lang-rustTargets
    ARCHIVE  DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-check/analysis
)
install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-check/analysis
    FILES_MATCHING PATTERN "*.h"
)
