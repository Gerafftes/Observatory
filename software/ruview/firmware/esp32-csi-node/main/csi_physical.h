#ifndef CSI_PHYSICAL_H
#define CSI_PHYSICAL_H
#include "csi_collector.h"
#include "rx_profile.h"
#include <string.h>

/* Existing S3/IDF-5.4 physical contract (Gen02 A063 v1). No base/identity
 * reserved byte is repurposed. The exact driver configuration is attested. */
static inline void csi_physical_encode(const wifi_csi_info_t *info,
                                     const wifi_csi_config_t *config, uint8_t out[32])
{
    memset(out, 0, 32);
    uint32_t magic=0xc511a063u, profile=0x05040009u;
    uint16_t size=32, iq_bytes=(uint16_t)info->len;
    memcpy(out,&magic,4); out[4]=1; out[5]=9; memcpy(out+6,&size,2);
    out[8]=info->rx_ctrl.sig_mode; out[9]=info->rx_ctrl.cwb;
    out[10]=info->rx_ctrl.stbc; out[11]=info->rx_ctrl.secondary_channel;
    out[12]=(config->lltf_en?1:0)|(config->htltf_en?2:0)|(config->stbc_htltf2_en?4:0)
        |(config->ltf_merge_en?8:0)|(config->channel_filter_en?16:0)|(config->manu_scale?32:0);
    out[13]=config->shift; out[14]=1; out[15]=info->first_word_invalid;
    memcpy(out+16,&iq_bytes,2); out[18]=info->rx_ctrl.rx_state; memcpy(out+20,&profile,4);
    uint32_t crc=rx_crc32(out,28); memcpy(out+28,&crc,4);
}
#endif
