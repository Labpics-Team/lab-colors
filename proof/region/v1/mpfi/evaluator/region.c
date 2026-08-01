#include "region.h"

#include <stdlib.h>

#include "formula.h"

static void
clear_knot(lc_mpfi_region_knot *knot)
{
    mpfi_clear(&knot->radius_squared);
    mpfi_clear(&knot->center_b);
    mpfi_clear(&knot->center_a);
    mpfi_clear(&knot->tone);
}

static void
init_knot(lc_mpfi_region_knot *knot, mpfr_prec_t precision)
{
    mpfi_init2(&knot->tone, precision);
    mpfi_init2(&knot->center_a, precision);
    mpfi_init2(&knot->center_b, precision);
    mpfi_init2(&knot->radius_squared, precision);
}

static void
reset_result(lc_mpfi_region_result *result)
{
    result->outcome = LC_MPFI_REGION_BOUNDARY_UNPROVEN;
    result->formula_status = LC_MPFI_OK;
    result->exact_boundary = false;
    result->has_enclosure = false;
    result->exact_branch = 0;
    result->consumed_branches = 0;
    mpfi_set_ui(&result->enclosure, 0);
}

static bool
interval_strictly_below(mpfi_srcptr left, mpfi_srcptr right)
{
    mpfr_prec_t precision = mpfi_get_prec(left);
    mpfr_t left_high;
    mpfr_t right_low;
    int result;

    mpfr_inits2(precision, left_high, right_low, (mpfr_ptr) 0);
    mpfi_get_right(left_high, left);
    mpfi_get_left(right_low, right);
    result = mpfr_cmp(left_high, right_low) < 0;
    mpfr_clears(left_high, right_low, (mpfr_ptr) 0);
    return result != 0;
}

static bool
interval_strictly_above(mpfi_srcptr left, mpfi_srcptr right)
{
    return interval_strictly_below(right, left);
}

static bool
interval_at_least(mpfi_srcptr left, mpfi_srcptr right)
{
    mpfr_prec_t precision = mpfi_get_prec(left);
    mpfr_t left_low;
    mpfr_t right_high;
    int result;

    mpfr_inits2(precision, left_low, right_high, (mpfr_ptr) 0);
    mpfi_get_left(left_low, left);
    mpfi_get_right(right_high, right);
    result = mpfr_cmp(left_low, right_high) >= 0;
    mpfr_clears(left_low, right_high, (mpfr_ptr) 0);
    return result != 0;
}

static bool
interval_at_most(mpfi_srcptr left, mpfi_srcptr right)
{
    mpfr_prec_t precision = mpfi_get_prec(left);
    mpfr_t left_high;
    mpfr_t right_low;
    int result;

    mpfr_inits2(precision, left_high, right_low, (mpfr_ptr) 0);
    mpfi_get_right(left_high, left);
    mpfi_get_left(right_low, right);
    result = mpfr_cmp(left_high, right_low) <= 0;
    mpfr_clears(left_high, right_low, (mpfr_ptr) 0);
    return result != 0;
}

static bool
interval_equal(mpfi_srcptr left, mpfi_srcptr right)
{
    __mpfi_struct difference;
    bool equal;

    mpfi_init2(&difference, mpfi_get_prec(left));
    mpfi_sub(&difference, left, right);
    equal = mpfi_is_zero(&difference) != 0;
    mpfi_clear(&difference);
    return equal;
}

static void
record_enclosure(lc_mpfi_region_result *result, mpfi_srcptr value)
{
    if (result->has_enclosure) {
        mpfi_union(&result->enclosure, &result->enclosure, value);
    } else {
        mpfi_set(&result->enclosure, value);
        result->has_enclosure = true;
    }
}

bool
lc_mpfi_region_init(
    lc_mpfi_region *region,
    size_t knot_count,
    mpfr_prec_t precision
)
{
    region->knots = NULL;
    region->knot_count = 0;
    region->precision = precision;
    mpfi_init2(&region->metric_aa, precision);
    mpfi_init2(&region->metric_ab, precision);
    mpfi_init2(&region->metric_bb, precision);
    if (knot_count == 0 || knot_count > SIZE_MAX / sizeof(*region->knots)) {
        lc_mpfi_region_clear(region);
        return false;
    }
    region->knots = calloc(knot_count, sizeof(*region->knots));
    if (region->knots == NULL) {
        lc_mpfi_region_clear(region);
        return false;
    }
    region->knot_count = knot_count;
    for (size_t index = 0; index < knot_count; ++index) {
        init_knot(region->knots + index, precision);
    }
    return true;
}

