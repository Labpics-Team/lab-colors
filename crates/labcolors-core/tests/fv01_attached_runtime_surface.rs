//! Public contract sentinel for the attached Program runtime.
//!
//! FV-01 exposes one path from canonical Program wire to the existing Core
//! attachment transaction. No detached snapshot, Paint value, sRGB8 value, or
//! CSS string appears as an authority constructor.

use labcolors_core::program_wire::{
    AttachedPointSinkHostIntentV1, AttachedPointSinkHostV1, AttachedProgramCompileErrorV1,
    AttachedProgramEmissionBindingV1, AttachedProgramPresentationBindingV1,
    AttachedProgramUpdateStateV1, AttachedProgramV1, CompiledAttachedProgramV1, ProgramScenarioV1,
    compile_attached_program_wire_v1,
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
fn attachment_bindings_reuse_the_canonical_public_scenario_dto() {
    let emission = AttachedProgramEmissionBindingV1::new(17, 3);
    assert_eq!(emission.output(), 17);
    assert_eq!(emission.sink_output(), 3);

    let presentation = AttachedProgramPresentationBindingV1::new(17, 9, 8);
    assert_eq!(presentation.output(), 17);
    assert_eq!(presentation.root(), 9);
    assert_eq!(presentation.occurrence(), 8);

    let scenario = ProgramScenarioV1::new(7, vec![labcolors_core::Srgb8::new([0x80; 3])]);
    assert_eq!(scenario.id(), 7);
    assert_eq!(
        scenario.surfaces(),
        &[labcolors_core::Srgb8::new([0x80; 3])]
    );
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

#[test]
fn live_attachment_exposes_post_commit_lifecycle_and_authority_only() {
    fn drive<H: AttachedPointSinkHostV1>(
        runtime: &mut AttachedProgramV1<H>,
        scenario: &ProgramScenarioV1,
    ) {
        if let Ok(update) = runtime.update_observed(1, core::slice::from_ref(scenario)) {
            let _ = update.state();
            let _ = update.authorities();
        }
        let _ = runtime.update_unknown(2, 7);
    }

    let _ = drive::<Host> as fn(&mut AttachedProgramV1<Host>, &ProgramScenarioV1);
    let _ = AttachedProgramUpdateStateV1::Waiting;
    let _ = AttachedProgramUpdateStateV1::Ready;
    let _ = AttachedProgramUpdateStateV1::Stale;
    let _ = AttachedProgramUpdateStateV1::Failed;
}
