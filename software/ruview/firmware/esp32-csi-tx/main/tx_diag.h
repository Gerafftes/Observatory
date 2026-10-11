#ifndef TX_DIAG_H
#define TX_DIAG_H

#include "esp_err.h"
#include "esp_log.h"

/* Only constant operation names and numeric errors: never profile contents. */
#define TX_CHECKPOINT(stage) ESP_LOGI("tx_boot", "TX_BOOT %s", stage)
#define TX_ERROR(stage, error) \
    ESP_LOGE("tx_boot", "TX_BOOT ERROR stage=%s code=0x%x", stage, (unsigned)(error))
#define TX_TRY(call) do { \
    esp_err_t tx_error = (call); \
    if (tx_error != ESP_OK) { TX_ERROR(#call, tx_error); return tx_error; } \
} while (0)
#define TX_STEP(call) do { \
    TX_CHECKPOINT("BEGIN " #call); \
    TX_TRY(call); \
} while (0)

#endif
