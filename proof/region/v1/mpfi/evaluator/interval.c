#include "interval.h"

#include <limits.h>

static lc_mpfi_status
lc_mpfi_set_rational_bits(mpfi_ptr output, uint64_t bits)
{
    uint64_t exponent_bits = (bits >> 52) & UINT64_C(0x7ff);
    uint64_t significand = bits & UINT64_C(0x000fffffffffffff);
    mpz_t numerator;
    mpz_t denominator;
    mpq_t rational;
    long exponent;

    if (exponent_bits == UINT64_C(0x7ff)
        || bits == UINT64_C(0x8000000000000000)) {
        return LC_MPFI_INVALID_DYADIC;
    }
    if (exponent_bits == 0) {
        exponent = -1074;
    } else {
        significand |= UINT64_C(0x0010000000000000);
        exponent = (long) exponent_bits - 1075;
    }

    mpz_init_set_ui(numerator, significand);
    mpz_init_set_ui(denominator, 1);
    if ((bits >> 63) != 0 && significand != 0) {
        mpz_neg(numerator, numerator);
    }
    if (exponent >= 0) {
        mpz_mul_2exp(numerator, numerator, (unsigned long) exponent);
    } else {
        mpz_mul_2exp(denominator, denominator, (unsigned long) -exponent);
    }
    mpq_init(rational);
    mpq_set_num(rational, numerator);
    mpq_set_den(rational, denominator);
    mpq_canonicalize(rational);
    mpfi_set_q(output, rational);
    mpq_clear(rational);
    mpz_clear(denominator);
    mpz_clear(numerator);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_set_dyadic_bits(mpfi_ptr output, uint64_t bits)
{
    return lc_mpfi_set_rational_bits(output, bits);
}

lc_mpfi_status
lc_mpfi_add(mpfi_ptr output, mpfi_srcptr left, mpfi_srcptr right)
{
    mpfi_add(output, left, right);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_sub(mpfi_ptr output, mpfi_srcptr left, mpfi_srcptr right)
{
    mpfi_sub(output, left, right);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_mul(mpfi_ptr output, mpfi_srcptr left, mpfi_srcptr right)
{
    mpfi_mul(output, left, right);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_div(mpfi_ptr output, mpfi_srcptr left, mpfi_srcptr right)
{
    if (mpfi_has_zero(right)) {
        return LC_MPFI_DOMAIN_UNPROVEN;
    }
    mpfi_div(output, left, right);
    return LC_MPFI_OK;
}

static lc_mpfi_status
lc_mpfi_endpoint_select(
    mpfi_ptr output,
    mpfi_srcptr left,
    mpfi_srcptr right,
    int choose_lower
)
{
    mpfr_prec_t precision = mpfi_get_prec(output);
    mpfr_t left_endpoint;
    mpfr_t right_endpoint;
    mpfr_t left_other;
    mpfr_t right_other;

    mpfr_inits2(precision, left_endpoint, right_endpoint, left_other, right_other, (mpfr_ptr) 0);
    mpfi_get_left(left_endpoint, left);
    mpfi_get_right(right_endpoint, left);
    mpfi_get_left(left_other, right);
    mpfi_get_right(right_other, right);
    if (choose_lower) {
        mpfr_min(left_endpoint, left_endpoint, left_other, MPFR_RNDD);
        mpfr_min(right_endpoint, right_endpoint, right_other, MPFR_RNDU);
    } else {
        mpfr_max(left_endpoint, left_endpoint, left_other, MPFR_RNDD);
        mpfr_max(right_endpoint, right_endpoint, right_other, MPFR_RNDU);
    }
    mpfi_interv_fr(output, left_endpoint, right_endpoint);
    mpfr_clears(left_endpoint, right_endpoint, left_other, right_other, (mpfr_ptr) 0);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_min(mpfi_ptr output, mpfi_srcptr left, mpfi_srcptr right)
{
    return lc_mpfi_endpoint_select(output, left, right, 1);
}

lc_mpfi_status
lc_mpfi_max(mpfi_ptr output, mpfi_srcptr left, mpfi_srcptr right)
{
    return lc_mpfi_endpoint_select(output, left, right, 0);
}

lc_mpfi_status
lc_mpfi_root3(mpfi_ptr output, mpfi_srcptr input)
{
    if (!mpfi_is_nonneg(input)) {
        return LC_MPFI_DOMAIN_UNPROVEN;
    }
    mpfi_cbrt(output, input);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_sqrt(mpfi_ptr output, mpfi_srcptr input)
{
    if (!mpfi_is_nonneg(input)) {
        return LC_MPFI_DOMAIN_UNPROVEN;
    }
    mpfi_sqrt(output, input);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_exp(mpfi_ptr output, mpfi_srcptr input)
{
    mpfi_exp(output, input);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_log(mpfi_ptr output, mpfi_srcptr input)
{
    if (!mpfi_is_pos(input)) {
        return LC_MPFI_DOMAIN_UNPROVEN;
    }
    mpfi_log(output, input);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_sin(mpfi_ptr output, mpfi_srcptr input)
{
    mpfi_sin(output, input);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_cos(mpfi_ptr output, mpfi_srcptr input)
{
    mpfi_cos(output, input);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_abs(mpfi_ptr output, mpfi_srcptr input)
{
    mpfi_abs(output, input);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_sign(mpfi_ptr output, mpfi_srcptr input)
{
    if (mpfi_is_strictly_neg(input)) {
        mpfi_set_si(output, -1);
    } else if (mpfi_is_strictly_pos(input)) {
        mpfi_set_si(output, 1);
    } else if (mpfi_is_zero(input)) {
        mpfi_set_si(output, 0);
    } else {
        mpfi_interv_si(output, -1, 1);
    }
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_pow_pos(mpfi_ptr output, mpfi_srcptr base, mpfi_srcptr exponent)
{
    mpfi_t logarithm;

    if (!mpfi_is_pos(base)) {
        return LC_MPFI_DOMAIN_UNPROVEN;
    }
    mpfi_init2(logarithm, mpfi_get_prec(output));
    mpfi_log(logarithm, base);
    mpfi_mul(logarithm, logarithm, exponent);
    mpfi_exp(output, logarithm);
    mpfi_clear(logarithm);
    return LC_MPFI_OK;
}

lc_mpfi_status
lc_mpfi_pow_nn(mpfi_ptr output, mpfi_srcptr base, mpfi_srcptr exponent)
{
    if (mpfi_is_zero(base) && mpfi_is_pos(exponent)) {
        mpfi_set_ui(output, 0);
        return LC_MPFI_OK;
    }
    return lc_mpfi_pow_pos(output, base, exponent);
}

lc_mpfi_status
lc_mpfi_ratio0(
    mpfi_ptr output,
    mpfi_srcptr numerator,
    mpfi_srcptr denominator
)
{
    if (mpfi_is_zero(numerator) && mpfi_is_zero(denominator)) {
        mpfi_set_ui(output, 0);
        return LC_MPFI_OK;
    }
    if (!mpfi_is_pos(denominator)) {
        return LC_MPFI_DOMAIN_UNPROVEN;
    }
    mpfi_div(output, numerator, denominator);
    return LC_MPFI_OK;
}
