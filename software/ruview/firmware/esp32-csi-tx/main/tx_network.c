#include "tx_network.h"
#include "tx_safety.h"
#include "tx_diag.h"
#include "esp_wifi_default.h"
#include <string.h>
#include "esp_event.h"
#include "esp_mac.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#include "esp_timer.h"
#include "lwip/inet.h"
#include "dhcpserver/dhcpserver.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

static esp_netif_t *s_ap;
static int64_t s_wifi_start_us;

/* ESP-IDF 5.4 esp_wifi.h: ISO alpha-2 is distinct from the third octet.
   I/O/X retain their meaning; every other octet means unspecified (' ').
   Schema 1 stores an alpha-2 C string, so its terminator means unspecified. */
static unsigned country_environment(unsigned char octet)
{
    return octet == 'I' || octet == 'O' || octet == 'X' ? octet : ' ';
}

/* The pinned ESP32-S3 libnet80211 DE table stores 20 independently of the
   PHY limit. See docs/project/tx1-power-diagnostic-20261003.md and its ELF
   evidence. The esp_wifi.h claim that set_max_tx_power maps this field is
   not implemented by that binary. This is NOT our requested TX power. */
#define TX_DE_COUNTRY_TABLE_DBM 20

static esp_err_t trace_power_state(const char *phase, bool read_power)
{
    wifi_country_t country = {0};
    int8_t power = 0;
    const int64_t begin_us = esp_timer_get_time();
    esp_err_t country_rc = esp_wifi_get_country(&country);
    /* The power API is only valid after WiFi start. */
    esp_err_t power_rc = read_power ? esp_wifi_get_max_tx_power(&power) : ESP_OK;
    const int64_t end_us = esp_timer_get_time();
    ESP_LOGI("tx_boot", "TX_POWER phase=%s begin_us=%lld end_us=%lld "
             "country_rc=0x%x cc0=%d cc1=%d cc2=%d schan=%d nchan=%d country_dBm=%d policy=%d "
             "power_read=%d power_rc=0x%x power_quarter_dBm=%d unavailable=-1",
             phase, (long long)begin_us, (long long)end_us,
             (unsigned)country_rc,
             country_rc == ESP_OK ? (unsigned char)country.cc[0] : -1,
             country_rc == ESP_OK ? (unsigned char)country.cc[1] : -1,
             country_rc == ESP_OK ? (unsigned char)country.cc[2] : -1,
             country_rc == ESP_OK ? country.schan : -1,
             country_rc == ESP_OK ? country.nchan : -1,
             country_rc == ESP_OK ? country.max_tx_power : -1,
             country_rc == ESP_OK ? (int)country.policy : -1,
             read_power, (unsigned)power_rc, read_power && power_rc == ESP_OK ? power : -1);
    if (country_rc != ESP_OK) { TX_ERROR("esp_wifi_get_country", country_rc); return country_rc; }
    if (power_rc != ESP_OK) { TX_ERROR("esp_wifi_get_max_tx_power", power_rc); return power_rc; }
    return ESP_OK;
}

/* Log only numeric fields and equality results for credential-bearing fields.
   sample time is the beginning of a sequential read, not an atomic snapshot. */
#define CHECK_FIELD(name, operation, field, expected, actual, condition) do { \
    if (!(condition)) { \
        ESP_LOGE("tx_boot", "TX_COMPARE op=%s rc=0x%x field=%s expected=%lld actual=%lld " \
                 "boot_us=%lld since_start_us=%lld", operation, (unsigned)ESP_ERR_INVALID_STATE, \
                 field, (long long)(expected), (long long)(actual), \
                 (long long)sampled_us, (long long)(sampled_us - s_wifi_start_us)); \
        TX_ERROR("CHECK_" name, ESP_ERR_INVALID_STATE); \
        return ESP_ERR_INVALID_STATE; \
    } \
} while (0)

static void radio_event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    (void)arg;
    (void)base;
    (void)data;
    /* Event time is separate from the main task's DHCP-ready observation. */
    if (id == WIFI_EVENT_AP_START || id == WIFI_EVENT_AP_STACONNECTED
        || id == WIFI_EVENT_AP_STADISCONNECTED) {
        ESP_LOGI("tx_boot", "TX_RADIO event=%ld boot_us=%lld",
                 (long)id, (long long)esp_timer_get_time());
    }
}

