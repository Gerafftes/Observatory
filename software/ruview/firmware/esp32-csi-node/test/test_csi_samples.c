#include "csi_samples.h"

#include <math.h>
#include <stdio.h>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

static int failures;

static void check_near(float actual, float expected, const char *name)
{
    if (fabsf(actual - expected) > 0.0001f) {
        fprintf(stderr, "FAIL: %s: got %.6f, expected %.6f\n",
                name, actual, expected);
        failures++;
    }
}

int main(void)
{
    const uint8_t positive_real[] = {0, 1};   /* Q=0, I=1 */
    const uint8_t positive_imag[] = {1, 0};   /* Q=1, I=0 */
    const uint8_t negative_real[] = {0, 255}; /* Q=0, I=-1 */
    const uint8_t negative_imag[] = {255, 0}; /* Q=-1, I=0 */
    const uint8_t three_four[] = {4, 3};      /* Q=4, I=3 */

    check_near(csi_sample_phase(positive_real), 0.0f, "+real phase");
    check_near(csi_sample_phase(positive_imag), (float)M_PI / 2.0f, "+imag phase");
    check_near(fabsf(csi_sample_phase(negative_real)), (float)M_PI, "-real phase");
    check_near(csi_sample_phase(negative_imag), -(float)M_PI / 2.0f, "-imag phase");
    check_near(csi_sample_amplitude(three_four), 5.0f, "3-4-5 amplitude");

    if (failures != 0) {
        return 1;
    }
    puts("CSI sample semantics tests passed");
    return 0;
}
