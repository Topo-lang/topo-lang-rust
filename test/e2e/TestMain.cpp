#include "RustPlugin.h"
#include <gtest/gtest.h>

int main(int argc, char** argv) {
    topo::lang::registerRustPlugin();
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}