void
lc_mpfi_region_clear(lc_mpfi_region *region)
{
    if (region->knots != NULL) {
        for (size_t index = 0; index < region->knot_count; ++index) {
            clear_knot(region->knots + index);
        }
        free(region->knots);
    }
    mpfi_clear(&region->metric_bb);
    mpfi_clear(&region->metric_ab);
    mpfi_clear(&region->metric_aa);
    region->knots = NULL;
    region->knot_count = 0;
    region->precision = 0;
}

bool
lc_mpfi_region_result_init(lc_mpfi_region_result *result, mpfr_prec_t precision)
{
    if (precision == 0) {
        return false;
    }
    result->precision = precision;
    mpfi_init2(&result->enclosure, precision);
    reset_result(result);
    return true;
}

void
lc_mpfi_region_result_clear(lc_mpfi_region_result *result)
{
    mpfi_clear(&result->enclosure);
    result->precision = 0;
}

static void
evaluate_singleton(
    lc_mpfi_region_result *result,
    mpfi_srcptr point,
    const lc_mpfi_region *region,
    mpfr_prec_t precision,
    uint64_t branch_grant
)
{
    __mpfi_struct input[8];
    __mpfi_struct predicate;

    if (!interval_equal(point, &region->knots[0].tone)) {
        __mpfi_struct overlap;

        mpfi_init2(&overlap, precision);
        mpfi_intersect(&overlap, point, &region->knots[0].tone);
        result->outcome = mpfi_is_empty(&overlap)
            ? LC_MPFI_REGION_OUTSIDE
            : LC_MPFI_REGION_BOUNDARY_UNPROVEN;
        mpfi_clear(&overlap);
        return;
    }
    if (branch_grant == 0) {
        result->outcome = LC_MPFI_REGION_RESOURCE_LIMIT_REACHED;
        return;
    }
    for (size_t index = 0; index < 8; ++index) {
        mpfi_init2(input + index, precision);
    }
    mpfi_set(input + 0, point + 1);
    mpfi_set(input + 1, point + 2);
    mpfi_set(input + 2, &region->knots[0].center_a);
    mpfi_set(input + 3, &region->knots[0].center_b);
    mpfi_set(input + 4, &region->knots[0].radius_squared);
    mpfi_set(input + 5, &region->metric_aa);
    mpfi_set(input + 6, &region->metric_ab);
    mpfi_set(input + 7, &region->metric_bb);
    mpfi_init2(&predicate, precision);
    result->formula_status = lc_mpfi_formula_singleton(&predicate, input);
    result->consumed_branches = 1;
    if (result->formula_status == LC_MPFI_OK) {
        record_enclosure(result, &predicate);
        if (mpfi_is_nonpos(&predicate)) {
            result->outcome = LC_MPFI_REGION_INSIDE;
            result->exact_boundary = mpfi_is_zero(&predicate) != 0;
            result->exact_branch = 0;
        } else if (mpfi_is_pos(&predicate)) {
            result->outcome = LC_MPFI_REGION_OUTSIDE;
        }
    }
    mpfi_clear(&predicate);
    for (size_t index = 8; index-- != 0;) {
        mpfi_clear(input + index);
    }
}

