# Shared by the opt-in legacy main component and the trimmed release component.
set(RX_SOURCES
    rx_main.c rx_wifi.c rx_profile.c rx_budget.c rx_settings.c rx_settings_codec.c
    rx_control.c csi_peer.c csi_collector.c csi_transport.c stream_sender.c
    nvs_config.c node_identity.c rx_diag.c diag_http.c status_led.c rx_ota.c rx_sync_disabled.c
)
set(RX_REQUIRES esp_wifi esp_netif esp_event nvs_flash app_update esp_http_server
    esp_app_format esp_timer esp_driver_gpio esp_hw_support lwip mbedtls json espressif__led_strip)
