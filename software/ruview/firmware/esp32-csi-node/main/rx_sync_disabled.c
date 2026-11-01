/* The common S3 measurement release makes no radio synchronization claim. */
#include "c6_sync_espnow.h"
bool c6_sync_espnow_is_valid(void) { return false; }
bool c6_sync_espnow_is_leader(void) { return false; }
uint64_t c6_sync_espnow_get_epoch_us(void) { return 0; }
int64_t c6_sync_espnow_get_offset_us_smoothed(void) { return 0; }
uint32_t c6_sync_espnow_get_age_ms(void) { return UINT32_MAX; }