void
lc_mpfi_region_decide(
    lc_mpfi_region_result *result,
    mpfi_srcptr point,
    const lc_mpfi_region *region,
    mpfr_prec_t precision,
    uint64_t branch_grant
)
{
    bool any_segment = false;
    bool all_inside = true;
    bool all_outside = true;
    bool exact_zero = false;
    bool outside_possible;
    uint64_t exact_branch = 0;
    __mpfi_struct segment_domain;
    __mpfi_struct intersection;

    reset_result(result);
    if (precision < 2) {
        result->formula_status = LC_MPFI_DOMAIN_UNPROVEN;
        return;
    }
    if (region->knot_count == 1) {
        evaluate_singleton(result, point, region, precision, branch_grant);
        return;
    }
    if (region->knot_count < 2) {
        result->formula_status = LC_MPFI_DOMAIN_UNPROVEN;
        return;
    }
    if (interval_strictly_below(point, &region->knots[0].tone)
        || interval_strictly_above(point, &region->knots[region->knot_count - 1].tone)) {
        result->outcome = LC_MPFI_REGION_OUTSIDE;
        return;
    }
    outside_possible = !interval_at_least(point, &region->knots[0].tone)
        || !interval_at_most(point, &region->knots[region->knot_count - 1].tone);
    mpfi_init2(&segment_domain, precision);
    mpfi_init2(&intersection, precision);
    for (size_t index = 0; index + 1 < region->knot_count; ++index) {
        const lc_mpfi_region_knot *left = region->knots + index;
        const lc_mpfi_region_knot *right = region->knots + index + 1;
        __mpfi_struct input[14];
        __mpfi_struct predicate;

        mpfi_union(&segment_domain, &left->tone, &right->tone);
        mpfi_intersect(&intersection, point, &segment_domain);
        if (mpfi_is_empty(&intersection)) {
            continue;
        }
        any_segment = true;
        if (result->consumed_branches == branch_grant) {
            result->outcome = LC_MPFI_REGION_RESOURCE_LIMIT_REACHED;
            break;
        }
        for (size_t input_index = 0; input_index < 14; ++input_index) {
            mpfi_init2(input + input_index, precision);
        }
        mpfi_set(input + 0, &intersection);
        mpfi_set(input + 1, point + 1);
        mpfi_set(input + 2, point + 2);
        mpfi_set(input + 3, &left->tone);
        mpfi_set(input + 4, &right->tone);
        mpfi_set(input + 5, &left->center_a);
        mpfi_set(input + 6, &left->center_b);
        mpfi_set(input + 7, &right->center_a);
        mpfi_set(input + 8, &right->center_b);
        mpfi_set(input + 9, &left->radius_squared);
        mpfi_set(input + 10, &right->radius_squared);
        mpfi_set(input + 11, &region->metric_aa);
        mpfi_set(input + 12, &region->metric_ab);
        mpfi_set(input + 13, &region->metric_bb);
        mpfi_init2(&predicate, precision);
        result->formula_status = lc_mpfi_formula_segment(&predicate, input);
        ++result->consumed_branches;
        if (result->formula_status == LC_MPFI_OK) {
            bool inside = mpfi_is_nonpos(&predicate) != 0;
            bool outside = mpfi_is_pos(&predicate) != 0;
            bool branch_exact = mpfi_is_zero(&predicate) != 0;

            record_enclosure(result, &predicate);
            all_inside = all_inside && inside;
            all_outside = all_outside && outside;
            if (branch_exact && !exact_zero) {
                exact_branch = (uint64_t) index;
            }
            exact_zero = exact_zero || branch_exact;
        } else {
            all_inside = false;
            all_outside = false;
        }
        mpfi_clear(&predicate);
        for (size_t input_index = 14; input_index-- != 0;) {
            mpfi_clear(input + input_index);
        }
    }
    if (result->outcome == LC_MPFI_REGION_RESOURCE_LIMIT_REACHED) {
        mpfi_clear(&intersection);
        mpfi_clear(&segment_domain);
        return;
    }
    if (!any_segment) {
        result->outcome = LC_MPFI_REGION_BOUNDARY_UNPROVEN;
    } else if (all_outside) {
        result->outcome = LC_MPFI_REGION_OUTSIDE;
    } else if (all_inside && !outside_possible) {
        result->outcome = LC_MPFI_REGION_INSIDE;
        result->exact_boundary = exact_zero;
        result->exact_branch = exact_branch;
    } else {
        result->outcome = LC_MPFI_REGION_BOUNDARY_UNPROVEN;
    }
    mpfi_clear(&intersection);
    mpfi_clear(&segment_domain);
}

void
lc_mpfi_region_evaluate_rgb(
    lc_mpfi_region_result *result,
    const uint8_t rgb[3],
    mpfi_srcptr context,
    uint8_t surround,
    const lc_mpfi_region *region,
    mpfr_prec_t precision,
    uint64_t branch_grant
)
{
    __mpfi_struct point[3];

    reset_result(result);
    if (precision < 2) {
        result->formula_status = LC_MPFI_DOMAIN_UNPROVEN;
        return;
    }
    for (size_t index = 0; index < 3; ++index) {
        mpfi_init2(point + index, precision);
    }
    result->formula_status = lc_mpfi_formula_point(point, rgb, context, surround);
    if (result->formula_status == LC_MPFI_OK) {
        lc_mpfi_region_decide(result, point, region, precision, branch_grant);
    }
    for (size_t index = 3; index-- != 0;) {
        mpfi_clear(point + index);
    }
}
