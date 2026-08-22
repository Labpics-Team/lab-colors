#ifndef LABCOLOR_MPFI_REGION_H
#define LABCOLOR_MPFI_REGION_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "interval.h"

typedef enum {
    LC_MPFI_REGION_INSIDE = 0,
    LC_MPFI_REGION_OUTSIDE = 1,
    LC_MPFI_REGION_BOUNDARY_UNPROVEN = 2,
    LC_MPFI_REGION_RESOURCE_LIMIT_REACHED = 3
} lc_mpfi_region_outcome;

typedef struct {
    __mpfi_struct tone;
    __mpfi_struct center_a;
    __mpfi_struct center_b;
    __mpfi_struct radius_squared;
} lc_mpfi_region_knot;

typedef struct {
    __mpfi_struct metric_aa;
    __mpfi_struct metric_ab;
    __mpfi_struct metric_bb;
    lc_mpfi_region_knot *knots;
    size_t knot_count;
    mpfr_prec_t precision;
} lc_mpfi_region;

typedef struct {
    lc_mpfi_region_outcome outcome;
    lc_mpfi_status formula_status;
    bool exact_boundary;
    bool has_enclosure;
    uint64_t exact_branch;
    uint64_t consumed_branches;
    __mpfi_struct enclosure;
    mpfr_prec_t precision;
} lc_mpfi_region_result;

bool lc_mpfi_region_init(
    lc_mpfi_region *region,
    size_t knot_count,
    mpfr_prec_t precision
);
void lc_mpfi_region_clear(lc_mpfi_region *region);
bool lc_mpfi_region_result_init(
    lc_mpfi_region_result *result,
    mpfr_prec_t precision
);
void lc_mpfi_region_result_clear(lc_mpfi_region_result *result);
void lc_mpfi_region_decide(
    lc_mpfi_region_result *result,
    mpfi_srcptr point,
    const lc_mpfi_region *region,
    mpfr_prec_t precision,
    uint64_t branch_grant
);
void lc_mpfi_region_evaluate_rgb(
    lc_mpfi_region_result *result,
    const uint8_t rgb[3],
    mpfi_srcptr context,
    uint8_t surround,
    const lc_mpfi_region *region,
    mpfr_prec_t precision,
    uint64_t branch_grant
);

#endif
