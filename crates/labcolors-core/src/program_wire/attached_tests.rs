use std::cell::RefCell;
use std::rc::Rc;

use crate::Srgb8;
use crate::program::attachment::fv01::fail_next_attached_authority_reservation_for_test;
use crate::program::wire::ProgramWireBuilderV1;

use super::{
    AppearanceSurroundV1, AttachedMaterializationAuthorityErrorV1, AttachedPointSinkErrorV1,
    AttachedPointSinkHostIntentV1, AttachedPointSinkHostV1, AttachedProgramEmissionBindingV1,
    AttachedProgramPresentationBindingV1, AttachedProgramUpdateErrorV1,
    AttachedProgramUpdateStateV1, ProgramScenarioV1, RendererProvenanceV1,
    compile_attached_program_wire_v1,
};

const OUTPUT: u32 = 17;
const SINK_OUTPUT: u32 = 3;
const PRESENTATION_ROOT: u32 = 9;
const OCCURRENCE: u32 = 8;
const SOURCE: Srgb8 = Srgb8::new([0x40; 3]);
const BACKDROP: Srgb8 = Srgb8::new([0x80; 3]);
const COMPOSITE: Srgb8 = Srgb8::new([0x60; 3]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostFailure {
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PatchEntry {
    output: u32,
    sink_output: u32,
    source: Srgb8,
    opacity: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum HostEvent {
    SetAll {
        revision: u64,
        binding_epoch: u64,
        expected_sequence: u64,
        desired_sequence: u64,
        patch: Vec<PatchEntry>,
    },
    RevokeAll {
        revision: u64,
        binding_epoch: u64,
        expected_sequence: u64,
        desired_sequence: u64,
    },
    ConfirmExact {
        revision: u64,
        binding_epoch: u64,
        published_sequence: u64,
        patch: Vec<PatchEntry>,
    },
}

#[derive(Default)]
struct ProbeState {
    events: Vec<HostEvent>,
    reject_next: bool,
}

#[derive(Clone, Default)]
struct HostProbe(Rc<RefCell<ProbeState>>);

impl HostProbe {
    fn host(&self) -> Host {
        Host {
            probe: self.clone(),
        }
    }

    fn reject_next(&self) {
        self.0.borrow_mut().reject_next = true;
    }
}

struct Host {
    probe: HostProbe,
}

impl AttachedPointSinkHostV1 for Host {
    type Error = HostFailure;

    fn try_install(
        &mut self,
        intent: AttachedPointSinkHostIntentV1<'_>,
    ) -> Result<(), Self::Error> {
        let mut state = self.probe.0.borrow_mut();
        if state.reject_next {
            state.reject_next = false;
            return Err(HostFailure::Rejected);
        }

        let copy_patch = |patch: &[super::AttachedPointSinkHostPatchEntryV1]| {
            patch
                .iter()
                .copied()
                .map(|entry| PatchEntry {
                    output: entry.output(),
                    sink_output: entry.sink_output(),
                    source: entry.source(),
                    opacity: entry.opacity(),
                })
                .collect()
        };
        let event = match intent {
            AttachedPointSinkHostIntentV1::SetAll {
                revision,
                binding_epoch,
                expected_sequence,
                desired_sequence,
                patch,
            } => HostEvent::SetAll {
                revision,
                binding_epoch,
                expected_sequence,
                desired_sequence,
                patch: copy_patch(patch),
            },
            AttachedPointSinkHostIntentV1::RevokeAll {
                revision,
                binding_epoch,
                expected_sequence,
                desired_sequence,
            } => HostEvent::RevokeAll {
                revision,
                binding_epoch,
                expected_sequence,
                desired_sequence,
            },
            AttachedPointSinkHostIntentV1::ConfirmExact {
                revision,
                binding_epoch,
                published_sequence,
                patch,
            } => HostEvent::ConfirmExact {
                revision,
                binding_epoch,
                published_sequence,
                patch: copy_patch(patch),
            },
        };
        state.events.push(event);
        Ok(())
    }
}

fn program_wire(adapting_luminance: f64) -> Vec<u8> {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .source(1, SOURCE)
        .fixed_target(2, 1)
        .opacity_input(5, 0.5)
        .solid_paint(3, 2)
        .opacity_paint(4, 3, 5)
        .surface_input_port(6)
        .input_surface(7, 6)
        .source_over_occurrence(OCCURRENCE, 4, 7, adapting_luminance, 0.2, 1)
        .presentation_root(PRESENTATION_ROOT, OCCURRENCE)
        .presentation_target(PRESENTATION_ROOT, OCCURRENCE)
        .exact_visible_unary(true, 10, OCCURRENCE, COMPOSITE)
        .output(OUTPUT, 4);
    match builder.finish() {
        Ok(bytes) => bytes,
        Err(_) => panic!("fixture wire must be representable"),
    }
}

fn bindings() -> (
    [AttachedProgramEmissionBindingV1; 1],
    [AttachedProgramPresentationBindingV1; 1],
) {
    (
        [AttachedProgramEmissionBindingV1::new(OUTPUT, SINK_OUTPUT)],
        [AttachedProgramPresentationBindingV1::new(
            OUTPUT,
            PRESENTATION_ROOT,
            OCCURRENCE,
        )],
    )
}

fn scenario() -> ProgramScenarioV1 {
    ProgramScenarioV1::new(101, vec![BACKDROP])
}

#[test]
fn ready_authority_is_bound_to_the_atomic_host_install_and_revoked_on_unknown() {
    let compiled = match compile_attached_program_wire_v1(&program_wire(64.0)) {
        Ok(value) => value,
        Err(_) => panic!("fixture Program must compile"),
    };
    let content_identity = compiled.content_identity();
    let (emissions, presentations) = bindings();
    let probe = HostProbe::default();
    let mut runtime = match compiled.attach(100, &emissions, &presentations, probe.host()) {
        Ok(value) => value,
        Err(_) => panic!("fixture attachment must bind"),
    };
    let observed = scenario();

    let ready = match runtime.update_observed(1, core::slice::from_ref(&observed)) {
        Ok(value) => value,
        Err(_) => panic!("valid observation must publish"),
    };
    assert_eq!(ready.state(), AttachedProgramUpdateStateV1::Ready);
    assert_eq!(ready.authorities().len(), 1);
    let authority = &ready.authorities()[0];
    assert_eq!(authority.content_identity(), content_identity);
    assert_eq!(authority.published_revision(), 1);
    assert_eq!(authority.sink_sequence(), 1);
    assert_ne!(authority.sink_binding_epoch(), 0);
    assert_eq!(authority.presentation_root(), PRESENTATION_ROOT);
    assert_eq!(authority.presentation_occurrence(), OCCURRENCE);
    assert_eq!(authority.terminal_occurrence(), OCCURRENCE);
    assert_eq!(authority.source(), SOURCE);
    assert_eq!(authority.opacity(), 0.5);
    assert_eq!(authority.case_count(), 1);
    let case = authority.cases().next().expect("one physical case");
    assert_eq!(case.case_index(), 0);
    assert_eq!(case.composite(), COMPOSITE);
    assert_eq!(authority.adapting_luminance_cd_m2(), 64.0);
    assert_eq!(authority.background_luminance_ratio_yb_yw(), 0.2);
    assert_eq!(
        authority.appearance_surround(),
        AppearanceSurroundV1::Average
    );
    assert_eq!(
        authority.renderer_provenance(),
        RendererProvenanceV1::Unverified
    );
    assert_eq!(runtime.validate_authority(authority), Ok(()));

    {
        let state = probe.0.borrow();
        assert_eq!(state.events.len(), 1);
        match &state.events[0] {
            HostEvent::SetAll {
                revision,
                binding_epoch,
                expected_sequence,
                desired_sequence,
                patch,
            } => {
                assert_eq!(*revision, 1);
                assert_eq!(*binding_epoch, authority.sink_binding_epoch());
                assert_eq!((*expected_sequence, *desired_sequence), (0, 1));
                assert_eq!(
                    patch,
                    &[PatchEntry {
                        output: OUTPUT,
                        sink_output: SINK_OUTPUT,
                        source: SOURCE,
                        opacity: 0.5,
                    }]
                );
            }
            other => panic!("first host event must be SetAll, got {other:?}"),
        }
    }

    let stale = match runtime.update_unknown(2, 77) {
        Ok(value) => value,
        Err(_) => panic!("unknown observation must revoke atomically"),
    };
    assert_eq!(stale.state(), AttachedProgramUpdateStateV1::Stale);
    assert!(stale.authorities().is_empty());
    assert_eq!(
        runtime.validate_authority(authority),
        Err(AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch)
    );

    let state = probe.0.borrow();
    assert_eq!(state.events.len(), 2);
    match state.events[1] {
        HostEvent::RevokeAll {
            revision,
            binding_epoch,
            expected_sequence,
            desired_sequence,
        } => {
            assert_eq!(revision, 2);
            assert_eq!(binding_epoch, authority.sink_binding_epoch());
            assert_eq!((expected_sequence, desired_sequence), (1, 2));
        }
        ref other => panic!("second host event must be RevokeAll, got {other:?}"),
    }
}

#[test]
fn authority_reservation_failure_precedes_any_host_install_and_preserves_retry() {
    let compiled = match compile_attached_program_wire_v1(&program_wire(64.0)) {
        Ok(value) => value,
        Err(_) => panic!("fixture Program must compile"),
    };
    let (emissions, presentations) = bindings();
    let probe = HostProbe::default();
    let mut runtime = match compiled.attach(100, &emissions, &presentations, probe.host()) {
        Ok(value) => value,
        Err(_) => panic!("fixture attachment must bind"),
    };
    let observed = scenario();

    {
        let _failure = fail_next_attached_authority_reservation_for_test();
        match runtime.update_observed(1, core::slice::from_ref(&observed)) {
            Err(AttachedProgramUpdateErrorV1::Authority(
                AttachedMaterializationAuthorityErrorV1::ResourceExhausted,
            )) => {}
            _ => panic!("authority reservation failure must remain typed"),
        }
        assert!(
            probe.0.borrow().events.is_empty(),
            "fallible authority preparation must precede every observable host install",
        );
    }

    let retry = match runtime.update_observed(1, core::slice::from_ref(&observed)) {
        Ok(value) => value,
        Err(_) => panic!("same revision must remain retryable after authority preflight failure"),
    };
    assert_eq!(retry.state(), AttachedProgramUpdateStateV1::Ready);
    assert_eq!(retry.authorities().len(), 1);
    assert_eq!(probe.0.borrow().events.len(), 1);
}

#[test]
fn rejected_host_install_preserves_the_previous_live_authority_and_sequence() {
    let compiled = match compile_attached_program_wire_v1(&program_wire(64.0)) {
        Ok(value) => value,
        Err(_) => panic!("fixture Program must compile"),
    };
    let (emissions, presentations) = bindings();
    let probe = HostProbe::default();
    let mut runtime = match compiled.attach(100, &emissions, &presentations, probe.host()) {
        Ok(value) => value,
        Err(_) => panic!("fixture attachment must bind"),
    };
    let observed = scenario();
    let first = match runtime.update_observed(1, core::slice::from_ref(&observed)) {
        Ok(value) => value,
        Err(_) => panic!("first install must succeed"),
    };
    let mut first_authorities = first.into_authorities();
    let old_authority = first_authorities.pop().expect("one authority");
    assert_eq!(runtime.validate_authority(&old_authority), Ok(()));

    probe.reject_next();
    match runtime.update_observed(2, core::slice::from_ref(&observed)) {
        Err(AttachedProgramUpdateErrorV1::SinkInstall(AttachedPointSinkErrorV1::Host(
            HostFailure::Rejected,
        ))) => {}
        _ => panic!("host rejection must remain a typed SinkInstall failure"),
    }
    assert_eq!(runtime.validate_authority(&old_authority), Ok(()));
    assert_eq!(probe.0.borrow().events.len(), 1);

    let retry = match runtime.update_observed(2, core::slice::from_ref(&observed)) {
        Ok(value) => value,
        Err(_) => panic!("same revision must be retryable after rejected install"),
    };
    assert_eq!(retry.state(), AttachedProgramUpdateStateV1::Ready);
    let new_authority = &retry.authorities()[0];
    assert_eq!(
        new_authority.sink_binding_epoch(),
        old_authority.sink_binding_epoch()
    );
    assert_eq!(new_authority.sink_sequence(), 2);
    assert_eq!(
        runtime.validate_authority(&old_authority),
        Err(AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch)
    );
    assert_eq!(runtime.validate_authority(new_authority), Ok(()));

    let state = probe.0.borrow();
    assert_eq!(state.events.len(), 2);
    match &state.events[1] {
        HostEvent::SetAll {
            revision,
            expected_sequence,
            desired_sequence,
            ..
        } => {
            assert_eq!(*revision, 2);
            assert_eq!((*expected_sequence, *desired_sequence), (1, 2));
        }
        other => panic!("retry must publish SetAll, got {other:?}"),
    }
}

#[test]
fn authority_revalidation_distinguishes_identity_owner_generation_and_binding_epoch() {
    let wire = program_wire(64.0);
    let compiled = match compile_attached_program_wire_v1(&wire) {
        Ok(value) => value,
        Err(_) => panic!("fixture Program must compile"),
    };
    let (emissions, presentations) = bindings();
    let probe_a = HostProbe::default();
    let probe_b = HostProbe::default();
    let mut runtime_a = match compiled.attach(100, &emissions, &presentations, probe_a.host()) {
        Ok(value) => value,
        Err(_) => panic!("first attachment must bind"),
    };
    let runtime_b = match compiled.attach(101, &emissions, &presentations, probe_b.host()) {
        Ok(value) => value,
        Err(_) => panic!("second attachment must bind"),
    };
    let observed = scenario();
    let ready = match runtime_a.update_observed(1, core::slice::from_ref(&observed)) {
        Ok(value) => value,
        Err(_) => panic!("first attachment must publish"),
    };
    let authority = &ready.authorities()[0];
    assert_eq!(
        runtime_b.validate_authority(authority),
        Err(AttachedMaterializationAuthorityErrorV1::ForeignBindingEpoch)
    );

    let same_identity_new_owner = match compile_attached_program_wire_v1(&wire) {
        Ok(value) => value,
        Err(_) => panic!("same bytes must compile again"),
    };
    let probe_c = HostProbe::default();
    let runtime_c =
        match same_identity_new_owner.attach(102, &emissions, &presentations, probe_c.host()) {
            Ok(value) => value,
            Err(_) => panic!("new owner attachment must bind"),
        };
    assert_eq!(
        runtime_c.validate_authority(authority),
        Err(AttachedMaterializationAuthorityErrorV1::ForeignOwnerGeneration)
    );

    let changed_identity = match compile_attached_program_wire_v1(&program_wire(65.0)) {
        Ok(value) => value,
        Err(_) => panic!("changed context Program must compile"),
    };
    let probe_d = HostProbe::default();
    let runtime_d = match changed_identity.attach(103, &emissions, &presentations, probe_d.host()) {
        Ok(value) => value,
        Err(_) => panic!("changed identity attachment must bind"),
    };
    assert_eq!(
        runtime_d.validate_authority(authority),
        Err(AttachedMaterializationAuthorityErrorV1::ProgramIdentityMismatch)
    );
}