static esp_err_t check_radio(const tx_config_t *c, wifi_config_t *ap)
{
    uint8_t primary = 0;
    wifi_second_chan_t secondary = WIFI_SECOND_CHAN_NONE;
    wifi_bandwidth_t bandwidth = WIFI_BW_HT20;
    wifi_mode_t mode = WIFI_MODE_NULL;
    wifi_sta_list_t stations = {0};
    const int64_t sampled_us = esp_timer_get_time();
    /* Read all diagnostic fields before propagating an error. Never print the
       AP structure: it contains credentials. -1 explicitly means unavailable. */
    esp_err_t channel_rc = esp_wifi_get_channel(&primary, &secondary);
    esp_err_t bandwidth_rc = esp_wifi_get_bandwidth(WIFI_IF_AP, &bandwidth);
    esp_err_t mode_rc = esp_wifi_get_mode(&mode);
    esp_err_t config_rc = esp_wifi_get_config(WIFI_IF_AP, ap);
    esp_err_t stations_rc = esp_wifi_ap_get_sta_list(&stations);
    ESP_LOGI("tx_boot", "TX_RADIO check boot_us=%lld since_start_us=%lld "
             "expected_primary=%u expected_secondary=%u expected_bw_mhz=%u expected_mode=%d unavailable=-1",
             (long long)sampled_us, (long long)(sampled_us - s_wifi_start_us),
             c->channel, c->secondary_channel, c->bandwidth_mhz, WIFI_MODE_AP);
    ESP_LOGI("tx_boot", "TX_RADIO op=esp_wifi_get_channel rc=0x%x primary=%d secondary=%d",
             (unsigned)channel_rc, channel_rc == ESP_OK ? primary : -1,
             channel_rc == ESP_OK ? (int)secondary : -1);
    ESP_LOGI("tx_boot", "TX_RADIO op=esp_wifi_get_bandwidth rc=0x%x interface_bw=%d",
             (unsigned)bandwidth_rc, bandwidth_rc == ESP_OK ? (int)bandwidth : -1);
    ESP_LOGI("tx_boot", "TX_RADIO op=esp_wifi_get_mode rc=0x%x mode=%d",
             (unsigned)mode_rc, mode_rc == ESP_OK ? (int)mode : -1);
    ESP_LOGI("tx_boot", "TX_RADIO op=esp_wifi_get_config rc=0x%x configured_primary=%d max_clients=%d",
             (unsigned)config_rc, config_rc == ESP_OK ? ap->ap.channel : -1,
             config_rc == ESP_OK ? ap->ap.max_connection : -1);
    if (config_rc == ESP_OK) {
        size_t length = ap->ap.ssid_len ? ap->ap.ssid_len : strnlen((const char *)ap->ap.ssid, sizeof(ap->ap.ssid));
        bool ssid_valid = ap->ap.ssid_len <= sizeof(ap->ap.ssid)
            && (ap->ap.ssid_len != 0 || length < sizeof(ap->ap.ssid));
        ESP_LOGI("tx_boot", "TX_RADIO op=esp_wifi_get_config rc=0x0 authmode=%d beacon_interval_TU=%u ssid_raw_length=%u ssid_effective_length=%u ssid_matches_redacted=%d password_matches_redacted=%d",
                 (int)ap->ap.authmode, ap->ap.beacon_interval, ap->ap.ssid_len, (unsigned)length,
                 ssid_valid && length == strlen(c->wifi_ssid) && memcmp(ap->ap.ssid, c->wifi_ssid, length) == 0,
                 memcmp(ap->ap.password, c->wifi_password, strlen(c->wifi_password) + 1) == 0);
    }
    ESP_LOGI("tx_boot", "TX_RADIO op=esp_wifi_ap_get_sta_list rc=0x%x clients=%d sample_end_us=%lld",
             (unsigned)stations_rc, stations_rc == ESP_OK ? stations.num : -1,
             (long long)esp_timer_get_time());
#define RADIO_RESULT(operation, result) do { \
    if ((result) != ESP_OK) { \
        ESP_LOGE("tx_boot", "TX_RADIO op=%s rc=0x%x field=getter_result expected=ESP_OK actual=unavailable boot_us=%lld since_start_us=%lld", \
                 operation, (unsigned)(result), (long long)sampled_us, (long long)(sampled_us - s_wifi_start_us)); \
        TX_ERROR(operation, result); return result; \
    } \
} while (0)
    RADIO_RESULT("esp_wifi_get_channel", channel_rc);
    RADIO_RESULT("esp_wifi_get_bandwidth", bandwidth_rc);
    RADIO_RESULT("esp_wifi_get_mode", mode_rc);
    RADIO_RESULT("esp_wifi_get_config", config_rc);
    RADIO_RESULT("esp_wifi_ap_get_sta_list", stations_rc);
