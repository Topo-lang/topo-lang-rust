add_library(TopoRustTranspile
    RustEmitter.cpp
)
add_library(topo::lang-rust::TopoRustTranspile ALIAS TopoRustTranspile)

target_include_directories(TopoRustTranspile PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-transpile>
)
# topo-core/include comes via topo::core::TopoTranspile's PUBLIC propagation.
target_link_libraries(TopoRustTranspile PUBLIC topo::core::TopoTranspile)
topo_set_compiler_flags(TopoRustTranspile)

install(TARGETS TopoRustTranspile
    EXPORT topo-lang-rustTargets
    ARCHIVE  DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-transpile
)
install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-transpile
    FILES_MATCHING PATTERN "*.h"
)
