#ifndef TX_SAFETY_H
#define TX_SAFETY_H
#include "sdkconfig.h"
/* These paths can burn eFuses or change data needed by the original app. */
#if defined(CONFIG_SECURE_BOOT) || defined(CONFIG_SECURE_FLASH_ENC_ENABLED) \
    || defined(CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK) || defined(CONFIG_NVS_ENCRYPTION)
#error "TX migration must not enable security-state changes"
#endif
#if defined(CONFIG_ESP_WIFI_NVS_ENABLED) \
    || defined(CONFIG_ESP_PHY_CALIBRATION_AND_DATA_STORAGE) \
    || defined(CONFIG_ESP_PHY_INIT_DATA_IN_PARTITION) \
    || defined(CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH)
#error "TX migration must not write shared WiFi/PHY/coredump data"
#endif
#endif
