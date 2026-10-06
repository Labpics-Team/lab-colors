//! Настоящее равенство observation: координат недостаточно без identity backing.
//! Пустой payload допустим только для этой проверки: predicate его не читает.
//! Входы stream/revision полные; alias и независимый backing создаёт настоящий pool.
use super::*;

#[kani::proof]
#[kani::unwind(5)]
fn equality_requires_the_same_observation_allocation() {
    let schema = CanonicalObservationSchemaV1(Rc::from([SurfaceInputPortId::new(1)]));
    let mut pool = ObservationArenaPoolV1::new(&schema);
    let first_backing = pool.materialize_into(|_| Ok(())).unwrap();
    let other_backing = pool.materialize_into(|_| Ok(())).unwrap();
    let same_allocation: bool = kani::any();
    let first_stream: u32 = kani::any();
    let second_stream: u32 = kani::any();
    let first_revision: u64 = kani::any();
    let second_revision: u64 = kani::any();
    let first = RevisionBoundObservationV1 {
        stream: ObservationStreamId::new(first_stream),
        revision: Revision::new(first_revision),
        backing: first_backing.clone(),
    };
    let second = RevisionBoundObservationV1 {
        stream: ObservationStreamId::new(second_stream),
        revision: Revision::new(second_revision),
        backing: if same_allocation {
            first_backing
        } else {
            other_backing
        },
    };
    let coordinates_match = first_stream == second_stream && first_revision == second_revision;
    let expected = same_allocation && coordinates_match;
    assert!(
        first.is_same_binding_as(&second) == expected,
        "observation equality must bind stream revision and allocation identity"
    );
    kani::cover!(
        same_allocation && coordinates_match,
        "same observation accepted"
    );
    kani::cover!(
        !same_allocation && coordinates_match,
        "separate backing rejected despite equal coordinates"
    );
    kani::cover!(
        same_allocation && first_stream != second_stream,
        "foreign stream rejected"
    );
    kani::cover!(
        same_allocation && first_revision != second_revision,
        "foreign revision rejected"
    );
}
