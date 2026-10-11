cmake_minimum_required(VERSION 3.16)

file(STRINGS "${CMAKE_CURRENT_LIST_DIR}/version.txt" PROJECT_VER LIMIT_COUNT 1)
string(STRIP "${PROJECT_VER}" PROJECT_VER)

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(esp32-csi-tx VERSION ${PROJECT_VER})
