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

## Bounded WCAG 2.2 feasibility

`wcag22_feasibility` is a complete finite-domain check, not a colour selector or
ranker. The client declares opaque relation and occurrence IDs, the applicable
WCAG 2.2 criterion and exact adjacent sRGB8 colours. Core canonicalizes those
declarations, evaluates every candidate/adjacency cell and returns one sealed
terminal: `Feasible`, `Infeasible` or declaration-only `NotEvaluated`.
`Infeasible` means that this selected registered domain has no feasible member;
it does not claim that no colour exists outside that domain.

Version 1 registers the 256-member encoded-sRGB8 neutral axis. It does not infer
text size, component semantics or applicability from an ID. Resource excess,
contradictory declarations, allocation failure and evaluator/compiler invariant
failure are typed errors; there is no partial result or fallback.

```rust
use labcolors_core::{
    Srgb8,
    wcag22::Wcag22CriterionV1,
    wcag22_feasibility::{
        DomainIdV1, OccurrenceId, RelationId, RelationV1, RequestV1,
        ResourceProfileIdV1, evaluate,
    },
};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let relation = RelationV1::applicable(
    RelationId::try_new("label-on-surface")?,
    OccurrenceId::try_new("button/label")?,
    Wcag22CriterionV1::Sc143TextDefault,
    vec![Srgb8::new([0; 3]), Srgb8::new([255; 3])],
)?;
let request = RequestV1::try_new(
    DomainIdV1::Srgb8NeutralAxis,
    vec![relation],
    ResourceProfileIdV1::Compile,
)?;

let result = evaluate(request)?;
assert!(result.is_feasible());
if let Some(evaluated) = result.evaluated() {
    let candidates: Vec<_> = evaluated
        .feasible_candidates()
        .map(|candidate| candidate.bytes())
        .collect();
    assert_eq!(candidates, vec![[0x75; 3], [0x76; 3]]);
}
# Ok(())
# }
```
