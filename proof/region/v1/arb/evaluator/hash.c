#include "hash.h"

#include <string.h>

static const uint32_t round_constants[64] = {
    UINT32_C(0x428a2f98), UINT32_C(0x71374491), UINT32_C(0xb5c0fbcf), UINT32_C(0xe9b5dba5),
    UINT32_C(0x3956c25b), UINT32_C(0x59f111f1), UINT32_C(0x923f82a4), UINT32_C(0xab1c5ed5),
    UINT32_C(0xd807aa98), UINT32_C(0x12835b01), UINT32_C(0x243185be), UINT32_C(0x550c7dc3),
    UINT32_C(0x72be5d74), UINT32_C(0x80deb1fe), UINT32_C(0x9bdc06a7), UINT32_C(0xc19bf174),
    UINT32_C(0xe49b69c1), UINT32_C(0xefbe4786), UINT32_C(0x0fc19dc6), UINT32_C(0x240ca1cc),
    UINT32_C(0x2de92c6f), UINT32_C(0x4a7484aa), UINT32_C(0x5cb0a9dc), UINT32_C(0x76f988da),
    UINT32_C(0x983e5152), UINT32_C(0xa831c66d), UINT32_C(0xb00327c8), UINT32_C(0xbf597fc7),
    UINT32_C(0xc6e00bf3), UINT32_C(0xd5a79147), UINT32_C(0x06ca6351), UINT32_C(0x14292967),
    UINT32_C(0x27b70a85), UINT32_C(0x2e1b2138), UINT32_C(0x4d2c6dfc), UINT32_C(0x53380d13),
    UINT32_C(0x650a7354), UINT32_C(0x766a0abb), UINT32_C(0x81c2c92e), UINT32_C(0x92722c85),
    UINT32_C(0xa2bfe8a1), UINT32_C(0xa81a664b), UINT32_C(0xc24b8b70), UINT32_C(0xc76c51a3),
    UINT32_C(0xd192e819), UINT32_C(0xd6990624), UINT32_C(0xf40e3585), UINT32_C(0x106aa070),
    UINT32_C(0x19a4c116), UINT32_C(0x1e376c08), UINT32_C(0x2748774c), UINT32_C(0x34b0bcb5),
    UINT32_C(0x391c0cb3), UINT32_C(0x4ed8aa4a), UINT32_C(0x5b9cca4f), UINT32_C(0x682e6ff3),
    UINT32_C(0x748f82ee), UINT32_C(0x78a5636f), UINT32_C(0x84c87814), UINT32_C(0x8cc70208),
    UINT32_C(0x90befffa), UINT32_C(0xa4506ceb), UINT32_C(0xbef9a3f7), UINT32_C(0xc67178f2),
};

static uint32_t
rotate_right(uint32_t value, unsigned distance)
{
    return (value >> distance) | (value << (32U - distance));
}

static uint32_t
read_u32_be(const uint8_t *bytes)
{
    return ((uint32_t) bytes[0] << 24)
        | ((uint32_t) bytes[1] << 16)
        | ((uint32_t) bytes[2] << 8)
        | (uint32_t) bytes[3];
}

static void
write_u32_be(uint8_t *bytes, uint32_t value)
{
    bytes[0] = (uint8_t) (value >> 24);
    bytes[1] = (uint8_t) (value >> 16);
    bytes[2] = (uint8_t) (value >> 8);
    bytes[3] = (uint8_t) value;
}

static void
compress(lc_sha256_context *context, const uint8_t block[64])
{
    uint32_t words[64];
    uint32_t a;
    uint32_t b;
    uint32_t c;
    uint32_t d;
    uint32_t e;
    uint32_t f;
    uint32_t g;
    uint32_t h;

    for (size_t index = 0; index < 16; ++index) {
        words[index] = read_u32_be(block + index * 4);
    }
    for (size_t index = 16; index < 64; ++index) {
        uint32_t s0 = rotate_right(words[index - 15], 7)
            ^ rotate_right(words[index - 15], 18)
            ^ (words[index - 15] >> 3);
        uint32_t s1 = rotate_right(words[index - 2], 17)
            ^ rotate_right(words[index - 2], 19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16] + s0 + words[index - 7] + s1;
    }

    a = context->state[0];
    b = context->state[1];
    c = context->state[2];
    d = context->state[3];
    e = context->state[4];
    f = context->state[5];
    g = context->state[6];
    h = context->state[7];
    for (size_t index = 0; index < 64; ++index) {
        uint32_t sum1 = rotate_right(e, 6) ^ rotate_right(e, 11) ^ rotate_right(e, 25);
        uint32_t choose = (e & f) ^ ((~e) & g);
        uint32_t temporary1 = h + sum1 + choose + round_constants[index] + words[index];
        uint32_t sum0 = rotate_right(a, 2) ^ rotate_right(a, 13) ^ rotate_right(a, 22);
        uint32_t majority = (a & b) ^ (a & c) ^ (b & c);
        uint32_t temporary2 = sum0 + majority;

        h = g;
        g = f;
        f = e;
        e = d + temporary1;
        d = c;
        c = b;
        b = a;
        a = temporary1 + temporary2;
    }
    context->state[0] += a;
    context->state[1] += b;
    context->state[2] += c;
    context->state[3] += d;
    context->state[4] += e;
    context->state[5] += f;
    context->state[6] += g;
    context->state[7] += h;
}

void
lc_sha256_init(lc_sha256_context *context)
{
    context->state[0] = UINT32_C(0x6a09e667);
    context->state[1] = UINT32_C(0xbb67ae85);
    context->state[2] = UINT32_C(0x3c6ef372);
    context->state[3] = UINT32_C(0xa54ff53a);
    context->state[4] = UINT32_C(0x510e527f);
    context->state[5] = UINT32_C(0x9b05688c);
    context->state[6] = UINT32_C(0x1f83d9ab);
    context->state[7] = UINT32_C(0x5be0cd19);
    context->bit_length = 0;
    context->block_length = 0;
}

void
lc_sha256_update(lc_sha256_context *context, const uint8_t *bytes, size_t length)
{
    while (length != 0) {
        size_t available = sizeof(context->block) - context->block_length;
        size_t copied = length < available ? length : available;

        memcpy(context->block + context->block_length, bytes, copied);
        context->block_length += copied;
        bytes += copied;
        length -= copied;
        if (context->block_length == sizeof(context->block)) {
            compress(context, context->block);
            context->bit_length += UINT64_C(512);
            context->block_length = 0;
        }
    }
}

void
lc_sha256_finish(lc_sha256_context *context, uint8_t digest[32])
{
    uint64_t total_bits = context->bit_length + (uint64_t) context->block_length * 8U;

    context->block[context->block_length++] = UINT8_C(0x80);
    if (context->block_length > 56) {
        memset(context->block + context->block_length, 0, 64 - context->block_length);
        compress(context, context->block);
        context->block_length = 0;
    }
    memset(context->block + context->block_length, 0, 56 - context->block_length);
    for (size_t index = 0; index < 8; ++index) {
        context->block[63 - index] = (uint8_t) (total_bits >> (index * 8));
    }
    compress(context, context->block);
    for (size_t index = 0; index < 8; ++index) {
        write_u32_be(digest + index * 4, context->state[index]);
    }
    memset(context, 0, sizeof(*context));
}

void
lc_sha256(const uint8_t *bytes, size_t length, uint8_t digest[32])
{
    lc_sha256_context context;

    lc_sha256_init(&context);
    lc_sha256_update(&context, bytes, length);
    lc_sha256_finish(&context, digest);
}