#undef RADIO_RESULT
    CHECK_FIELD("MODE", "esp_wifi_get_mode", "mode", WIFI_MODE_AP, mode, mode == WIFI_MODE_AP);
    CHECK_FIELD("CONFIG_CHANNEL", "esp_wifi_get_config", "ap.channel", c->channel, ap->ap.channel,
                ap->ap.channel == c->channel);
    wifi_bandwidth_t expected_bw = c->bandwidth_mhz == 20 ? WIFI_BW_HT20 : WIFI_BW_HT40;
    CHECK_FIELD("BANDWIDTH", "esp_wifi_get_bandwidth", "interface_bandwidth_enum", expected_bw, bandwidth,
                bandwidth == expected_bw);
    CHECK_FIELD("PRIMARY_CHANNEL", "esp_wifi_get_channel", "primary", c->channel, primary, primary == c->channel);
    /* IDF 5.4 wifi.rst: AP actual bandwidth is negotiated with its clients.
       esp_wifi_types_generic.h defines NONE as HT20, not a configured direction.
       Keep HT40 configured and reject the wrong ACTIVE HT40 direction. NONE
       proves neither that the requested direction was retained nor HT40 RX. */
    CHECK_FIELD("SECONDARY_CHANNEL", "esp_wifi_get_channel", "secondary", c->secondary_channel, secondary,
        secondary == c->secondary_channel
        || (c->bandwidth_mhz == 40 && secondary == WIFI_SECOND_CHAN_NONE));
    ESP_LOGI("tx_boot", "TX_RADIO secondary_state=%s",
             c->bandwidth_mhz == 40 && secondary == WIFI_SECOND_CHAN_NONE
                 ? "HT20_OBSERVED_HT40_UNPROVEN" : "PROFILE_MATCH");
    return ESP_OK;
}

static esp_err_t configure_dhcp(void)
{
    esp_err_t err = esp_netif_dhcps_stop(s_ap);
    if (err != ESP_OK && err != ESP_ERR_ESP_NETIF_DHCP_ALREADY_STOPPED) {
        TX_ERROR("DHCP_STOP", err);
        return err;
    }
    esp_netif_ip_info_t ip = {
        .ip.addr = inet_addr("192.168.4.1"), .gw.addr = inet_addr("192.168.4.1"),
        .netmask.addr = inet_addr("255.255.255.0"),
    };
    TX_TRY(esp_netif_set_ip_info(s_ap, &ip));
    dhcps_lease_t lease = {.enable = true};
    lease.start_ip.addr = inet_addr(TX_DHCP_FIRST);
    lease.end_ip.addr = inet_addr(TX_DHCP_LAST);
    TX_TRY(esp_netif_dhcps_option(s_ap, ESP_NETIF_OP_SET,
                              ESP_NETIF_REQUESTED_IP_ADDRESS, &lease, sizeof(lease)));
    TX_TRY(esp_netif_dhcps_start(s_ap));
    return ESP_OK;
}

