#ifndef LABCOLOR_MPFI_WIRE_H
#define LABCOLOR_MPFI_WIRE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "region.h"

/*
 * M1.5's direct executable has an explicit resource profile.  These are
 * operational admission bounds, not mathematical restrictions on the
 * contextual-region wire grammar; M2a must bind the same profile to its
 * controller/executor limits before minting any observation.
 */
#define LC_MPFI_MAX_JOB_BYTES_V1 \
    (UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024))
#define LC_MPFI_MAX_OUTPUT_BYTES_V1 \
    (UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024))
#define LC_MPFI_MAX_PRECISION_BITS_V1 UINT32_C(4096)
#define LC_MPFI_MAX_POLICY_RUNGS_V1 UINT32_C(32)
#define LC_MPFI_MAX_KNOTS_V1 UINT64_C(1024)

typedef enum {
    LC_MPFI_WIRE_OK = 0,
    LC_MPFI_WIRE_TRUNCATED = 1,
    LC_MPFI_WIRE_TRAILING_BYTES = 2,
    LC_MPFI_WIRE_LENGTH_OUT_OF_BOUNDS = 3,
    LC_MPFI_WIRE_BAD_MAGIC = 4,
    LC_MPFI_WIRE_UNKNOWN_RELEASE = 5,
    LC_MPFI_WIRE_NONCANONICAL = 6,
    LC_MPFI_WIRE_DIGEST_MISMATCH = 7,
    LC_MPFI_WIRE_ALLOCATION_FAILED = 8,
    LC_MPFI_WIRE_RESOURCE_LIMIT = 9
} lc_mpfi_wire_error;

typedef struct {
    const uint8_t *bytes;
    size_t length;
} lc_mpfi_slice;

typedef struct {
    uint32_t start;
    uint32_t end;
} lc_mpfi_ordinal_range;

typedef struct {
    lc_mpfi_ordinal_range *ranges;
    size_t range_count;
    uint64_t point_count;
    uint8_t identity[32];
} lc_mpfi_domain;

typedef struct {
    uint32_t *precision_ladder;
    size_t precision_count;
    uint64_t per_point_work;
    uint64_t global_pregrant;
    uint8_t identity[32];
} lc_mpfi_policy;

typedef struct {
    const lc_mpfi_domain *domain;
    size_t range_index;
    uint32_t ordinal;
    uint64_t emitted;
} lc_mpfi_domain_iterator;

typedef struct {
    lc_mpfi_region region;
    __mpfi_struct context[2];
    uint8_t surround;
    lc_mpfi_domain domain;
    lc_mpfi_policy policy;
    uint8_t formula_release[32];
    uint8_t job_identity[32];
    mpfr_prec_t maximum_precision;
    bool context_ready;
    bool region_ready;
} lc_mpfi_job;

bool lc_mpfi_parse_job(
    lc_mpfi_job *job,
    const uint8_t *bytes,
    size_t length,
    lc_mpfi_wire_error *error
);
void lc_mpfi_job_clear(lc_mpfi_job *job);
void lc_mpfi_domain_iterator_init(
    lc_mpfi_domain_iterator *iterator,
    const lc_mpfi_domain *domain
);
bool lc_mpfi_domain_iterator_next(
    lc_mpfi_domain_iterator *iterator,
    uint32_t *ordinal
);
void lc_mpfi_ordinal_to_rgb(uint32_t ordinal, uint8_t rgb[3]);
const char *lc_mpfi_wire_error_name(lc_mpfi_wire_error error);

bool lc_mpfi_write_all(int descriptor, const uint8_t *bytes, size_t length);
void lc_mpfi_write_u32_be(uint8_t output[4], uint32_t value);
void lc_mpfi_write_u64_be(uint8_t output[8], uint64_t value);

#endif
