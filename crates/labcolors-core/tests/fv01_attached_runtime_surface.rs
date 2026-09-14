//! RED contract for the public attached Program runtime.
//!
//! FV-01 must expose one path from canonical Program wire to the existing Core
//! attachment transaction. The test intentionally names no detached snapshot
//! API as an authority constructor.

use labcolors_core::program_wire::{
    AttachedPointSinkHostIntentV1, AttachedPointSinkHostV1, AttachedProgramCompileErrorV1,
    AttachedProgramEmissionBindingV1, AttachedProgramPresentationBindingV1,
    AttachedProgramScenarioV1, CompiledAttachedProgramV1, compile_attached_program_wire_v1,
};

struct Host;

impl AttachedPointSinkHostV1 for Host {
    type Error = ();

    fn try_install(
        &mut self,
        _intent: AttachedPointSinkHostIntentV1<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn canonical_wire_has_a_distinct_attached_compile_seam() {
    let _ = compile_attached_program_wire_v1
        as fn(&[u8]) -> Result<CompiledAttachedProgramV1, AttachedProgramCompileErrorV1>;
}

#[test]
fn attachment_bindings_and_observations_are_typed_public_inputs() {
    let emission = AttachedProgramEmissionBindingV1::new(17, 3);
    assert_eq!(emission.output(), 17);
    assert_eq!(emission.sink_output(), 3);

    let presentation = AttachedProgramPresentationBindingV1::new(17, 9, 8);
    assert_eq!(presentation.output(), 17);
    assert_eq!(presentation.root(), 9);
    assert_eq!(presentation.occurrence(), 8);

    let scenario = AttachedProgramScenarioV1::new(7, vec![labcolors_core::Srgb8::new([0x80; 3])]);
    assert_eq!(scenario.id(), 7);
    assert_eq!(scenario.surfaces(), &[labcolors_core::Srgb8::new([0x80; 3])]);
}

#[test]
fn compiled_program_attaches_only_through_the_typed_host_port() {
    fn attach<H: AttachedPointSinkHostV1>(compiled: &CompiledAttachedProgramV1, host: H) {
        let emissions = [AttachedProgramEmissionBindingV1::new(17, 3)];
        let presentations = [AttachedProgramPresentationBindingV1::new(17, 9, 8)];
        let _ = compiled.attach(100, &emissions, &presentations, host);
    }
    let _ = attach::<Host> as fn(&CompiledAttachedProgramV1, Host);
}