esp_err_t tx_network_check(const tx_config_t *c)
{
    if (s_ap == NULL) {
        ESP_LOGE("tx_boot", "TX_COMPARE op=tx_network_check rc=0x%x field=ap_netif expected=present actual=unavailable boot_us=%lld phase=before_getters",
                 (unsigned)ESP_ERR_INVALID_STATE, (long long)esp_timer_get_time());
        TX_ERROR("NETIF_MISSING", ESP_ERR_INVALID_STATE);
        return ESP_ERR_INVALID_STATE;
    }
    const int64_t sampled_us = esp_timer_get_time();
    uint8_t mac[6] = {0}, protocol = 0;
    int8_t power = 0;
    wifi_config_t ap = {0};
    wifi_country_t country = {0};
    esp_netif_ip_info_t ip = {0};
    dhcps_lease_t lease = {0};
    esp_netif_dhcp_status_t dhcp = 0;
    /* Read-only diagnostics complete before returning the first error. No
       setter, sender or unsafe continuation occurs after a failed check. */
    esp_err_t radio_rc = check_radio(c, &ap);
    esp_err_t mac_rc = esp_wifi_get_mac(WIFI_IF_AP, mac);
    esp_err_t protocol_rc = esp_wifi_get_protocol(WIFI_IF_AP, &protocol);
    esp_err_t power_rc = esp_wifi_get_max_tx_power(&power);
    esp_err_t country_rc = esp_wifi_get_country(&country);
    esp_err_t ip_rc = esp_netif_get_ip_info(s_ap, &ip);
    esp_err_t dhcp_rc = esp_netif_dhcps_get_status(s_ap, &dhcp);
    esp_err_t lease_rc = esp_netif_dhcps_option(s_ap, ESP_NETIF_OP_GET,
                              ESP_NETIF_REQUESTED_IP_ADDRESS, &lease, sizeof(lease));
    ESP_LOGI("tx_boot", "TX_NET sample_begin_us=%lld sample_end_us=%lld since_start_us=%lld unavailable=-1",
             (long long)sampled_us, (long long)esp_timer_get_time(), (long long)(sampled_us - s_wifi_start_us));
    if (mac_rc == ESP_OK) {
        for (unsigned i = 0; i < 6; ++i)
            ESP_LOGI("tx_boot", "TX_NET op=esp_wifi_get_mac rc=0x0 field=ap_mac[%u] expected=0x%02x actual=0x%02x",
                     i, c->ap_mac[i], mac[i]);
    }
    ESP_LOGI("tx_boot", "TX_NET op=esp_wifi_get_protocol rc=0x%x expected_mask=%u actual_mask=%d",
             (unsigned)protocol_rc, c->protocol_mask, protocol_rc == ESP_OK ? protocol : -1);
    ESP_LOGI("tx_boot", "TX_NET op=esp_wifi_get_max_tx_power rc=0x%x unit=quarter_dBm expected=%d actual=%d",
             (unsigned)power_rc, TX_POWER_QUARTER_DBM, power_rc == ESP_OK ? power : -1);
    if (country_rc == ESP_OK) {
        for (unsigned i = 0; i < 3; ++i)
            ESP_LOGI("tx_boot", "TX_NET op=esp_wifi_get_country rc=0x0 field=country.cc[%u] expected_raw=0x%02x actual_raw=0x%02x",
                     i, (unsigned char)c->country[i], (unsigned char)country.cc[i]);
        ESP_LOGI("tx_boot", "TX_NET country expected_env=0x%02x actual_env=0x%02x schan=%u nchan=%u max_tx_power_dBm=%d policy=%d",
                 country_environment(c->country[2]), country_environment(country.cc[2]),
                 country.schan, country.nchan, country.max_tx_power, (int)country.policy);
    } else {
        for (unsigned i = 0; i < 3; ++i)
            ESP_LOGI("tx_boot", "TX_NET op=esp_wifi_get_country rc=0x%x field=country.cc[%u] expected_raw=0x%02x actual_raw=unavailable",
                     (unsigned)country_rc, i, (unsigned char)c->country[i]);
    }
    if (ip_rc == ESP_OK)
        ESP_LOGI("tx_boot", "TX_NET op=esp_netif_get_ip_info rc=0x0 ip=0x%08lx gw=0x%08lx netmask=0x%08lx encoding=network_order",
                 (unsigned long)ip.ip.addr, (unsigned long)ip.gw.addr, (unsigned long)ip.netmask.addr);
    ESP_LOGI("tx_boot", "TX_NET op=esp_netif_dhcps_get_status rc=0x%x expected=%d actual=%d",
             (unsigned)dhcp_rc, ESP_NETIF_DHCP_STARTED, dhcp_rc == ESP_OK ? (int)dhcp : -1);
    if (lease_rc == ESP_OK)
        ESP_LOGI("tx_boot", "TX_NET op=esp_netif_dhcps_option(GET) rc=0x0 enable=%d start=0x%08lx end=0x%08lx encoding=network_order",
                 lease.enable, (unsigned long)lease.start_ip.addr, (unsigned long)lease.end_ip.addr);
#define READ_RESULT(operation, result) do { \
    if ((result) != ESP_OK) \
        ESP_LOGE("tx_boot", "TX_NET op=%s rc=0x%x field=getter_result expected=ESP_OK actual=unavailable boot_us=%lld since_start_us=%lld", \
                 operation, (unsigned)(result), (long long)sampled_us, (long long)(sampled_us - s_wifi_start_us)); \
} while (0)
    READ_RESULT("esp_wifi_get_mac", mac_rc);
    READ_RESULT("esp_wifi_get_protocol", protocol_rc);
    READ_RESULT("esp_wifi_get_max_tx_power", power_rc);
    READ_RESULT("esp_wifi_get_country", country_rc);
    READ_RESULT("esp_netif_get_ip_info", ip_rc);
    READ_RESULT("esp_netif_dhcps_get_status", dhcp_rc);
    READ_RESULT("esp_netif_dhcps_option(GET)", lease_rc);
