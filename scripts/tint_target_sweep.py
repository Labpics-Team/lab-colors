#!/usr/bin/env python3
"""Reproduce the RMS sweep that pins TINT_TARGET_MP = 6.1 (semantic.rs).

Value-preserving provenance script (changes NO shipped value). It recomputes, from
first principles, why 6.1 is the RMS-minimising constant CAM16-UCS M' target of the
tint-identity curve against the owner's reference neutral ramp — the same computation
the in-code VALIDATION test `curve_fits_reference_plateau_colorfulness` performs,
lifted out of `#[cfg(test)]` so the sweep is reproducible without the Rust engine.

Method (mirrors semantic.rs `node_measure` / `curve_measure`):
  * REFERENCE_NODES — the 12 mid-to-light nodes of the owner reference ramp
    (`REFERENCE_NODES` in semantic.rs; pure #FFFFFF is dropped as achromatic).
    The reference is a YARDSTICK, never an input.
  * For each node: Oklab lightness L (Ottosson 2020, from linear sRGB) and CAM16-UCS
    colourfulness M'.
  * Plateau = nodes with Oklab L in [0.45, 0.90], where the gamut has room and the
    constant-M' policy holds, so the curve's realised M' == the target t. (The two
    ends release colourfulness by hand in the reference and are excluded — see the
    test's docstring.)
  * Residual(t, node) = |t - M'_node| on the plateau; sweep t and report the argmin
    of the RMS residual. Because the target is a constant, the argmin equals the
    arithmetic mean of the plateau M' values.

Viewing conditions match `ViewingConditions::srgb()`: D65, L_A=64, Y_b=20, surround
Average (F=1.0, c=0.69, N_c=1.0), discount_illuminant=False — identical to
scripts/golden_ref.py.

CAM16-UCS M' compression: M' = (1/0.0228)·ln(1 + 0.0228·M) (Li et al. 2017;
Luo et al. 2006 UCS coefficients).

Run:  pip install colour-science numpy  &&  python scripts/tint_target_sweep.py
Expected: swept argmin ~= 6.1, plateau RMS residual at 6.1 ~= 0.90 M' (2026-06-12).
"""
import math

import numpy as np
import colour
from colour.appearance import XYZ_to_CIECAM16, VIEWING_CONDITIONS_CIECAM16

# The 12 mid-to-light reference nodes (semantic.rs REFERENCE_NODES; #FFFFFF dropped).
REFERENCE_NODES = [
    "#101012", "#151518", "#212125", "#303136", "#44444B", "#5B5C64",
    "#787881", "#9698A2", "#B3B5BF", "#CDD0D9", "#E4E7ED", "#F6F8FA",
]

PLATEAU_L_MIN, PLATEAU_L_MAX = 0.45, 0.90  # Oklab L window (semantic.rs test).
UCS_C2 = 0.0228  # CAM16-UCS colourfulness compression coefficient.

XYZ_W = colour.xy_to_XYZ(np.array([0.3127, 0.3290])) * 100.0
L_A, Y_B = 64.0, 20.0
SURROUND = VIEWING_CONDITIONS_CIECAM16["Average"]


def _srgb_eotf(c):
    # sRGB encoded [0, 1] -> linear (matches srgb_gamma_inv in spaces/srgb.rs).
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def _channels(h):
    return [int(h[i:i + 2], 16) / 255.0 for i in (1, 3, 5)]


def oklab_l(hex_str):
    r, g, b = (_srgb_eotf(c) for c in _channels(hex_str))
    l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b
    m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b
    s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b
    l_, m_, s_ = np.cbrt(l), np.cbrt(m), np.cbrt(s)
    return 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_


def cam16_ucs_mp(hex_str):
    rgb = np.array(_channels(hex_str))
    xyz = colour.sRGB_to_XYZ(rgb) * 100.0
    spec = XYZ_to_CIECAM16(xyz, XYZ_W, L_A, Y_B, SURROUND, discount_illuminant=False)
    m = float(spec.M)
    return (1.0 / UCS_C2) * math.log(1.0 + UCS_C2 * m)


def main():
    plateau = []
    print("node       Oklab-L   M'       on-plateau")
    for h in REFERENCE_NODES:
        l = float(oklab_l(h))
        mp = cam16_ucs_mp(h)
        on = PLATEAU_L_MIN <= l <= PLATEAU_L_MAX
        print(f"{h}   {l:7.4f}   {mp:6.3f}   {'yes' if on else 'no'}")
        if on:
            plateau.append(mp)

    plateau = np.array(plateau)
    # Sweep candidate constant targets; RMS residual against plateau node M'.
    ts = np.arange(4.0, 8.0 + 1e-9, 0.001)
    rms = np.array([math.sqrt(float(np.mean((t - plateau) ** 2))) for t in ts])
    t_star = float(ts[int(np.argmin(rms))])

    print(f"\nplateau nodes: {len(plateau)}")
    print(f"closed-form optimum (mean of plateau M'): {plateau.mean():.4f}")
    print(f"swept argmin target t*:                   {t_star:.3f}")
    print(f"RMS residual at t=6.1:                    "
          f"{math.sqrt(float(np.mean((6.1 - plateau) ** 2))):.3f}")
    print(f"max |6.1 - M'| on plateau:                "
          f"{float(np.max(np.abs(6.1 - plateau))):.3f}")


if __name__ == "__main__":
    main()
