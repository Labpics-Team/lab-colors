#ifndef LABCOLOR_MPFI_INTERVAL_H
#define LABCOLOR_MPFI_INTERVAL_H

#include <stdint.h>

#include <mpfi.h>

typedef enum {
    LC_MPFI_OK = 0,
    LC_MPFI_DOMAIN_UNPROVEN = 1,
    LC_MPFI_INVALID_DYADIC = 2
} lc_mpfi_status;

lc_mpfi_status lc_mpfi_set_dyadic_bits(mpfi_ptr output, uint64_t bits);

lc_mpfi_status lc_mpfi_add(
    mpfi_ptr output,
    mpfi_srcptr left,
    mpfi_srcptr right
);
lc_mpfi_status lc_mpfi_sub(
    mpfi_ptr output,
    mpfi_srcptr left,
    mpfi_srcptr right
);
lc_mpfi_status lc_mpfi_mul(
    mpfi_ptr output,
    mpfi_srcptr left,
    mpfi_srcptr right
);
lc_mpfi_status lc_mpfi_div(
    mpfi_ptr output,
    mpfi_srcptr left,
    mpfi_srcptr right
);
lc_mpfi_status lc_mpfi_min(
    mpfi_ptr output,
    mpfi_srcptr left,
    mpfi_srcptr right
);
lc_mpfi_status lc_mpfi_max(
    mpfi_ptr output,
    mpfi_srcptr left,
    mpfi_srcptr right
);
lc_mpfi_status lc_mpfi_root3(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_sqrt(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_exp(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_log(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_sin(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_cos(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_abs(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_sign(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_pow_pos(
    mpfi_ptr output,
    mpfi_srcptr base,
    mpfi_srcptr exponent
);
lc_mpfi_status lc_mpfi_pow_nn(
    mpfi_ptr output,
    mpfi_srcptr base,
    mpfi_srcptr exponent
);
lc_mpfi_status lc_mpfi_ratio0(
    mpfi_ptr output,
    mpfi_srcptr numerator,
    mpfi_srcptr denominator
);

#endif
