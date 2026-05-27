# embed the Rust native-lldb formatter into the init
# provider (mirrors topo-lang-cpp/topo-init/CMakeLists.txt).
set(_RUST_LLDB_FORMATTER
    "${CMAKE_CURRENT_SOURCE_DIR}/../topo-debug/lldb_formatter.py")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    "${_RUST_LLDB_FORMATTER}")
file(READ "${_RUST_LLDB_FORMATTER}" TOPO_RUST_LLDB_FORMATTER_SOURCE)
set(_RUST_LLDB_ASSET "${CMAKE_CURRENT_BINARY_DIR}/RustLldbFormatterAsset.cpp")
configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/RustLldbFormatterAsset.cpp.in"
    "${_RUST_LLDB_ASSET}"
    @ONLY)

add_library(TopoRustInit STATIC
    RustInitTemplateProvider.cpp
    "${_RUST_LLDB_ASSET}")
add_library(topo::lang-rust::TopoRustInit ALIAS TopoRustInit)

target_include_directories(TopoRustInit PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-init>
)
target_link_libraries(TopoRustInit PUBLIC topo::lang::TopoLang)
topo_set_compiler_flags(TopoRustInit)

install(TARGETS TopoRustInit
    EXPORT topo-lang-rustTargets
    ARCHIVE  DESTINATION ${CMAKE_INSTALL_LIBDIR}
    INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-init
)
install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo-lang-rust/topo-init
    FILES_MATCHING PATTERN "*.h"
)
