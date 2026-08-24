#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define LD2450_FRAME_SIZE 30
#define LD2450_TARGET_COUNT 3

typedef struct {
    bool present;
    int16_t x_mm;
    int16_t y_mm;
    int16_t speed_cm_s;
    uint16_t resolution_mm;
} ld2450_target_t;

typedef struct {
    ld2450_target_t targets[LD2450_TARGET_COUNT];
} ld2450_frame_t;

typedef struct {
    uint8_t bytes[LD2450_FRAME_SIZE];
    size_t length;
} ld2450_parser_t;

void ld2450_parser_init(ld2450_parser_t *parser);
bool ld2450_parser_push(ld2450_parser_t *parser, uint8_t byte, ld2450_frame_t *frame);
bool ld2450_decode_frame(const uint8_t bytes[LD2450_FRAME_SIZE], ld2450_frame_t *frame);
