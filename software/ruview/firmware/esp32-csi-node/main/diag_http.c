#include "status_led.h"
#include "ota_update.h"
#include "diag_http.h"
#include <inttypes.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <string.h>
#include "esp_ota_ops.h"
#include "esp_app_desc.h"
#include "esp_timer.h"
#include "esp_heap_caps.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/idf_additions.h"
#include "csi_transport.h"
#include "csi_peer.h"
#include "rx_settings.h"
#include "wifi_reconnect.h"
#include "esp_system.h"
#ifndef DIAG_SOURCE_SHA
#define DIAG_SOURCE_SHA "unmanifested"
#endif
static char image_hex[65];
static bool image_ready;
static int64_t image_begin_us, image_end_us;
static esp_err_t chunk(httpd_req_t *req, const char *format, ...) {
    char buffer[512]; va_list ap; va_start(ap, format);
    int n = vsnprintf(buffer, sizeof(buffer), format, ap); va_end(ap);
    return n < 0 || (size_t)n >= sizeof(buffer) ? ESP_FAIL : httpd_resp_send_chunk(req, buffer, n);
}
bool diag_http_requested(httpd_req_t *req) {
    char query[96], mode[8];
    return httpd_req_get_url_query_str(req, query, sizeof(query)) == ESP_OK &&
        httpd_query_key_value(query, "diag", mode, sizeof(mode)) == ESP_OK && !strcmp(mode, "1");
}
static bool cursor(httpd_req_t *req, uint64_t *after) {
    char query[96], value[24]; *after = 0;
    if (httpd_req_get_url_query_len(req) == 0) return true;
    if (httpd_req_get_url_query_str(req, query, sizeof(query)) != ESP_OK) return false;
    esp_err_t parsed = httpd_query_key_value(query, "after", value, sizeof(value));
    if (parsed == ESP_ERR_NOT_FOUND) return true;
    if (parsed != ESP_OK) return false;
    if (!value[0]) return false;
    for (size_t i=0; value[i]; ++i) if (value[i]<'0' || value[i]>'9') return false;
    errno=0; char *end; *after=strtoull(value,&end,10); return !errno && !*end;
}
static esp_err_t begin(httpd_req_t *req, uint64_t boot, const char *role) {
    if (!image_ready) {
        const esp_partition_t *running=esp_ota_get_running_partition(); uint8_t sha[32];
        image_begin_us=esp_timer_get_time();
        if (!running || esp_partition_get_sha256(running,sha)!=ESP_OK) {
            httpd_resp_send_err(req,HTTPD_500_INTERNAL_SERVER_ERROR,"image binding unavailable");
            return ESP_FAIL;
        }
        image_end_us=esp_timer_get_time();
        for (unsigned i=0;i<32;++i) snprintf(image_hex+2*i,3,"%02x",sha[i]);
        image_ready=true;
    }
    httpd_resp_set_type(req,"application/json");
    return chunk(req,"{\"schema\":\"observatory.rx-diagnostic.v2\",\"role\":\"%s\",\"boot_id\":%" PRIu64 ",\"image_sha256\":\"%s\",\"source_sha256\":\"%s\",\"image_begin_us\":%lld,\"image_end_us\":%lld,\"snapshot_begin_us\":%lld,",
        role,boot,image_hex,DIAG_SOURCE_SHA,(long long)image_begin_us,(long long)image_end_us,(long long)esp_timer_get_time());
}
#define EMIT(...) do { if (chunk(req,__VA_ARGS__) != ESP_OK) return ESP_FAIL; } while(0)
#include "rx_diag.h"
#include "csi_collector.h"
esp_err_t diag_http_send(httpd_req_t *req) {
    RX_HTTP_SCOPE(http_scope, RX_HTTP_DIAG);
    uint64_t after; if (!cursor(req,&after) || after) return httpd_resp_send_err(req,HTTPD_400_BAD_REQUEST,"invalid cursor");
    if (begin(req,csi_collector_get_boot_id(),"RX-unified") != ESP_OK) return ESP_FAIL;
    uint8_t source_digest[32];char source_hex[65]={0};
    bool source_valid=rx_mac_valid(rx_settings_active_profile()->tx_mac);
    memcpy(source_digest,csi_peer_source_hash(0),sizeof(source_digest));
    if(source_valid) for(unsigned i=0;i<32;i++) snprintf(source_hex+2*i,3,"%02x",source_digest[i]);
    EMIT("\"tx_filter_identity_valid\":%s,\"tx_filter_sha256\":\"%s\",",source_valid?"true":"false",source_hex);
    rx_diag_snapshot_t s; rx_diag_snapshot(&s);
    EMIT("\"allocation_first_failure\":{\"state\":%u,\"record\":", s.first_failure_state);
    if (s.first_failure_state == RX_FAILURE_COMPLETE) {
        EMIT("{\"time_us\":%lld,\"requested_bytes\":%llu,\"caps\":%lu,\"api_id\":%llu,\"core_id\":%lu,\"http_phase_counts\":%lu,\"task_id\":null,\"isr\":null}",
            (long long)s.first_failure.time_us, (unsigned long long)s.first_failure.requested_bytes,
            (unsigned long)s.first_failure.caps, (unsigned long long)s.first_failure.api_id,
            (unsigned long)s.first_failure.core_id, (unsigned long)s.first_failure.http_phases);
    } else EMIT("null");
    EMIT("},");
    EMIT("\"saturated\":%s,\"classes\":[",s.saturated?"true":"false");
    for (unsigned f=0;f<RX_CLASSES;++f) {
        EMIT("%s{\"format\":%u,\"has_sequence\":%s,\"last_sequence\":%" PRIu32 ",\"stages\":[",f?",":"",f,s.has_sequence[f]?"true":"false",s.last_sequence[f]);
        for (unsigned k=0;k<RX_STAGES;++k) EMIT("%s%" PRIu32,k?",":"",s.count[f][k]);
        EMIT("],\"features\":[");
        for (unsigned k=0;k<8;++k) EMIT("%s%" PRIu32,k?",":"",s.features[f][k]);
        EMIT("]}");
    }
    EMIT("],\"grid_stages\":[");
    for (unsigned k=0;k<RX_STAGES;++k) EMIT("%s%" PRIu32,k?",":"",s.grid_stages[k]);
    csi_transport_stats_t transport;csi_transport_get_stats(&transport);
    csi_collector_stats_t collector; csi_collector_get_stats(&collector);
    EMIT("],\"allocation_failures\":%lu,\"allocation_counter_start_us\":%lld,\"resources\":",(unsigned long)s.allocation_failures,(long long)s.allocation_counter_start_us);
    EMIT("{\"schema\":\"observatory.rx-resources.v1\",\"unit\":\"bytes\",\"sample_us\":%lld,\"heap_internal_free_bytes\":%u,\"heap_internal_min_bytes\":%u,\"heap_internal_largest_bytes\":%u,",
         (long long)esp_timer_get_time(),(unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT),(unsigned)heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT),(unsigned)heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL|MALLOC_CAP_8BIT));
    EMIT("\"http_stack_min_bytes\":%u,\"callback_stack_min_bytes\":%u,\"sender_stack_min_bytes\":%lu,\"sender_task_exists\":%s,\"queue_depth\":%lu,\"queue_capacity\":%lu,\"queue_max_sampled_depth\":%lu,\"timing_errors\":%lu,",
         (unsigned)uxTaskGetStackHighWaterMark(NULL),(unsigned)collector.callback_stack_free_min_bytes,(unsigned long)transport.processing_stack_free_min_bytes,transport.processing_stack_runtime_measured?"true":"false",(unsigned long)transport.queue_depth,(unsigned long)CSI_TRANSPORT_CAPTURE_SLOTS,(unsigned long)transport.queue_high_watermark,(unsigned long)transport.ownership_errors);
    csi_peer_stats_t peer; csi_peer_get_stats(&peer);
    EMIT("\"peer_profile\":%s,\"peer_task_running\":%s,\"peer_stack_min_bytes\":%lu,",
        peer.enabled?"true":"false",peer.task_running?"true":"false",(unsigned long)peer.task_stack_min_bytes);
