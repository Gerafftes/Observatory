# ESP32 CSI Node Firmware (ADR-018)
# Requires ESP-IDF v5.4
cmake_minimum_required(VERSION 3.16)

set(EXTRA_COMPONENT_DIRS "")

# Read firmware version from version.txt so esp_app_get_description()->version
# matches the release tag.  Fixes issue #354 (version mismatch after flashing).
file(STRINGS "${CMAKE_CURRENT_LIST_DIR}/version.txt" PROJECT_VER LIMIT_COUNT 1)
string(STRIP "${PROJECT_VER}" PROJECT_VER)
if(OBSERVATORY_RX_RELEASE)
    set(PROJECT_VER "rx-unified-1")
    set(EXTRA_COMPONENT_DIRS "${CMAKE_CURRENT_LIST_DIR}/rx_release")
    set(COMPONENTS rx_release)
    set(EXCLUDE_COMPONENTS main wasm3)
endif()

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
if(OBSERVATORY_RX_RELEASE)
    idf_build_set_property(DEPENDENCIES_LOCK "${CMAKE_CURRENT_LIST_DIR}/dependencies.rx-release.lock")
endif()
project(esp32-csi-node)
