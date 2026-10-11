idf_component_register(
    SRCS "app_main.c" "tx_config.c" "tx_ota.c" "tx_packet.c" "tx_scheduler.c" "tx_network.c" "tx_boot.c" "tx_led.c"
    INCLUDE_DIRS "."
    REQUIRES esp_wifi esp_netif esp_event nvs_flash app_update esp_http_server
             esp_app_format esp_timer esp_system lwip mbedtls
    PRIV_REQUIRES espressif__led_strip
)

target_link_options(${COMPONENT_LIB} INTERFACE
    "-Wl,--wrap=netbuf_alloc" "-Wl,--wrap=esp_wifi_internal_tx")

execute_process(
    COMMAND git -C "${CMAKE_CURRENT_LIST_DIR}" rev-parse --short=12 HEAD
    OUTPUT_VARIABLE TX_GIT_HASH
    OUTPUT_STRIP_TRAILING_WHITESPACE
    ERROR_QUIET
)
if(NOT TX_GIT_HASH)
    set(TX_GIT_HASH "unknown")
endif()

set(TX_SOURCE_SHA "unmanifested")
include("${CMAKE_SOURCE_DIR}/source_identity.cmake" OPTIONAL)
target_compile_definitions(${COMPONENT_LIB} PRIVATE TX_GIT_HASH="${TX_GIT_HASH}")
target_compile_definitions(${COMPONENT_LIB} PRIVATE TX_SOURCE_SHA="${TX_SOURCE_SHA}")
