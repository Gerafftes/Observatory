/**
 * @file csi_samples.h
 * @brief Canonical ESP-IDF CSI complex-sample decoding helpers.
 *
 * ESP-IDF 5.4 delivers each signed complex sample in imaginary/real order:
 * byte 0 is Q (imaginary), byte 1 is I (real). Keeping this in one helper
 * prevents phase-sign drift between edge features and host-side fixtures.
 */

#ifndef CSI_SAMPLES_H
#define CSI_SAMPLES_H

#include <math.h>
#include <stdint.h>

static inline int8_t csi_sample_imaginary(const uint8_t *pair)
{
    return (int8_t)pair[0];
}

static inline int8_t csi_sample_real(const uint8_t *pair)
{
    return (int8_t)pair[1];
}

static inline float csi_sample_phase(const uint8_t *pair)
{
    return atan2f(
        (float)csi_sample_imaginary(pair),
        (float)csi_sample_real(pair)
    );
}

static inline float csi_sample_amplitude(const uint8_t *pair)
{
    float imaginary = (float)csi_sample_imaginary(pair);
    float real = (float)csi_sample_real(pair);
    return sqrtf(real * real + imaginary * imaginary);
}

#endif /* CSI_SAMPLES_H */
