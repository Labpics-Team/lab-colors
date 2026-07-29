#include "region.h"

#include <stdlib.h>

#include "formula.h"

static void
reset_result(lc_region_result *result)
{
    result->outcome = LC_REGION_BOUNDARY_UNPROVEN;
    result->formula_status = LC_OK;
    result->exact_boundary = false;
    result->has_enclosure = false;
    result->exact_branch = 0;
    result->consumed_branches = 0;
    arb_zero(&result->enclosure);
}

static void
record_enclosure(lc_region_result *result, arb_srcptr value, slong precision)
{
    if (result->has_enclosure) {
        arb_union(&result->enclosure, &result->enclosure, value, precision);
    } else {
        arb_set(&result->enclosure, value);
        result->has_enclosure = true;
    }
}

bool
lc_region_init(lc_region *region, size_t knot_count)
{
    region->knots = NULL;
    region->knot_count = 0;
    arb_init(&region->metric_aa);
    arb_init(&region->metric_ab);
    arb_init(&region->metric_bb);
    if (knot_count == 0 || knot_count > SIZE_MAX / sizeof(*region->knots)) {
        lc_region_clear(region);
        return false;
    }
    region->knots = calloc(knot_count, sizeof(*region->knots));
    if (region->knots == NULL) {
        lc_region_clear(region);
        return false;
    }
    region->knot_count = knot_count;
    for (size_t index = 0; index < knot_count; ++index) {
        arb_init(&region->knots[index].tone);
        arb_init(&region->knots[index].center_a);
        arb_init(&region->knots[index].center_b);
        arb_init(&region->knots[index].radius_squared);
    }
    return true;
}

void
lc_region_clear(lc_region *region)
{
    if (region->knots != NULL) {
        for (size_t index = 0; index < region->knot_count; ++index) {
            arb_clear(&region->knots[index].radius_squared);
            arb_clear(&region->knots[index].center_b);
            arb_clear(&region->knots[index].center_a);
            arb_clear(&region->knots[index].tone);
        }
        free(region->knots);
    }
    arb_clear(&region->metric_bb);
    arb_clear(&region->metric_ab);
    arb_clear(&region->metric_aa);
    region->knots = NULL;
    region->knot_count = 0;
}

void
lc_region_result_init(lc_region_result *result)
{
    arb_init(&result->enclosure);
    reset_result(result);
}

void
lc_region_result_clear(lc_region_result *result)
{
    arb_clear(&result->enclosure);
}

static void
evaluate_singleton(
    lc_region_result *result,
    arb_srcptr point,
    const lc_region *region,
    slong precision,
    uint64_t branch_grant
)
{
    arb_struct input[8];
    arb_t predicate;

    if (!arb_equal(point, &region->knots[0].tone)) {
        result->outcome = arb_overlaps(point, &region->knots[0].tone)
            ? LC_REGION_BOUNDARY_UNPROVEN
            : LC_REGION_OUTSIDE;
        return;
    }
    if (branch_grant == 0) {
        result->outcome = LC_REGION_RESOURCE_LIMIT_REACHED;
        return;
    }
    for (size_t index = 0; index < 8; ++index) {
        arb_init(input + index);
    }
    arb_set(input + 0, point + 1);
    arb_set(input + 1, point + 2);
    arb_set(input + 2, &region->knots[0].center_a);
    arb_set(input + 3, &region->knots[0].center_b);
    arb_set(input + 4, &region->knots[0].radius_squared);
    arb_set(input + 5, &region->metric_aa);
    arb_set(input + 6, &region->metric_ab);
    arb_set(input + 7, &region->metric_bb);
    arb_init(predicate);
    result->formula_status = lc_formula_singleton(predicate, input, precision);
    result->consumed_branches = 1;
    if (result->formula_status == LC_OK) {
        record_enclosure(result, predicate, precision);
        if (arb_is_nonpositive(predicate)) {
            result->outcome = LC_REGION_INSIDE;
            result->exact_boundary = arb_is_zero(predicate);
            result->exact_branch = 0;
        } else if (arb_is_positive(predicate)) {
            result->outcome = LC_REGION_OUTSIDE;
        }
    }
    arb_clear(predicate);
    for (size_t index = 8; index-- != 0;) {
        arb_clear(input + index);
    }
}

