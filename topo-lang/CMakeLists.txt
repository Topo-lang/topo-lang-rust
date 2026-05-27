add_library(TopoRustPlugin
    RustPlugin.cpp
)
add_library(topo::lang-rust::TopoRustPlugin ALIAS TopoRustPlugin)

target_include_directories(TopoRustPlugin
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-lang>
)
target_link_libraries(TopoRustPlugin PUBLIC
    topo::lang::TopoLang
    TopoRustInit
    TopoRustAnalysis
    TopoRustCheck
    TopoRustTranspile
    TopoRustLSP
)
topo_set_compiler_flags(TopoRustPlugin)

install(TARGETS TopoRustPlugin
    EXPORT topo-lang-rustTargets
    ARCHIVE  DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-lang
)
install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-lang
    FILES_MATCHING PATTERN "*.h"
)
