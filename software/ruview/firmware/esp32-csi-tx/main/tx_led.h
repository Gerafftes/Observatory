#ifndef TX_LED_H
#define TX_LED_H
#include "tx_config.h"

/* Called once after sounding starts. Does not fail the network on LED errors. */
esp_err_t tx_led_start(const tx_config_t *config);
#endif
