cmake_minimum_required(VERSION 3.24)
project(rinui-rs VERSION 0.1.0 LANGUAGES C CXX)
set(RINUI_RS_VERSION_STRING "0.1.0-alpha.1")

option(RINUI_ENABLE_QT_SEARCH "Locate Qt6 via find_package and forward Qt6::qmake to Cargo" ON)
option(RINUI_BUILD_RELEASE "Build Cargo targets with --release" OFF)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
include(RinUIHost)
include(BundleHelpers)

enable_testing()
add_test(
    NAME rinui_upstream_audit
    COMMAND ${Python3_EXECUTABLE} "${CMAKE_CURRENT_SOURCE_DIR}/scripts/audit_upstream.py"
)
