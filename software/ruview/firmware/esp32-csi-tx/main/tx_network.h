#ifndef TX_NETWORK_H
#define TX_NETWORK_H
#include "tx_config.h"
#define TX_POWER_QUARTER_DBM 44
#define TX_DHCP_FIRST "192.168.4.2"
#define TX_DHCP_LAST "192.168.4.49"
esp_err_t tx_network_start(const tx_config_t *config);
esp_err_t tx_network_check(const tx_config_t *config);
esp_err_t tx_network_restart(const tx_config_t *config);
void tx_network_stop(void);
#endif
