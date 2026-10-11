cmake_minimum_required(VERSION 3.16)

set(IDF_TARGET "esp32c3")
set(PROJECT_VER "0.2.0-diag")
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(mmwave2-ap-probe)