#undef READ_RESULT
    if (radio_rc != ESP_OK) return radio_rc;
#define RETURN_ERROR(operation, result) do { \
    if ((result) != ESP_OK) { TX_ERROR(operation, result); return result; } \
} while (0)
    RETURN_ERROR("esp_wifi_get_mac", mac_rc);
    RETURN_ERROR("esp_wifi_get_protocol", protocol_rc);
    RETURN_ERROR("esp_wifi_get_max_tx_power", power_rc);
    RETURN_ERROR("esp_wifi_get_country", country_rc);
    RETURN_ERROR("esp_netif_get_ip_info", ip_rc);
    RETURN_ERROR("esp_netif_dhcps_get_status", dhcp_rc);
    RETURN_ERROR("esp_netif_dhcps_option(GET)", lease_rc);
#undef RETURN_ERROR
    static const char *mac_fields[] = {"ap_mac[0]", "ap_mac[1]", "ap_mac[2]", "ap_mac[3]", "ap_mac[4]", "ap_mac[5]"};
    for (unsigned i = 0; i < 6; ++i)
        CHECK_FIELD("AP_MAC", "esp_wifi_get_mac", mac_fields[i], c->ap_mac[i], mac[i], mac[i] == c->ap_mac[i]);
    CHECK_FIELD("PROTOCOL", "esp_wifi_get_protocol", "protocol_mask", c->protocol_mask, protocol, protocol == c->protocol_mask);
    CHECK_FIELD("TX_POWER", "esp_wifi_get_max_tx_power", "power_quarter_dBm", TX_POWER_QUARTER_DBM, power, power == TX_POWER_QUARTER_DBM);
    CHECK_FIELD("COUNTRY", "esp_wifi_get_country", "country.cc[0]", (unsigned char)c->country[0], (unsigned char)country.cc[0], country.cc[0] == c->country[0]);
    CHECK_FIELD("COUNTRY", "esp_wifi_get_country", "country.cc[1]", (unsigned char)c->country[1], (unsigned char)country.cc[1], country.cc[1] == c->country[1]);
    CHECK_FIELD("COUNTRY", "esp_wifi_get_country", "country.environment", country_environment(c->country[2]), country_environment(country.cc[2]),
                country_environment(country.cc[2]) == country_environment(c->country[2]));
    CHECK_FIELD("COUNTRY", "esp_wifi_get_country", "country.schan", 1, country.schan, country.schan == 1);
    CHECK_FIELD("COUNTRY", "esp_wifi_get_country", "country.nchan", c->country_channels, country.nchan, country.nchan == c->country_channels);
    CHECK_FIELD("COUNTRY", "esp_wifi_get_country", "country.policy", WIFI_COUNTRY_POLICY_MANUAL, country.policy, country.policy == WIFI_COUNTRY_POLICY_MANUAL);
    /* Only the bound DE/1..13/MANUAL contract was verified against the pinned
       library. Other countries have different table values; fail closed. */
    CHECK_FIELD("COUNTRY", "pinned_libnet80211", "country.power_contract_DE_1_13", 1, 0,
                c->country[0] == 'D' && c->country[1] == 'E' && c->country_channels == 13);
    CHECK_FIELD("COUNTRY", "esp_wifi_get_country", "country.max_tx_power_dBm", TX_DE_COUNTRY_TABLE_DBM, country.max_tx_power,
                country.max_tx_power == TX_DE_COUNTRY_TABLE_DBM);
    /* ssid_len=0 requires a terminator inside the 32-byte SDK array. With an
       explicit length, no terminator or equality of unused padding is required. */
    size_t ssid_len = ap.ap.ssid_len ? ap.ap.ssid_len : strnlen((const char *)ap.ap.ssid, sizeof(ap.ap.ssid));
    CHECK_FIELD("AP_CONFIG", "esp_wifi_get_config", "ap.ssid_representation_valid", 1, 0,
                ap.ap.ssid_len <= sizeof(ap.ap.ssid) && (ap.ap.ssid_len != 0 || ssid_len < sizeof(ap.ap.ssid)));
    CHECK_FIELD("AP_CONFIG", "esp_wifi_get_config", "ap.ssid_length", strlen(c->wifi_ssid), ssid_len, ssid_len == strlen(c->wifi_ssid));
    CHECK_FIELD("AP_CONFIG", "esp_wifi_get_config", "ap.ssid_matches_redacted", 1, 0, memcmp(ap.ap.ssid, c->wifi_ssid, ssid_len) == 0);
    CHECK_FIELD("AP_CONFIG", "esp_wifi_get_config", "ap.password_matches_redacted", 1, 0,
                memcmp(ap.ap.password, c->wifi_password, strlen(c->wifi_password) + 1) == 0);
    CHECK_FIELD("AP_CONFIG", "esp_wifi_get_config", "ap.authmode", WIFI_AUTH_WPA2_PSK, ap.ap.authmode, ap.ap.authmode == WIFI_AUTH_WPA2_PSK);
    CHECK_FIELD("AP_CONFIG", "esp_wifi_get_config", "ap.max_connection", 8, ap.ap.max_connection, ap.ap.max_connection == 8);
    CHECK_FIELD("AP_CONFIG", "esp_wifi_get_config", "ap.beacon_interval_TU", c->beacon_interval, ap.ap.beacon_interval, ap.ap.beacon_interval == c->beacon_interval);
    CHECK_FIELD("AP_IP", "esp_netif_get_ip_info", "ip.network_order", inet_addr("192.168.4.1"), ip.ip.addr, ip.ip.addr == inet_addr("192.168.4.1"));
    CHECK_FIELD("AP_IP", "esp_netif_get_ip_info", "gw.network_order", ip.ip.addr, ip.gw.addr, ip.gw.addr == ip.ip.addr);
    CHECK_FIELD("AP_IP", "esp_netif_get_ip_info", "netmask.network_order", inet_addr("255.255.255.0"), ip.netmask.addr, ip.netmask.addr == inet_addr("255.255.255.0"));
    CHECK_FIELD("DHCP_STATE", "esp_netif_dhcps_get_status", "dhcp.status", ESP_NETIF_DHCP_STARTED, dhcp, dhcp == ESP_NETIF_DHCP_STARTED);
    CHECK_FIELD("DHCP_STATE", "esp_netif_dhcps_option(GET)", "lease.enable", 1, lease.enable, lease.enable);
    CHECK_FIELD("DHCP_POOL", "esp_netif_dhcps_option(GET)", "lease.start.network_order", inet_addr(TX_DHCP_FIRST), lease.start_ip.addr, lease.start_ip.addr == inet_addr(TX_DHCP_FIRST));
    CHECK_FIELD("DHCP_POOL", "esp_netif_dhcps_option(GET)", "lease.end.network_order", inet_addr(TX_DHCP_LAST), lease.end_ip.addr, lease.end_ip.addr == inet_addr(TX_DHCP_LAST));
    return ESP_OK;
}