void
lc_region_decide(
    lc_region_result *result,
    arb_srcptr point,
    const lc_region *region,
    slong precision,
    uint64_t branch_grant
)
{
    bool any_segment = false;
    bool all_inside = true;
    bool all_outside = true;
    bool exact_zero = false;
    bool outside_possible;
    uint64_t exact_branch = 0;
    arb_t segment_domain;
    arb_t intersection;

    reset_result(result);
    /* FLINT's two-bit minimum applies to the public decision entry point too;
       otherwise a singleton can bypass the policy before any segment exists. */
    if (precision < 2) {
        result->formula_status = LC_DOMAIN_UNPROVEN;
        return;
    }
    if (region->knot_count == 1) {
        evaluate_singleton(result, point, region, precision, branch_grant);
        return;
    }
    if (region->knot_count < 2) {
        result->formula_status = LC_DOMAIN_UNPROVEN;
        return;
    }
    if (arb_lt(point, &region->knots[0].tone)
        || arb_gt(point, &region->knots[region->knot_count - 1].tone)) {
        result->outcome = LC_REGION_OUTSIDE;
        return;
    }
    outside_possible = !arb_ge(point, &region->knots[0].tone)
        || !arb_le(point, &region->knots[region->knot_count - 1].tone);
    arb_init(segment_domain);
    arb_init(intersection);
    for (size_t index = 0; index + 1 < region->knot_count; ++index) {
        const lc_region_knot *left = region->knots + index;
        const lc_region_knot *right = region->knots + index + 1;
        arb_struct input[14];
        arb_t predicate;

        arb_union(segment_domain, &left->tone, &right->tone, precision);
        if (!arb_intersection(intersection, point, segment_domain, precision)) {
            continue;
        }
        any_segment = true;
        if (result->consumed_branches == branch_grant) {
            result->outcome = LC_REGION_RESOURCE_LIMIT_REACHED;
            goto cleanup;
        }
        for (size_t input_index = 0; input_index < 14; ++input_index) {
            arb_init(input + input_index);
        }
        arb_set(input + 0, intersection);
        arb_set(input + 1, point + 1);
        arb_set(input + 2, point + 2);
        arb_set(input + 3, &left->tone);
        arb_set(input + 4, &right->tone);
        arb_set(input + 5, &left->center_a);
        arb_set(input + 6, &left->center_b);
        arb_set(input + 7, &right->center_a);
        arb_set(input + 8, &right->center_b);
        arb_set(input + 9, &left->radius_squared);
        arb_set(input + 10, &right->radius_squared);
        arb_set(input + 11, &region->metric_aa);
        arb_set(input + 12, &region->metric_ab);
        arb_set(input + 13, &region->metric_bb);
        arb_init(predicate);
        result->formula_status = lc_formula_segment(predicate, input, precision);
        ++result->consumed_branches;
        if (result->formula_status == LC_OK) {
            bool inside = arb_is_nonpositive(predicate);
            bool outside = arb_is_positive(predicate);
            bool branch_exact = arb_is_zero(predicate);

            record_enclosure(result, predicate, precision);
            all_inside = all_inside && inside;
            all_outside = all_outside && outside;
            /* Strict segment order makes the first exact branch canonical. */
            if (branch_exact && !exact_zero) {
                exact_branch = (uint64_t) index;
            }
            exact_zero = exact_zero || branch_exact;
        } else {
            all_inside = false;
            all_outside = false;
        }
        arb_clear(predicate);
        for (size_t input_index = 14; input_index-- != 0;) {
            arb_clear(input + input_index);
        }
    }
    if (!any_segment) {
        result->outcome = LC_REGION_BOUNDARY_UNPROVEN;
    } else if (all_outside) {
        result->outcome = LC_REGION_OUTSIDE;
    } else if (all_inside && !outside_possible) {
        result->outcome = LC_REGION_INSIDE;
        result->exact_boundary = exact_zero;
        result->exact_branch = exact_branch;
    } else {
        result->outcome = LC_REGION_BOUNDARY_UNPROVEN;
    }

cleanup:
    arb_clear(intersection);
    arb_clear(segment_domain);
}

void
lc_region_evaluate_rgb(
    lc_region_result *result,
    const uint8_t rgb[3],
    arb_srcptr context,
    uint8_t surround,
    const lc_region *region,
    slong precision,
    uint64_t branch_grant
)
{
    arb_struct point[3];

    reset_result(result);
    /* FLINT defines two bits as its minimum working precision. Lower policy
       rungs remain unresolved and must never enter Arb arithmetic. */
    if (precision < 2) {
        result->formula_status = LC_DOMAIN_UNPROVEN;
        return;
    }
    for (size_t index = 0; index < 3; ++index) {
        arb_init(point + index);
    }
    result->formula_status = lc_formula_point(point, rgb, context, surround, precision);
    if (result->formula_status == LC_OK) {
        lc_region_decide(result, point, region, precision, branch_grant);
    }
    for (size_t index = 3; index-- != 0;) {
        arb_clear(point + index);
    }
}
