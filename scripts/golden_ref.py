"""Reference CIECAM16 J,M,h values via colour-science for lab-colors cross-validation.

VC matches lab-colors vc.rs intent: D65, L_A=64, Y_b=20,
surround Average (F=1.0, c=0.69, Nc=1.0) and Dim (F=0.9, c=0.59, Nc=0.9),
discount_illuminant=False.
"""
import json
import numpy as np
import colour
from colour.appearance import XYZ_to_CIECAM16, VIEWING_CONDITIONS_CIECAM16

HEXES = ["#FF0000", "#00FF00", "#0000FF", "#FFFFFF", "#808080", "#787880",
         "#007AFF", "#FFD700", "#34C759", "#101012", "#FF9500", "#5856D6"]

XYZ_W = colour.xy_to_XYZ(np.array([0.3127, 0.3290])) * 100.0
L_A = 64.0
Y_B = 20.0

def hex_to_xyz100(h):
    rgb = np.array([int(h[i:i+2], 16) / 255.0 for i in (1, 3, 5)])
    xyz = colour.sRGB_to_XYZ(rgb)  # decodes gamma, D65, domain [0,1]
    return xyz * 100.0

out = {}
for vc_name, surround_key in (("avg", "Average"), ("dim", "Dim")):
    surround = VIEWING_CONDITIONS_CIECAM16[surround_key]
    out[vc_name] = {}
    for h in HEXES:
        xyz = hex_to_xyz100(h)
        spec = XYZ_to_CIECAM16(xyz, XYZ_W, L_A, Y_B, surround,
                               discount_illuminant=False)
        out[vc_name][h] = {"J": float(spec.J), "M": float(spec.M),
                           "h": float(spec.h)}

print(json.dumps(out, indent=1))
print("# whitepoint XYZ_w =", XYZ_W.tolist(), flush=True)