static esp_err_t start_radio(const tx_config_t *c)
{
    /* Install the restricted pool before AP_START can offer any lease. */
    TX_STEP(configure_dhcp());
    TX_CHECKPOINT("DHCP_CONFIG_OK");
    s_wifi_start_us = esp_timer_get_time();
    ESP_LOGI("tx_boot", "TX_RADIO op=esp_wifi_start begin_us=%lld", (long long)s_wifi_start_us);
    TX_STEP(esp_wifi_start());
    TX_CHECKPOINT("WIFI_START_RETURNED");
    /* IDF forbids set_channel with associated STAs. This snapshot catches an
       already connected client; it is not an atomic exclusion of association. */
    wifi_sta_list_t stations = {0};
    esp_err_t clients_rc = esp_wifi_ap_get_sta_list(&stations);
    ESP_LOGI("tx_boot", "TX_RADIO before_set_channel op=esp_wifi_ap_get_sta_list rc=0x%x clients=%d since_start_us=%lld",
             (unsigned)clients_rc, clients_rc == ESP_OK ? stations.num : -1,
             (long long)(esp_timer_get_time() - s_wifi_start_us));
    if (clients_rc != ESP_OK || stations.num != 0) {
        wifi_config_t observed = {0};
        (void)check_radio(c, &observed);
        if (clients_rc != ESP_OK) {
            TX_ERROR("esp_wifi_ap_get_sta_list", clients_rc);
            return clients_rc;
        }
        ESP_LOGE("tx_boot", "TX_COMPARE op=esp_wifi_ap_get_sta_list rc=0x%x field=clients_before_set_channel expected=0 actual=%d boot_us=%lld since_start_us=%lld",
                 (unsigned)ESP_ERR_INVALID_STATE, stations.num, (long long)esp_timer_get_time(),
                 (long long)(esp_timer_get_time() - s_wifi_start_us));
        TX_ERROR("SET_CHANNEL_CLIENTS_PRESENT", ESP_ERR_INVALID_STATE);
        return ESP_ERR_INVALID_STATE;
    }
    esp_err_t channel_rc = esp_wifi_set_channel(c->channel, (wifi_second_chan_t)c->secondary_channel);
    ESP_LOGI("tx_boot", "TX_RADIO op=esp_wifi_set_channel rc=0x%x requested_primary=%u requested_secondary=%u since_start_us=%lld",
             (unsigned)channel_rc, c->channel, c->secondary_channel,
             (long long)(esp_timer_get_time() - s_wifi_start_us));
    if (channel_rc != ESP_OK) {
        /* Capture actual values even when the setter fails, without masking it. */
        wifi_config_t observed = {0};
        (void)check_radio(c, &observed);
        TX_ERROR("esp_wifi_set_channel", channel_rc);
        return channel_rc;
    }
    TX_STEP(esp_wifi_set_max_tx_power(TX_POWER_QUARTER_DBM));
    TX_TRY(trace_power_state("after_power_set", true));
    /* AP_START and netif start are asynchronous; wait at most three seconds. */
    for (unsigned attempt = 0; attempt < 30; ++attempt) {
        esp_netif_dhcp_status_t status;
        TX_TRY(esp_netif_dhcps_get_status(s_ap, &status));
        if (status == ESP_NETIF_DHCP_STARTED) {
            /* The default AP_START handler has brought up the netif. */
            TX_CHECKPOINT("AP_STARTED");
            TX_TRY(tx_network_check(c));
            TX_CHECKPOINT("DHCP_OK");
            return ESP_OK;
        }
        vTaskDelay(pdMS_TO_TICKS(100));
    }
    ESP_LOGE("tx_boot", "TX_COMPARE op=esp_netif_dhcps_get_status rc=0x%x field=dhcp_ready expected=%d actual=not_started budget_ms=3000 boot_us=%lld since_start_us=%lld",
             (unsigned)ESP_ERR_TIMEOUT, ESP_NETIF_DHCP_STARTED, (long long)esp_timer_get_time(),
             (long long)(esp_timer_get_time() - s_wifi_start_us));
    TX_ERROR("AP_START_TIMEOUT", ESP_ERR_TIMEOUT);
    return ESP_ERR_TIMEOUT;
}