#if defined(CONFIG_FREERTOS_GENERATE_RUN_TIME_STATS) && CONFIG_FREERTOS_GENERATE_RUN_TIME_STATS
    EMIT("\"cpu_idle_task_runtime_us\":[%llu,%llu],\"cpu_runtime_counter_bits\":64,\"cpu_runtime_clock\":\"esp_timer_boot_us\",",
         (unsigned long long)ulTaskGetIdleRunTimeCounterForCore(0),(unsigned long long)ulTaskGetIdleRunTimeCounterForCore(1));
#else
    EMIT("\"cpu_idle_task_runtime_us\":null,\"cpu_runtime_reason\":\"SDK_runtime_stats_disabled\",");
#endif
    status_led_snapshot_t led;status_led_snapshot(&led);
    EMIT("\"allocation_counter_scope\":\"registered_app_entry_until_snapshot\"},\"led\":{\"code\":\"%s\",\"displayed\":\"%s\",\"reason\":\"%s\",\"sample_us\":%lld,\"since_us\":%lld,\"last_csi_us\":%lld,\"qualified_frames\":%lu,\"driver_ok\":%s,\"testing\":%s},\"ota_upload\":",
        status_led_name(led.code),status_led_name(led.displayed),led.reason,
        (long long)led.sample_us,(long long)led.since_us,(long long)led.last_csi_us,
        (unsigned long)led.qualified_frames,led.driver_ok?"true":"false",led.testing?"true":"false");
    char progress[512];int progress_len=ota_update_format_progress(progress,sizeof(progress));
    if(progress_len<0 || (size_t)progress_len>=sizeof(progress) || httpd_resp_send_chunk(req,progress,progress_len)!=ESP_OK) return ESP_FAIL;
    const rx_profile_t *profile = rx_settings_active_profile();
    EMIT(",\"profile\":{\"node_id\":%u,\"generation\":%lu,\"mode\":\"%s\",\"reset_reason\":%u,\"peer_session_ended\":%s,\"raw_send_attempts\":%lu,\"raw_send_errors\":%lu},",
        profile->node_id,(unsigned long)profile->generation,rx_profile_mode_name(profile->mode),
        (unsigned)esp_reset_reason(),peer.session_ended?"true":"false",(unsigned long)peer.raw_send_attempts,(unsigned long)peer.raw_send_errors);
    EMIT("\"callback\":{\"received\":%lu,\"invalid\":%lu,\"foreign\":%lu,\"rate_limited\":%lu,\"admitted\":%lu,\"copied\":%lu,\"handoff_drops\":%lu},",
        (unsigned long)collector.callbacks_received,(unsigned long)collector.invalid_inputs,
        (unsigned long)collector.foreign_tx,(unsigned long)collector.rate_limited,(unsigned long)collector.admitted,
        (unsigned long)collector.capture_handoff_accepted,(unsigned long)collector.capture_handoff_drops);
    EMIT("\"transport\":{\"captured\":%lu,\"waiting\":%lu,\"in_flight\":%lu,\"aborted\":%lu,\"serialize_errors\":%lu,\"os_accepted\":%lu,\"abandoned\":%lu,\"expired\":%lu},",
        (unsigned long)transport.captures_enqueued,(unsigned long)transport.captures_waiting,
        (unsigned long)transport.captures_in_flight,(unsigned long)transport.captures_aborted,
        (unsigned long)transport.serialization_failures,(unsigned long)transport.frames_os_accepted,
        (unsigned long)transport.frames_abandoned,(unsigned long)transport.frames_expired);
    wifi_reconnect_snapshot_t wifi; wifi_reconnect_snapshot(&wifi);
    EMIT("\"reconnect\":{\"connected\":%s,\"in_flight\":%s,\"timer_active\":%s,\"disconnects\":%lu,\"attempts\":%lu,\"got_ip\":%lu,\"last_rc\":%ld,\"arm_rc\":%ld,\"saturated\":%s},\"wifi_events\":[",
        wifi.connected?"true":"false",wifi.connect_in_flight?"true":"false",wifi.timer_active?"true":"false",
        (unsigned long)wifi.disconnect_count,(unsigned long)wifi.connect_count,(unsigned long)wifi.got_ip_count,
        (long)wifi.connect_rc,(long)wifi.arm_rc,wifi.saturated?"true":"false");
    wifi_reconnect_event_t events[WIFI_RECONNECT_EVENT_CAPACITY];
    size_t event_count=wifi_reconnect_events(events,WIFI_RECONNECT_EVENT_CAPACITY);
    for (size_t i=0;i<event_count;++i) {
        EMIT("%s{\"event\":%lu,\"sequence\":%lu,\"boot_us\":%lld,\"result\":%ld}",i?",":"",
            (unsigned long)events[i].event,(unsigned long)events[i].sequence,(long long)events[i].time_us,(long)events[i].result);
    }
    EMIT("],\"links\":[");
    for (unsigned i=0; i<=profile->peer_count; ++i) {
        csi_peer_link_stats_t *link=&peer.links[i];
        EMIT("%s{\"source_node\":%u,\"receiver_node\":%u,\"observed\":%lu,\"admitted\":%lu,\"rejected\":%lu,\"captured\":%lu,\"queued\":%lu,\"queue_drops\":%lu,\"high_watermark\":%lu,\"os_accepted\":%lu,\"abandoned\":%lu,\"serialize_errors\":%lu,\"aborted\":%lu,\"expired\":%lu}",
            i?",":"",i?profile->peers[i-1].node_id:0,profile->node_id,
            (unsigned long)link->observed,(unsigned long)link->admitted,(unsigned long)link->rejected,
            (unsigned long)link->captured,(unsigned long)link->queued,(unsigned long)link->queue_drops,
            (unsigned long)link->high_watermark,(unsigned long)link->delivered,(unsigned long)link->abandoned,
            (unsigned long)link->serialize_errors,(unsigned long)link->aborted,(unsigned long)link->expired);
    }
    EMIT("],\"gain_available\":false,\"complete\":true,\"snapshot_end_us\":%lld}",(long long)esp_timer_get_time());
    return httpd_resp_send_chunk(req,NULL,0);
}
