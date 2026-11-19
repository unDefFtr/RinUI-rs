# Desktop Qt deployment helpers for rinui-rs standalone bundles.

function(rinui_add_package_target TARGET_NAME BINARY_PATH OUTPUT_DIR)
    if(APPLE)
        add_custom_target(${TARGET_NAME}
            COMMAND "${CMAKE_CURRENT_SOURCE_DIR}/scripts/package_macos.sh"
                "${BINARY_PATH}" "${OUTPUT_DIR}"
            DEPENDS rinui_gallery_rust
            WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
            COMMENT "Bundling macOS .app with macdeployqt"
        )
    elseif(WIN32)
        find_program(PWSH_EXECUTABLE NAMES pwsh powershell REQUIRED)
        add_custom_target(${TARGET_NAME}
            COMMAND "${PWSH_EXECUTABLE}" -NoProfile -ExecutionPolicy Bypass
                -File "${CMAKE_CURRENT_SOURCE_DIR}/scripts/package_windows.ps1"
                -BinaryPath "${BINARY_PATH}" -OutputDir "${OUTPUT_DIR}"
            DEPENDS rinui_gallery_rust
            WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
            COMMENT "Bundling Windows release directory with windeployqt"
        )
    else()
        add_custom_target(${TARGET_NAME}
            COMMAND "${CMAKE_CURRENT_SOURCE_DIR}/scripts/package_linux.sh"
                "${BINARY_PATH}" "${OUTPUT_DIR}"
            DEPENDS rinui_gallery_rust
            WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
            COMMENT "Bundling Linux Qt runtime staging directory"
        )
    endif()
endfunction()

set(RINUI_GALLERY_BINARY_DEFAULT
    "${CMAKE_CURRENT_SOURCE_DIR}/target/${RINUI_CARGO_OUTPUT_SUBDIR}/rinui-gallery${CMAKE_EXECUTABLE_SUFFIX}"
)
rinui_add_package_target(
    rinui_package
    "${RINUI_GALLERY_BINARY_DEFAULT}"
    "${CMAKE_CURRENT_SOURCE_DIR}/dist"
)
