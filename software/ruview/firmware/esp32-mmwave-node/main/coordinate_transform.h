#pragma once

#include <stdbool.h>
#include <stdint.h>

void coordinate_transform_to_room(int32_t origin_x_mm,
                                  int32_t origin_z_mm,
                                  int32_t yaw_mdeg,
                                  bool invert_raw_x,
                                  int16_t raw_x_mm,
                                  int16_t forward_y_mm,
                                  int32_t *room_x_mm,
                                  int32_t *room_z_mm);
