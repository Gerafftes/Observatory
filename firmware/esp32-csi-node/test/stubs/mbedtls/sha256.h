/*
 * Minimal Mbed TLS SHA-256 declaration for host-side CSI serializer tests.
 * The test-double implementation lives in stubs/esp_stubs.c.
 */
#ifndef MBEDTLS_SHA256_H_STUB
#define MBEDTLS_SHA256_H_STUB

#include <stddef.h>

int mbedtls_sha256(
    const unsigned char *input,
    size_t ilen,
    unsigned char output[32],
    int is224
);

#endif /* MBEDTLS_SHA256_H_STUB */
