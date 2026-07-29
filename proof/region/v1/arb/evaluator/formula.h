#ifndef LABCOLOR_ARB_FORMULA_H
#define LABCOLOR_ARB_FORMULA_H

#include <stdint.h>

#include "interval.h"

lc_status lc_formula_point(
    arb_ptr output,
    const uint8_t rgb[3],
    arb_srcptr context,
    uint8_t surround,
    slong precision
);
lc_status lc_formula_segment(arb_t output, arb_srcptr input, slong precision);
lc_status lc_formula_singleton(arb_t output, arb_srcptr input, slong precision);

#endif
