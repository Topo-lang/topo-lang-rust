add_library(TopoRustLSP
    RustAnalyzerBridge.cpp
)
add_library(topo::lang-rust::TopoRustLSP ALIAS TopoRustLSP)

target_include_directories(TopoRustLSP PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-lsp>
)
target_link_libraries(TopoRustLSP PUBLIC topo::core::TopoLSPBridge)
topo_set_compiler_flags(TopoRustLSP)

install(TARGETS TopoRustLSP
    EXPORT topo-lang-rustTargets
    ARCHIVE  DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-lsp
)
install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-lsp
    FILES_MATCHING PATTERN "*.h"
)
