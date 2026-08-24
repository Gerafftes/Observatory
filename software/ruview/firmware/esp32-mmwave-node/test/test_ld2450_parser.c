#include <assert.h>
#include <stdio.h>

#include "../main/coordinate_transform.h"
#include "../main/ld2450_parser.h"

static void test_official_example(void)
{
    const uint8_t bytes[LD2450_FRAME_SIZE] = {
        0xAA, 0xFF, 0x03, 0x00,
        0x0E, 0x03, 0xB1, 0x86, 0x10, 0x00, 0x40, 0x01,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0x55, 0xCC,
    };
    ld2450_frame_t frame;
    assert(ld2450_decode_frame(bytes, &frame));
    assert(frame.targets[0].present);
    assert(frame.targets[0].x_mm == -782);
    assert(frame.targets[0].y_mm == 1713);
    assert(frame.targets[0].speed_cm_s == -16);
    assert(frame.targets[0].resolution_mm == 320);
    assert(!frame.targets[1].present);
    assert(!frame.targets[2].present);
}

static void test_stream_resynchronizes(void)
{
    const uint8_t bytes[LD2450_FRAME_SIZE] = {
        0xAA, 0xFF, 0x03, 0x00,
        1, 0x80, 2, 0x80, 3, 0x80, 4, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0x55, 0xCC,
    };
    ld2450_parser_t parser;
    ld2450_parser_init(&parser);
    ld2450_frame_t frame;
    assert(!ld2450_parser_push(&parser, 0x99, &frame));
    for (size_t index = 0; index < LD2450_FRAME_SIZE - 1; ++index) {
        assert(!ld2450_parser_push(&parser, bytes[index], &frame));
    }
    assert(ld2450_parser_push(&parser, bytes[LD2450_FRAME_SIZE - 1], &frame));
    assert(frame.targets[0].x_mm == 1);
    assert(frame.targets[0].y_mm == 2);
    assert(frame.targets[0].speed_cm_s == 3);
}

static void test_corrupt_frame_keeps_nested_header(void)
{
    const uint8_t valid[LD2450_FRAME_SIZE] = {
        0xAA, 0xFF, 0x03, 0x00,
        9, 0x80, 8, 0x80, 7, 0x80, 6, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
        0x55, 0xCC,
    };
    ld2450_parser_t parser;
    ld2450_parser_init(&parser);
    ld2450_frame_t frame;
    const uint8_t corrupt_prefix[] = {
        0xAA, 0xFF, 0x03, 0x00, 1, 2, 3, 4, 5, 6,
    };
    for (size_t index = 0; index < sizeof(corrupt_prefix); ++index) {
        assert(!ld2450_parser_push(&parser, corrupt_prefix[index], &frame));
    }
    for (size_t index = 0; index < 20; ++index) {
        assert(!ld2450_parser_push(&parser, valid[index], &frame));
    }
    for (size_t index = 20; index < LD2450_FRAME_SIZE - 1; ++index) {
        assert(!ld2450_parser_push(&parser, valid[index], &frame));
    }
    assert(ld2450_parser_push(&parser, valid[LD2450_FRAME_SIZE - 1], &frame));
    assert(frame.targets[0].x_mm == 9);
}

static void test_room_coordinate_transform(void)
{
    int32_t room_x;
    int32_t room_z;
    coordinate_transform_to_room(2000, 3000, 0, false, 100, 1000,
                                 &room_x, &room_z);
    assert(room_x == 3000);
    assert(room_z == 3100);

    coordinate_transform_to_room(2000, 3000, -90000, false, 100, 1000,
                                 &room_x, &room_z);
    assert(room_x == 2100);
    assert(room_z == 2000);

    coordinate_transform_to_room(2000, 3000, 0, true, 100, 1000,
                                 &room_x, &room_z);
    assert(room_x == 3000);
    assert(room_z == 2900);
}

int main(void)
{
    test_official_example();
    test_stream_resynchronizes();
    test_corrupt_frame_keeps_nested_header();
    test_room_coordinate_transform();
    puts("ld2450 parser tests passed");
    return 0;
}
