add_library(TopoRustCheck STATIC RustCheckRunner.cpp)
add_library(topo::lang-rust::TopoRustCheck ALIAS TopoRustCheck)

target_include_directories(TopoRustCheck PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-check/runner>
)
target_link_libraries(TopoRustCheck PUBLIC topo::lang::TopoLang)
topo_set_compiler_flags(TopoRustCheck)

install(TARGETS TopoRustCheck
    EXPORT topo-lang-rustTargets
    ARCHIVE  DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-check/runner
)
install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-check/runner
    FILES_MATCHING PATTERN "*.h"
)
