#include "coordinate_transform.h"

#include <math.h>

static const double PI = 3.14159265358979323846;

void coordinate_transform_to_room(int32_t origin_x_mm,
                                  int32_t origin_z_mm,
                                  int32_t yaw_mdeg,
                                  bool invert_raw_x,
                                  int16_t raw_x_mm,
                                  int16_t forward_y_mm,
                                  int32_t *room_x_mm,
                                  int32_t *room_z_mm)
{
    const double yaw = (double)yaw_mdeg * PI / 180000.0;
    const double local_right = invert_raw_x ? -raw_x_mm : raw_x_mm;
    const double local_forward = forward_y_mm;
    *room_x_mm = origin_x_mm +
        (int32_t)lround(local_forward * cos(yaw) - local_right * sin(yaw));
    *room_z_mm = origin_z_mm +
        (int32_t)lround(local_forward * sin(yaw) + local_right * cos(yaw));
}
