add_library(TopoRustDriver STATIC
    RustDriver.cpp
)
add_library(topo::lang-rust::TopoRustDriver ALIAS TopoRustDriver)

target_include_directories(TopoRustDriver PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
    $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}/topo/lang-rust/build>
)

# toml++ as header-only (for Topo.toml parsing in RustDriver)
find_path(TOMLPLUSPLUS_INCLUDE_DIR toml++/toml.hpp)
target_include_directories(TopoRustDriver PRIVATE ${TOMLPLUSPLUS_INCLUDE_DIR})

target_link_libraries(TopoRustDriver PUBLIC
    topo::core::TopoPlatform
    topo::core::TopoBuildConfig
)

install(TARGETS TopoRustDriver
    EXPORT topo-lang-rustTargets
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
)
install(FILES RustDriver.h
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}/topo/lang-rust/build)
