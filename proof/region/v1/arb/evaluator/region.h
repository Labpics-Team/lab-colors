#ifndef LABCOLOR_ARB_REGION_H
#define LABCOLOR_ARB_REGION_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "interval.h"

typedef enum {
    LC_REGION_INSIDE = 0,
    LC_REGION_OUTSIDE = 1,
    LC_REGION_BOUNDARY_UNPROVEN = 2,
    LC_REGION_RESOURCE_LIMIT_REACHED = 3
} lc_region_outcome;

typedef struct {
    arb_struct tone;
    arb_struct center_a;
    arb_struct center_b;
    arb_struct radius_squared;
} lc_region_knot;

typedef struct {
    arb_struct metric_aa;
    arb_struct metric_ab;
    arb_struct metric_bb;
    lc_region_knot *knots;
    size_t knot_count;
} lc_region;

typedef struct {
    lc_region_outcome outcome;
    lc_status formula_status;
    bool exact_boundary;
    bool has_enclosure;
    uint64_t exact_branch;
    uint64_t consumed_branches;
    arb_struct enclosure;
} lc_region_result;

bool lc_region_init(lc_region *region, size_t knot_count);
void lc_region_clear(lc_region *region);
void lc_region_result_init(lc_region_result *result);
void lc_region_result_clear(lc_region_result *result);
void lc_region_decide(
    lc_region_result *result,
    arb_srcptr point,
    const lc_region *region,
    slong precision,
    uint64_t branch_grant
);
void lc_region_evaluate_rgb(
    lc_region_result *result,
    const uint8_t rgb[3],
    arb_srcptr context,
    uint8_t surround,
    const lc_region *region,
    slong precision,
    uint64_t branch_grant
);

#endif
