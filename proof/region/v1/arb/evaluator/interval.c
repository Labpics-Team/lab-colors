#include "interval.h"

#include <limits.h>

lc_status
lc_set_dyadic_bits(arb_t output, uint64_t bits)
{
    uint64_t exponent_bits = (bits >> 52) & UINT64_C(0x7ff);
    uint64_t significand = bits & UINT64_C(0x000fffffffffffff);
    slong exponent;
    fmpz_t integer;
    fmpz_t power;

    if (exponent_bits == UINT64_C(0x7ff) || bits == UINT64_C(0x8000000000000000)) {
        return LC_INVALID_DYADIC;
    }
    if (exponent_bits == 0) {
        exponent = -1074;
    } else {
        significand |= UINT64_C(0x0010000000000000);
        exponent = (slong) exponent_bits - 1075;
    }

    fmpz_init(integer);
    fmpz_init(power);
    fmpz_set_ui(integer, significand);
    if ((bits >> 63) != 0 && significand != 0) {
        fmpz_neg(integer, integer);
    }
    fmpz_set_si(power, exponent);
    arb_set_fmpz_2exp(output, integer, power);
    fmpz_clear(power);
    fmpz_clear(integer);
    return LC_OK;
}

void
lc_interval_get_dyadic_bounds(
    fmpz_t lower,
    fmpz_t upper,
    fmpz_t exponent,
    arb_srcptr value
)
{
    arb_get_interval_fmpz_2exp(lower, upper, exponent, value);
}

lc_status
lc_add(arb_t output, arb_srcptr left, arb_srcptr right, slong precision)
{
    arb_add(output, left, right, precision);
    return LC_OK;
}

lc_status
lc_sub(arb_t output, arb_srcptr left, arb_srcptr right, slong precision)
{
    arb_sub(output, left, right, precision);
    return LC_OK;
}

lc_status
lc_mul(arb_t output, arb_srcptr left, arb_srcptr right, slong precision)
{
    arb_mul(output, left, right, precision);
    return LC_OK;
}

lc_status
lc_div(arb_t output, arb_srcptr left, arb_srcptr right, slong precision)
{
    if (arb_contains_zero(right)) {
        return LC_DOMAIN_UNPROVEN;
    }
    arb_div(output, left, right, precision);
    return LC_OK;
}

lc_status
lc_min(arb_t output, arb_srcptr left, arb_srcptr right, slong precision)
{
    arb_min(output, left, right, precision);
    return LC_OK;
}

lc_status
lc_max(arb_t output, arb_srcptr left, arb_srcptr right, slong precision)
{
    arb_max(output, left, right, precision);
    return LC_OK;
}

lc_status
lc_root3(arb_t output, arb_srcptr input, slong precision)
{
    if (!arb_is_nonnegative(input)) {
        return LC_DOMAIN_UNPROVEN;
    }
    arb_root_ui(output, input, 3, precision);
    return LC_OK;
}

lc_status
lc_sqrt(arb_t output, arb_srcptr input, slong precision)
{
    if (!arb_is_nonnegative(input)) {
        return LC_DOMAIN_UNPROVEN;
    }
    arb_sqrt(output, input, precision);
    return LC_OK;
}

lc_status
lc_exp(arb_t output, arb_srcptr input, slong precision)
{
    arb_exp(output, input, precision);
    return LC_OK;
}

lc_status
lc_log(arb_t output, arb_srcptr input, slong precision)
{
    if (!arb_is_positive(input)) {
        return LC_DOMAIN_UNPROVEN;
    }
    arb_log(output, input, precision);
    return LC_OK;
}

lc_status
lc_sin(arb_t output, arb_srcptr input, slong precision)
{
    arb_sin(output, input, precision);
    return LC_OK;
}

lc_status
lc_cos(arb_t output, arb_srcptr input, slong precision)
{
    arb_cos(output, input, precision);
    return LC_OK;
}

lc_status
lc_abs(arb_t output, arb_srcptr input, slong precision)
{
    (void) precision;
    arb_abs(output, input);
    return LC_OK;
}

lc_status
lc_sign(arb_t output, arb_srcptr input, slong precision)
{
    (void) precision;
    arb_sgn(output, input);
    return LC_OK;
}

lc_status
lc_pow_pos(arb_t output, arb_srcptr base, arb_srcptr exponent, slong precision)
{
    arb_t logarithm;

    if (!arb_is_positive(base)) {
        return LC_DOMAIN_UNPROVEN;
    }
    arb_init(logarithm);
    arb_log(logarithm, base, precision);
    arb_mul(logarithm, logarithm, exponent, precision);
    arb_exp(output, logarithm, precision);
    arb_clear(logarithm);
    return LC_OK;
}

lc_status
lc_pow_nn(arb_t output, arb_srcptr base, arb_srcptr exponent, slong precision)
{
    if (arb_is_zero(base) && arb_is_positive(exponent)) {
        arb_zero(output);
        return LC_OK;
    }
    return lc_pow_pos(output, base, exponent, precision);
}

lc_status
lc_ratio0(arb_t output, arb_srcptr numerator, arb_srcptr denominator, slong precision)
{
    if (arb_is_zero(numerator) && arb_is_zero(denominator)) {
        arb_zero(output);
        return LC_OK;
    }
    if (!arb_is_positive(denominator)) {
        return LC_DOMAIN_UNPROVEN;
    }
    arb_div(output, numerator, denominator, precision);
    return LC_OK;
}
