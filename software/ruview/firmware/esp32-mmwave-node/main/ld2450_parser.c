#include "ld2450_parser.h"

#include <string.h>

static const uint8_t FRAME_HEADER[] = {0xAA, 0xFF, 0x03, 0x00};
static const uint8_t FRAME_FOOTER[] = {0x55, 0xCC};

static uint16_t read_u16_le(const uint8_t *bytes)
{
    return (uint16_t)bytes[0] | ((uint16_t)bytes[1] << 8);
}

/* LD2450 does not use two's complement. Bit 15 means positive and the
 * remaining 15 bits contain the magnitude. A clear sign bit means negative. */
static int16_t decode_signed_magnitude(uint16_t raw)
{
    int16_t magnitude = (int16_t)(raw & 0x7FFFu);
    return (raw & 0x8000u) != 0 ? magnitude : (int16_t)-magnitude;
}

void ld2450_parser_init(ld2450_parser_t *parser)
{
    memset(parser, 0, sizeof(*parser));
}

bool ld2450_decode_frame(const uint8_t bytes[LD2450_FRAME_SIZE], ld2450_frame_t *frame)
{
    if (memcmp(bytes, FRAME_HEADER, sizeof(FRAME_HEADER)) != 0 ||
        memcmp(bytes + LD2450_FRAME_SIZE - sizeof(FRAME_FOOTER),
               FRAME_FOOTER, sizeof(FRAME_FOOTER)) != 0) {
        return false;
    }

    for (size_t index = 0; index < LD2450_TARGET_COUNT; ++index) {
        const uint8_t *target_bytes = bytes + 4 + index * 8;
        uint16_t raw_x = read_u16_le(target_bytes);
        uint16_t raw_y = read_u16_le(target_bytes + 2);
        uint16_t raw_speed = read_u16_le(target_bytes + 4);
        uint16_t resolution = read_u16_le(target_bytes + 6);

        ld2450_target_t *target = &frame->targets[index];
        target->present = raw_x != 0 || raw_y != 0 || raw_speed != 0 || resolution != 0;
        target->x_mm = target->present ? decode_signed_magnitude(raw_x) : 0;
        target->y_mm = target->present ? decode_signed_magnitude(raw_y) : 0;
        target->speed_cm_s = target->present ? decode_signed_magnitude(raw_speed) : 0;
        target->resolution_mm = resolution;
    }
    return true;
}

bool ld2450_parser_push(ld2450_parser_t *parser, uint8_t byte, ld2450_frame_t *frame)
{
    if (parser->length < sizeof(FRAME_HEADER)) {
        if (byte == FRAME_HEADER[parser->length]) {
            parser->bytes[parser->length++] = byte;
        } else {
            parser->length = byte == FRAME_HEADER[0] ? 1 : 0;
            if (parser->length == 1) {
                parser->bytes[0] = byte;
            }
        }
        return false;
    }

    parser->bytes[parser->length++] = byte;
    if (parser->length < LD2450_FRAME_SIZE) {
        return false;
    }

    bool decoded = ld2450_decode_frame(parser->bytes, frame);
    if (decoded) {
        parser->length = 0;
        return true;
    }

    /* A damaged byte must not make us discard the beginning of the next frame.
     * Retain any complete header found inside the rejected 30-byte candidate. */
    parser->length = 0;
    for (size_t offset = 1; offset + sizeof(FRAME_HEADER) <= LD2450_FRAME_SIZE;
         ++offset) {
        if (memcmp(parser->bytes + offset, FRAME_HEADER, sizeof(FRAME_HEADER)) == 0) {
            parser->length = LD2450_FRAME_SIZE - offset;
            memmove(parser->bytes, parser->bytes + offset, parser->length);
            break;
        }
    }
    return false;
}
