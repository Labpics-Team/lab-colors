"""Reference J_HK (Hellwig-2022 H-K-corrected lightness) via colour-science.

Reproduces the twelve golden anchors pinned in
`crates/labcolors-core/src/lpc.rs::tests::j_hk_matches_hellwig_reference`.
This is the script that comment refers to (previously described only as
"archived alongside this commit").

Pipeline, matching `lpc::j_hk_from_xyz` exactly:
    XYZ = sRGB(IEC 61966-2-1) -> CIECAM16 (XYZ_w = D65*100, L_A = 64, Y_b = 20,
          surround = Average, discount_illuminant = False)  ->  J, M, h
    F_L = CIECAM16 luminance-adaptation factor for L_A = 64
    C   = M / F_L**0.25                        (crate: lpc.rs `chroma`)
    f(h)= -0.160 cos h + 0.132 cos 2h - 0.405 sin h + 0.080 sin 2h + 0.792
    J_HK = J + f(h) * C**0.587                 (HK_CHROMA_EXPONENT = 0.587)

Run:  py -3 scripts/jhk_golden_ref.py
Needs: colour-science (developed against 0.4.7).
"""
import numpy as np
import colour
from colour.appearance import XYZ_to_CIECAM16, VIEWING_CONDITIONS_CIECAM16

# White point: 4-digit chromaticity (0.3127, 0.3290) of IEC 61966-2-1, the same
# white the crate's D65_WHITE is derived from (NOT the tabulated CIE 015 D65).
XYZ_W = colour.xy_to_XYZ(np.array([0.3127, 0.3290])) * 100.0
L_A = 64.0
Y_B = 20.0
HK_CHROMA_EXPONENT = 0.587

# Golden anchors pinned in lpc.rs (hex -> expected J_HK). Keep in lock-step with
# the Rust test; a mismatch here means the reference itself moved.
ANCHORS = [
    ("#0000FF", 38.949467),
    ("#FF0000", 56.023889),
    ("#FFD700", 85.095269),
    ("#00FF00", 88.930558),
    ("#34C759", 68.618093),
    ("#00FFFF", 98.343680),
    ("#008B8B", 51.238150),
    ("#FF00FF", 68.208430),
    ("#C71585", 48.391467),
    ("#FF9500", 68.405244),
    ("#FF7F00", 64.718227),
    ("#007AFF", 56.061369),
]


def f_l(l_a: float) -> float:
    """CIECAM16 luminance-adaptation factor F_L (matches vc.rs `fl`)."""
    k = 1.0 / (5.0 * l_a + 1.0)
    k4 = k ** 4
    return k4 * l_a + 0.1 * (1.0 - k4) ** 2 * (5.0 * l_a) ** (1.0 / 3.0)


def hk_coeff(h_deg: float) -> float:
    """Hue-dependent H-K coefficient f(h) (matches lpc.rs `hk_coeff`)."""
    h = np.radians(h_deg)
    return (
        -0.160 * np.cos(h)
        + 0.132 * np.cos(2.0 * h)
        - 0.405 * np.sin(h)
        + 0.080 * np.sin(2.0 * h)
        + 0.792
    )


def hex_to_xyz100(h: str) -> np.ndarray:
    rgb = np.array([int(h[i:i + 2], 16) / 255.0 for i in (1, 3, 5)])
    return colour.sRGB_to_XYZ(rgb) * 100.0  # decodes gamma, D65, domain [0, 1]


def main() -> int:
    surround = VIEWING_CONDITIONS_CIECAM16["Average"]
    fl = f_l(L_A)
    fl_qrt = fl ** 0.25

    print(f"# XYZ_w = {XYZ_W.tolist()}")
    print(f"# L_A = {L_A}, Y_b = {Y_B}, surround = Average, F_L = {fl:.10f}")
    print(f"# {'hex':<9} {'J':>10} {'M':>10} {'h':>9} {'C':>10} "
          f"{'J_HK':>11} {'expected':>11} {'delta':>10}")

    worst = 0.0
    for hex_code, expected in ANCHORS:
        xyz = hex_to_xyz100(hex_code)
        spec = XYZ_to_CIECAM16(xyz, XYZ_W, L_A, Y_B, surround,
                               discount_illuminant=False)
        j, m, h = float(spec.J), float(spec.M), float(spec.h)
        c = m / fl_qrt
        j_hk = j + hk_coeff(h) * c ** HK_CHROMA_EXPONENT
        delta = abs(j_hk - expected)
        worst = max(worst, delta)
        print(f"  {hex_code:<9} {j:>10.6f} {m:>10.6f} {h:>9.4f} {c:>10.6f} "
              f"{j_hk:>11.6f} {expected:>11.6f} {delta:>10.2e}")

    # 0.05 Lc is the Rust test's own budget: the documented sRGB-matrix / F_L
    # micro-delta band (|dJ| < 0.005, |dC| < 0.05). This pure colour-science
    # pipeline crosses the sRGB-matrix boundary relative to the crate, so it
    # agrees with the pins to ~0.006 Lc — well inside the 0.05 band.
    tol = 0.05
    status = "PASS" if worst < tol else "FAIL"
    print(f"# worst |J_HK - expected| = {worst:.3e}  (tol {tol:.0e})  -> {status}")
    return 0 if worst < tol else 1


if __name__ == "__main__":
    raise SystemExit(main())
