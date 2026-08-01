#ifndef LABCOLOR_MPFI_FORMULA_H
#define LABCOLOR_MPFI_FORMULA_H

#include <stdint.h>

#include <mpfi.h>

#include "interval.h"

lc_mpfi_status lc_mpfi_formula_point(
    mpfi_ptr output,
    const uint8_t rgb[3],
    mpfi_srcptr context,
    uint8_t surround
);
lc_mpfi_status lc_mpfi_formula_segment(mpfi_ptr output, mpfi_srcptr input);
lc_mpfi_status lc_mpfi_formula_singleton(mpfi_ptr output, mpfi_srcptr input);

#endif
