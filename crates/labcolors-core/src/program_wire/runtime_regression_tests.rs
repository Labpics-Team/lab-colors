use super::*;
use crate::program::wire::ProgramWireBuilderV1;

fn runtime_wire() -> Vec<u8> {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .source(1, crate::Srgb8::new([0x40, 0x40, 0x40]))
        .fixed_target(2, 1)
        .surface_input_port(6)
        .opacity_input(5, 0.5)
        .solid_paint(3, 2)
        .opacity_paint(4, 3, 5)
        .input_surface(7, 6)
        .source_over_occurrence(8, 4, 7, 64.0, 0.2, 2)
        .presentation_root(9, 8)
        .presentation_target(9, 8)
        .exact_visible_unary(true, 10, 8, crate::Srgb8::new([0x60, 0x60, 0x60]))
        .output(17, 4);
    builder.finish().unwrap()
}

#[test]
fn stale_snapshot_retains_diagnostics_without_publishing_ready_outputs() {
    let compiled = compile_program_wire_v1(&runtime_wire()).unwrap();
    let mut session = compiled.instantiate(100).unwrap();
    let ready = session
        .update_observed(
            1,
            &[ProgramScenarioV1::new(
                7,
                vec![crate::Srgb8::new([0x80, 0x80, 0x80])],
            )],
        )
        .unwrap();
    assert_eq!(ready.state(), ProgramSnapshotStateV1::Ready);
    assert_eq!(ready.outputs().len(), 1);

    let stale = session.update_unknown(2, 9).unwrap();
    assert_eq!(stale.state(), ProgramSnapshotStateV1::Stale);
    assert!(
        stale.outputs().is_empty(),
        "stale diagnostic certificates must not republish Ready outputs"
    );
}
