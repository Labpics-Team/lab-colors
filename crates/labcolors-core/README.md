# labcolors-core

Dependency-free compiler and runtime resolver for client-owned colour-token
contracts.

The client owns token identifiers, aliases, hierarchy, component states and
design semantics. The core treats those identifiers as opaque and owns the
mathematics: graph resolution, contrast, compositing, adaptation, finite output
and numerical certificates. Physical colours are contextual results, not the
source schema.

## Exact encoded-sRGB8 example

The declared source-over reference rounds the exact half-tie upward:

```rust
use labcolors_core::alpha::composite_over_srgb8;

# fn main() -> Result<(), String> {
let composite = composite_over_srgb8(
    [0xC0, 0xB2, 0xFA],
    0.122,
    [0x00, 0x00, 0x00],
)?;
assert_eq!(composite, [0x17, 0x16, 0x1F]);
# Ok(())
# }
```

Exact composite guarantees do not extend to an unknown renderer, display,
spatial blur field or the explicit platform-dependent legacy Glow decision.
See the repository migration guide and conformance pack for versioned boundary
details.