void tx_network_stop(void)
{
    if (s_ap != NULL) (void)esp_netif_dhcps_stop(s_ap);
    (void)esp_wifi_stop();
}

esp_err_t tx_network_restart(const tx_config_t *c)
{
    tx_network_stop();
    esp_err_t err = start_radio(c);
    if (err != ESP_OK) tx_network_stop();
    return err;
}

esp_err_t tx_network_start(const tx_config_t *c)
{
    if (!tx_config_validate(c)) {
        TX_ERROR("NETWORK_PROFILE", ESP_ERR_INVALID_ARG);
        return ESP_ERR_INVALID_ARG;
    }
    const int64_t sampled_us = esp_timer_get_time();
    uint8_t base_mac[6];
    TX_STEP(esp_efuse_mac_get_default(base_mac));
    for (unsigned i = 0; i < 6; ++i) {
        if (base_mac[i] != c->base_mac[i]) {
            ESP_LOGE("tx_boot", "TX_COMPARE op=esp_efuse_mac_get_default rc=0x%x field=base_mac[%u] expected=0x%02x actual=0x%02x boot_us=%lld phase=before_wifi",
                     (unsigned)ESP_ERR_INVALID_STATE, i, c->base_mac[i], base_mac[i], (long long)sampled_us);
            TX_ERROR("BASE_MAC_MISMATCH", ESP_ERR_INVALID_STATE);
            return ESP_ERR_INVALID_STATE;
        }
    }
    TX_STEP(esp_netif_init());
    TX_STEP(esp_event_loop_create_default());
    /* Same default AP setup, with returned errors instead of hidden asserts/abort. */
    esp_netif_config_t netif_config = ESP_NETIF_DEFAULT_WIFI_AP();
    TX_CHECKPOINT("BEGIN esp_netif_new");
    s_ap = esp_netif_new(&netif_config);
    if (s_ap == NULL) {
        ESP_LOGE("tx_boot", "TX_COMPARE op=esp_netif_new rc=0x%x field=allocated_ap_netif expected=present actual=unavailable boot_us=%lld phase=before_attach",
                 (unsigned)ESP_ERR_NO_MEM, (long long)esp_timer_get_time());
        TX_ERROR("NETIF_ALLOC", ESP_ERR_NO_MEM);
        return ESP_ERR_NO_MEM;
    }
    TX_STEP(esp_netif_attach_wifi_ap(s_ap));
    TX_STEP(esp_wifi_set_default_wifi_ap_handlers());
    TX_STEP(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID, radio_event, NULL));
    TX_CHECKPOINT("NETIF_OK");
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    TX_STEP(esp_wifi_init(&init));
    TX_CHECKPOINT("WIFI_INIT_OK");
    TX_STEP(esp_wifi_set_storage(WIFI_STORAGE_RAM));
    TX_STEP(esp_wifi_set_mode(WIFI_MODE_AP));
    TX_STEP(esp_wifi_set_mac(WIFI_IF_AP, c->ap_mac));
    wifi_country_t country = {.schan = 1, .nchan = c->country_channels,
                             .max_tx_power = 11, .policy = WIFI_COUNTRY_POLICY_MANUAL};
    memcpy(country.cc, c->country, 3);
    TX_STEP(esp_wifi_set_country(&country));
    TX_TRY(trace_power_state("after_country_set", false));
    wifi_config_t ap = {0};
    memcpy(ap.ap.ssid, c->wifi_ssid, strlen(c->wifi_ssid));
    memcpy(ap.ap.password, c->wifi_password, strlen(c->wifi_password) + 1);
    ap.ap.ssid_len = strlen(c->wifi_ssid);
    ap.ap.channel = c->channel;
    ap.ap.max_connection = 8;
    ap.ap.authmode = WIFI_AUTH_WPA2_PSK;
    ap.ap.beacon_interval = c->beacon_interval;
    TX_STEP(esp_wifi_set_config(WIFI_IF_AP, &ap));
    TX_STEP(esp_wifi_set_protocol(WIFI_IF_AP, c->protocol_mask));
    TX_STEP(esp_wifi_set_bandwidth(WIFI_IF_AP, c->bandwidth_mhz == 20 ? WIFI_BW_HT20 : WIFI_BW_HT40));
    TX_CHECKPOINT("AP_CONFIG_OK");
    esp_err_t err = start_radio(c);
    if (err != ESP_OK) tx_network_stop();
    return err;
}
