#ifndef LABCOLOR_ARB_INTERVAL_H
#define LABCOLOR_ARB_INTERVAL_H

#include <stdint.h>

#include <flint/arb.h>
#include <flint/fmpz.h>

typedef enum {
    LC_OK = 0,
    LC_DOMAIN_UNPROVEN = 1,
    LC_INVALID_DYADIC = 2
} lc_status;

lc_status lc_set_dyadic_bits(arb_t output, uint64_t bits);
void lc_interval_get_dyadic_bounds(
    fmpz_t lower,
    fmpz_t upper,
    fmpz_t exponent,
    arb_srcptr value
);

lc_status lc_add(arb_t output, arb_srcptr left, arb_srcptr right, slong precision);
lc_status lc_sub(arb_t output, arb_srcptr left, arb_srcptr right, slong precision);
lc_status lc_mul(arb_t output, arb_srcptr left, arb_srcptr right, slong precision);
lc_status lc_div(arb_t output, arb_srcptr left, arb_srcptr right, slong precision);
lc_status lc_min(arb_t output, arb_srcptr left, arb_srcptr right, slong precision);
lc_status lc_max(arb_t output, arb_srcptr left, arb_srcptr right, slong precision);
lc_status lc_root3(arb_t output, arb_srcptr input, slong precision);
lc_status lc_sqrt(arb_t output, arb_srcptr input, slong precision);
lc_status lc_exp(arb_t output, arb_srcptr input, slong precision);
lc_status lc_log(arb_t output, arb_srcptr input, slong precision);
lc_status lc_sin(arb_t output, arb_srcptr input, slong precision);
lc_status lc_cos(arb_t output, arb_srcptr input, slong precision);
lc_status lc_abs(arb_t output, arb_srcptr input, slong precision);
lc_status lc_sign(arb_t output, arb_srcptr input, slong precision);
lc_status lc_pow_pos(arb_t output, arb_srcptr base, arb_srcptr exponent, slong precision);
lc_status lc_pow_nn(arb_t output, arb_srcptr base, arb_srcptr exponent, slong precision);
lc_status lc_ratio0(arb_t output, arb_srcptr numerator, arb_srcptr denominator, slong precision);

#endif
