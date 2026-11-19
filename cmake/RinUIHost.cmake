find_package(Python3 REQUIRED COMPONENTS Interpreter)

find_program(CARGO_EXECUTABLE cargo REQUIRED)

set(RINUI_CARGO_ENV "")
if(RINUI_ENABLE_QT_SEARCH)
    find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2)
    get_target_property(RINUI_QMAKE_PATH Qt6::qmake IMPORTED_LOCATION)
    if(RINUI_QMAKE_PATH)
        list(APPEND RINUI_CARGO_ENV "QMAKE=${RINUI_QMAKE_PATH}")
    endif()
endif()

set(RINUI_CARGO_PROFILE_ARGS "")
set(RINUI_CARGO_OUTPUT_SUBDIR "debug")
if(RINUI_BUILD_RELEASE OR CMAKE_BUILD_TYPE STREQUAL "Release" OR CMAKE_BUILD_TYPE STREQUAL "RelWithDebInfo")
    list(APPEND RINUI_CARGO_PROFILE_ARGS "--release")
    set(RINUI_CARGO_OUTPUT_SUBDIR "release")
endif()

add_custom_target(rinui_audit
    COMMAND ${Python3_EXECUTABLE} "${CMAKE_CURRENT_SOURCE_DIR}/scripts/audit_upstream.py"
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    COMMENT "Auditing pinned upstream/Rin-UI submodule and qmldir contracts"
)

add_custom_target(rinui_host_rust
    COMMAND ${CMAKE_COMMAND} -E env ${RINUI_CARGO_ENV}
        ${CARGO_EXECUTABLE} build -p rinui-host ${RINUI_CARGO_PROFILE_ARGS}
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    DEPENDS rinui_audit
    COMMENT "Building rinui-host Rust/CXX-Qt library"
)

add_custom_target(rinui_gallery_rust
    COMMAND ${CMAKE_COMMAND} -E env ${RINUI_CARGO_ENV}
        ${CARGO_EXECUTABLE} build -p rinui-gallery ${RINUI_CARGO_PROFILE_ARGS}
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    DEPENDS rinui_host_rust
    COMMENT "Building rinui-gallery reference application (${RINUI_CARGO_OUTPUT_SUBDIR})"
)

add_custom_target(rinui_test
    COMMAND ${CMAKE_COMMAND} -E env ${RINUI_CARGO_ENV}
        ${CARGO_EXECUTABLE} test --workspace
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    DEPENDS rinui_audit
    COMMENT "Running rinui-rs workspace unit and integration tests"
)
