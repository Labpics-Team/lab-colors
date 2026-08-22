#ifndef LABCOLOR_MPFI_HASH_H
#define LABCOLOR_MPFI_HASH_H

#include <stddef.h>
#include <stdint.h>

typedef struct {
    uint32_t words[8];
    uint64_t bits;
    uint8_t block[64];
    size_t used;
} lc_mpfi_sha256;

void lc_mpfi_sha256_init(lc_mpfi_sha256 *state);
void lc_mpfi_sha256_update(
    lc_mpfi_sha256 *state,
    const uint8_t *bytes,
    size_t length
);
void lc_mpfi_sha256_finish(lc_mpfi_sha256 *state, uint8_t digest[32]);
void lc_mpfi_sha256_bytes(
    const uint8_t *bytes,
    size_t length,
    uint8_t digest[32]
);

#endif
