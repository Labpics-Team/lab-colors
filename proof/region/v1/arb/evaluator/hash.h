#ifndef LABCOLOR_ARB_HASH_H
#define LABCOLOR_ARB_HASH_H

#include <stddef.h>
#include <stdint.h>

typedef struct {
    uint32_t state[8];
    uint64_t bit_length;
    uint8_t block[64];
    size_t block_length;
} lc_sha256_context;

void lc_sha256_init(lc_sha256_context *context);
void lc_sha256_update(lc_sha256_context *context, const uint8_t *bytes, size_t length);
void lc_sha256_finish(lc_sha256_context *context, uint8_t digest[32]);
void lc_sha256(const uint8_t *bytes, size_t length, uint8_t digest[32]);

#endif
