# Material colour and output precision

Colors keeps a role's source and the appearance of each use separate. An
opaque use emits its resolved RGB. A translucent use emits resolved RGB and
its authored alpha; its surface remains part of the composition.

The reference appearance is encoded-sRGB source-over, rounded to an RGB8
point after each layer. Hue, chroma, neutrality, tone preference and
`maxChange` refer to that appearance. The output path also carries the
numeric precision of rendering the material. Requirements such as text
contrast and surface separation cover this range before a frame is accepted.
The normal `colors()` API applies both parts automatically.

## Why a range is necessary

An interior pixel in the browser can differ from the reference by a channel
unit because alpha and premultiplied RGB are rounded at different stages.
For example, RGB `[246,107,57]` at alpha `.72` over `[107,127,114]` has reference
appearance `[207,113,73]`. Edge 158 rendered `[207,112,73]`. Ink `[36,36,36]`
passes the text threshold on the reference and falls below it on that pixel.
Changing from `rgba()` to `color(srgb …)` did not remove the discrepancy.

The solver therefore chooses ink and material together with enough room
for numeric emission. It keeps the material's alpha and can adjust the ink;
it does not replace the material with a precomposited opaque colour.

## Transfer through layers

The package owns one allowance for the normal encoded-byte raster path:

* An opaque input surface starts with radius zero.
* Opaque paint replaces the backdrop and resets its visible radius to zero.
* Fully transparent paint preserves the backdrop, including its radius and
  the fact that both pixels are the same.
* For other alpha values, a layer carries the backdrop error and adds a
  local allowance of three encoded channel units.

For an integer backdrop radius `r` and alpha `a`, the next radius is
`floor((1-a)*r+3)`, capped at 255. The implementation computes the equivalent
`r+3-ceil(a*r)` with the product rounded outward. A deep chain accumulates
its allowance; it does not reuse a fixed ±1 for the whole scene.

The local bound follows the published nearest-alpha, nearest-premultiplied
and integer-residual paths in Skia. Premultiplication contributes at most
0.5 channel unit; the shared alpha contributes at most 0.5; the residual
shortcuts contribute less than 1; rounding the reference contributes at
most 0.5. Three units cover their sum and floating arithmetic. Independent
integer checks cover all 65,536 byte-alpha/backdrop pairs for each residual
law. The observed browser patches provide an additional check of this path.

This is the package's encoded-sRGB8 material contract. Other output spaces
or extra compositing operations need their own corresponding transfer law.

## How requirements use it

WCAG checks the closest luminance endpoints after establishing one
foreground/background orientation for the whole range. Overlapping
luminance ranges cannot pass a positive contrast threshold. The threshold
decision uses the same exact WCAG kernel as ordinary opaque colours.

Surface separation uses Oklab bounds for both RGB ranges. Its report keeps
the reference `distance` and also exposes `distanceBounds`; both bounds of
the authored requirement must hold. A fully transparent use has separation
zero from its backdrop, even when that backdrop itself has a range.

Role character stays a reference-colour intention. A neutral translucent
use can compensate for a coloured surface; numeric emission does not turn
that intention into an impossible demand for identical RGB channels in
every renderer intermediate.

The V3 `pointOccurrence` encoder selects this material contract. The explicit
V1/V2 `sourceOverOccurrence` retains its mathematical point contract. Their
program identities differ. A mixed graph carries inherited uncertainty
through reference-only nodes until an opaque paint actually replaces it.

## Sources

* [W3C source-over composition](https://www.w3.org/TR/compositing-1/)
* [Skia uniform colour quantization](https://skia.googlesource.com/skia/+/eec27f3c3b3728be278d3945b78502d321ed4a8c/src/core/SkRasterPipeline.cpp)
* [Skia low- and high-precision raster operations](https://skia.googlesource.com/skia/+/8c87efbae9c59ec2b8e799f0088614c76097c4ec/src/opts/SkRasterPipeline_opts.h)
* [Skia byte alpha and packed source-over](https://skia.googlesource.com/skia/+/c6e63919c318/src/core/SkColorPriv.h)
