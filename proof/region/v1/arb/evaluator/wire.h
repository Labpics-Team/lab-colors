#ifndef LABCOLOR_ARB_WIRE_H
#define LABCOLOR_ARB_WIRE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "region.h"

/*
 * M2a's direct executable has an explicit operational admission profile.
 * These bounds cap transport and transcript storage; they do not change the
 * mathematical wire grammar or the set of contextual regions it describes.
 * The controller and executor must bind the same profile before minting an
 * observation.
 */
#define LC_ARB_MAX_JOB_BYTES_V1 \
    (UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024))
#define LC_ARB_MAX_OUTPUT_BYTES_V1 \
    (UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024))
#define LC_ARB_MAX_PRECISION_BITS_V1 UINT32_C(4096)
#define LC_ARB_MAX_POLICY_RUNGS_V1 UINT32_C(32)
#define LC_ARB_MAX_KNOTS_V1 UINT64_C(1024)

#define LC_ARB_EXIT_USAGE_V1 64
#define LC_ARB_EXIT_INPUT_REJECTED_V1 65
#define LC_ARB_EXIT_INPUT_LIMIT_V1 66
#define LC_ARB_EXIT_OUTPUT_LIMIT_V1 67
#define LC_ARB_EXIT_RESOURCE_LIMIT_V1 68
#define LC_ARB_EXIT_INTERNAL_V1 70
#define LC_ARB_EXIT_IO_V1 74

typedef enum {
    LC_WIRE_OK = 0,
    LC_WIRE_TRUNCATED = 1,
    LC_WIRE_TRAILING_BYTES = 2,
    LC_WIRE_LENGTH_OUT_OF_BOUNDS = 3,
    LC_WIRE_BAD_MAGIC = 4,
    LC_WIRE_UNKNOWN_RELEASE = 5,
    LC_WIRE_NONCANONICAL = 6,
    LC_WIRE_DIGEST_MISMATCH = 7,
    LC_WIRE_ALLOCATION_FAILED = 8,
    LC_WIRE_RESOURCE_LIMIT = 9
} lc_wire_error;

typedef struct {
    const uint8_t *bytes;
    size_t length;
} lc_slice;

typedef struct {
    uint32_t start;
    uint32_t end;
} lc_ordinal_range;

typedef struct {
    lc_ordinal_range *ranges;
    size_t range_count;
    uint64_t point_count;
    uint8_t identity[32];
} lc_domain;

typedef struct {
    uint32_t *precision_ladder;
    size_t precision_count;
    uint64_t per_point_work;
    uint64_t global_pregrant;
    uint8_t identity[32];
} lc_arb_policy;

typedef struct {
    lc_region region;
    arb_struct context[2];
    uint8_t surround;
    lc_domain domain;
    lc_arb_policy policy;
    uint8_t formula_release[32];
    uint8_t job_identity[32];
    bool context_ready;
    bool region_ready;
} lc_job;

typedef struct {
    const lc_domain *domain;
    size_t range_index;
    uint32_t ordinal;
    uint64_t emitted;
} lc_domain_iterator;

bool lc_parse_job(
    lc_job *job,
    const uint8_t *bytes,
    size_t length,
    lc_wire_error *error
);
void lc_job_clear(lc_job *job);
void lc_domain_iterator_init(lc_domain_iterator *iterator, const lc_domain *domain);
bool lc_domain_iterator_next(lc_domain_iterator *iterator, uint32_t *ordinal);
void lc_ordinal_to_rgb(uint32_t ordinal, uint8_t rgb[3]);
const char *lc_wire_error_name(lc_wire_error error);

bool lc_write_all(int descriptor, const uint8_t *bytes, size_t length);
void lc_write_u32_be(uint8_t output[4], uint32_t value);
void lc_write_u64_be(uint8_t output[8], uint64_t value);

#endif
