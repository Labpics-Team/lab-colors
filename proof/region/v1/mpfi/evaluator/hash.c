#include "hash.h"

#include <string.h>

static const uint32_t constants[64] = {
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
load_word(const uint8_t *source)
{
    return ((uint32_t) source[0] << 24)
        | ((uint32_t) source[1] << 16)
        | ((uint32_t) source[2] << 8)
        | (uint32_t) source[3];
}

static void
store_word(uint8_t *destination, uint32_t value)
{
    destination[0] = (uint8_t) (value >> 24);
    destination[1] = (uint8_t) (value >> 16);
    destination[2] = (uint8_t) (value >> 8);
    destination[3] = (uint8_t) value;
}

static uint32_t
rotate(uint32_t value, unsigned distance)
{
    return (value >> distance) | (value << (32U - distance));
}

static void
compress(lc_mpfi_sha256 *state, const uint8_t block[64])
{
    uint32_t schedule[64];
    uint32_t a = state->words[0];
    uint32_t b = state->words[1];
    uint32_t c = state->words[2];
    uint32_t d = state->words[3];
    uint32_t e = state->words[4];
    uint32_t f = state->words[5];
    uint32_t g = state->words[6];
    uint32_t h = state->words[7];

    for (size_t index = 0; index < 16; ++index) {
        schedule[index] = load_word(block + index * 4);
    }
    for (size_t index = 16; index < 64; ++index) {
        uint32_t older = schedule[index - 15];
        uint32_t newer = schedule[index - 2];
        uint32_t sigma0 = rotate(older, 7) ^ rotate(older, 18) ^ (older >> 3);
        uint32_t sigma1 = rotate(newer, 17) ^ rotate(newer, 19) ^ (newer >> 10);

        schedule[index] = schedule[index - 16] + sigma0 + schedule[index - 7] + sigma1;
    }
    for (size_t index = 0; index < 64; ++index) {
        uint32_t upper = rotate(e, 6) ^ rotate(e, 11) ^ rotate(e, 25);
        uint32_t choose = (e & f) ^ ((~e) & g);
        uint32_t first = h + upper + choose + constants[index] + schedule[index];
        uint32_t lower = rotate(a, 2) ^ rotate(a, 13) ^ rotate(a, 22);
        uint32_t majority = (a & b) ^ (a & c) ^ (b & c);
        uint32_t second = lower + majority;

        h = g;
        g = f;
        f = e;
        e = d + first;
        d = c;
        c = b;
        b = a;
        a = first + second;
    }
    state->words[0] += a;
    state->words[1] += b;
    state->words[2] += c;
    state->words[3] += d;
    state->words[4] += e;
    state->words[5] += f;
    state->words[6] += g;
    state->words[7] += h;
}

void
lc_mpfi_sha256_init(lc_mpfi_sha256 *state)
{
    static const uint32_t initial[8] = {
        UINT32_C(0x6a09e667), UINT32_C(0xbb67ae85), UINT32_C(0x3c6ef372), UINT32_C(0xa54ff53a),
        UINT32_C(0x510e527f), UINT32_C(0x9b05688c), UINT32_C(0x1f83d9ab), UINT32_C(0x5be0cd19),
    };

    memcpy(state->words, initial, sizeof(initial));
    state->bits = 0;
    state->used = 0;
}

void
lc_mpfi_sha256_update(
    lc_mpfi_sha256 *state,
    const uint8_t *bytes,
    size_t length
)
{
    while (length != 0) {
        size_t available = sizeof(state->block) - state->used;
        size_t take = length < available ? length : available;

        memcpy(state->block + state->used, bytes, take);
        state->used += take;
        bytes += take;
        length -= take;
        if (state->used == sizeof(state->block)) {
            compress(state, state->block);
            state->bits += UINT64_C(512);
            state->used = 0;
        }
    }
}

void
lc_mpfi_sha256_finish(lc_mpfi_sha256 *state, uint8_t digest[32])
{
    uint64_t length = state->bits + (uint64_t) state->used * 8;

    state->block[state->used++] = UINT8_C(0x80);
    if (state->used > 56) {
        memset(state->block + state->used, 0, sizeof(state->block) - state->used);
        compress(state, state->block);
        state->used = 0;
    }
    memset(state->block + state->used, 0, 56 - state->used);
    for (size_t index = 0; index < 8; ++index) {
        state->block[63 - index] = (uint8_t) (length >> (index * 8));
    }
    compress(state, state->block);
    for (size_t index = 0; index < 8; ++index) {
        store_word(digest + index * 4, state->words[index]);
    }
    memset(state, 0, sizeof(*state));
}

void
lc_mpfi_sha256_bytes(
    const uint8_t *bytes,
    size_t length,
    uint8_t digest[32]
)
{
    lc_mpfi_sha256 state;

    lc_mpfi_sha256_init(&state);
    lc_mpfi_sha256_update(&state, bytes, length);
    lc_mpfi_sha256_finish(&state, digest);
}
