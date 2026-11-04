use super::*;
use crate::program::wire::ProgramWireBuilderV1;
use std::cell::RefCell;
use std::rc::Rc;

const SLOT_A: u32 = 90;
const SLOT_B: u32 = 110;
const SINK_A: u32 = 901;
const SINK_B: u32 = 1101;
const SCENARIO: u32 = 71;
const SOURCE_A: Srgb8 = Srgb8::new([200, 40, 80]);
const SOURCE_B: Srgb8 = Srgb8::new([40, 160, 200]);
const BACKGROUNDS: [Srgb8; 2] = [Srgb8::new([20, 80, 120]), Srgb8::new([100, 120, 40])];
const BINDINGS: [(u32, u32, u32); 3] = [(SLOT_A, 9, 8), (SLOT_A, 19, 18), (SLOT_B, 29, 28)];

fn declaration(reverse: bool) -> Vec<u8> {
    let mut builder = ProgramWireBuilderV1::new();
    let order = if reverse { [1, 0] } else { [0, 1] };
    let paints = [
        (1, 2, 3, 4, 5, SOURCE_A, 0.5, SLOT_A),
        (11, 12, 13, 14, 15, SOURCE_B, 0.25, SLOT_B),
    ];
    for index in order {
        let (source, target, solid, paint, opacity, rgb, alpha, slot) = paints[index];
        builder
            .source(source, rgb)
            .fixed_target(target, source)
            .opacity_input(opacity, alpha)
            .solid_paint(solid, target)
            .opacity_paint(paint, solid, opacity)
            .output(slot, paint);
    }
    for index in order {
        let (port, surface) = [(6, 7), (16, 17)][index];
        builder
            .surface_input_port(port)
            .input_surface(surface, port);
    }
    let order = if reverse { [2, 1, 0] } else { [0, 1, 2] };
    for index in order {
        let (occurrence, root, paint, surface) =
            [(8, 9, 4, 7), (18, 19, 4, 17), (28, 29, 14, 7)][index];
        builder
            .source_over_occurrence(occurrence, paint, surface, 64.0, 0.2, 2)
            .presentation_root(root, occurrence)
            .presentation_target(root, occurrence)
            // Отчёт задаёт предмет проверки, сохраняя законную смену фона.
            .exact_visible_unary(
                false,
                100 + root,
                occurrence,
                expected_composites(BACKGROUNDS)[index].1,
            );
    }
    builder.finish().expect("полная декларация кодируется")
}

fn emissions() -> [ProgramPointEmissionBindingV1; 2] {
    [
        ProgramPointEmissionBindingV1::new(SLOT_A, SINK_A),
        ProgramPointEmissionBindingV1::new(SLOT_B, SINK_B),
    ]
}

fn presentations() -> [ProgramPointPresentationBindingV1; 3] {
    BINDINGS.map(|(slot, root, occurrence)| {
        ProgramPointPresentationBindingV1::new(slot, root, occurrence)
    })
}

// Рациональный oracle проверяет encoded source-over, не физические пиксели или восприятие.
fn composite(source: Srgb8, background: Srgb8, denominator: u16) -> Srgb8 {
    let source = source.bytes();
    let background = background.bytes();
    Srgb8::new(core::array::from_fn(|channel| {
        let numerator =
            u16::from(source[channel]) + (denominator - 1) * u16::from(background[channel]);
        assert_eq!(numerator % denominator, 0);
        u8::try_from(numerator / denominator).expect("выпуклая комбинация лежит в sRGB8")
    }))
}

fn expected_composites(backgrounds: [Srgb8; 2]) -> [(u32, Srgb8); 3] {
    [
        (9, composite(SOURCE_A, backgrounds[0], 2)),
        (19, composite(SOURCE_A, backgrounds[1], 2)),
        (29, composite(SOURCE_B, backgrounds[0], 4)),
    ]
}

fn observed(backgrounds: [Srgb8; 2]) -> [ProgramScenarioV1; 1] {
    [ProgramScenarioV1::new(SCENARIO, backgrounds.to_vec())]
}

#[derive(Debug, Clone, PartialEq)]
struct OwnedIntent {
    operation: ProgramPointSinkOperationV1,
    revision: u64,
    expected: ProgramPointSinkStampV1,
    desired: ProgramPointSinkStampV1,
    scope: Vec<u32>,
    points: Vec<ProgramPointSinkPointV1>,
    composites: Vec<ProgramPointPresentationCompositeV1>,
}

impl OwnedIntent {
    fn copy_from(intent: ProgramPointSinkIntentV1<'_>) -> Self {
        assert_eq!(intent.point(), None);
        assert_eq!(intent.sink_output(), None);
        Self {
            operation: intent.operation(),
            revision: intent.revision(),
            expected: intent.expected_stamp(),
            desired: intent.desired_stamp(),
            scope: intent.scope().collect(),
            points: intent.points().collect(),
            composites: intent.presentation_composites().collect(),
        }
    }

    fn assert_complete(&self, backgrounds: [Srgb8; 2]) {
        let mut scope = self.scope.clone();
        scope.sort_unstable();
        assert_eq!(scope, [SINK_A, SINK_B]);
        assert_eq!(self.points.len(), 2);
        for (slot, sink, source, alpha) in [
            (SLOT_A, SINK_A, SOURCE_A, 0.5),
            (SLOT_B, SINK_B, SOURCE_B, 0.25),
        ] {
            let point = self
                .points
                .iter()
                .find(|point| point.output().slot() == slot)
                .expect("каждый output присутствует до ACK");
            assert_eq!(point.sink_output(), sink);
            assert_eq!(point.output().source(), source);
            assert_eq!(point.output().opacity(), alpha);
        }
        assert_eq!(self.composites.len(), 3);
        for (root, expected) in expected_composites(backgrounds) {
            let actual = self
                .composites
                .iter()
                .find(|value| value.root() == root && value.scenario() == SCENARIO)
                .expect("каждая presentation присутствует до ACK");
            assert_eq!(actual.visible(), expected);
        }
    }
}

#[derive(Default)]
struct HostState {
    attempts: Vec<OwnedIntent>,
    stamp: Option<ProgramPointSinkStampV1>,
    installed: Option<OwnedIntent>,
    backgrounds: Option<[Srgb8; 2]>,
    reject_next: bool,
}

struct Host(Rc<RefCell<HostState>>);

impl ProgramPointSinkHostV1 for Host {
    fn try_install(
        &mut self,
        intent: ProgramPointSinkIntentV1<'_>,
    ) -> Result<(), ProgramPointSinkHostErrorV1> {
        // Все borrowed данные копируются и проверяются до ответа внешнему владельцу.
        let record = OwnedIntent::copy_from(intent);
        let mut state = self.0.borrow_mut();
        if let Some(stamp) = state.stamp {
            assert_eq!(record.expected, stamp);
        } else {
            assert_eq!(record.expected.sequence(), 0);
        }
        assert_ne!(record.expected.binding_epoch(), 0);
        match record.operation {
            ProgramPointSinkOperationV1::SetAll | ProgramPointSinkOperationV1::ConfirmExact => {
                record.assert_complete(state.backgrounds.expect("наблюдение объявлено host"));
            }
            ProgramPointSinkOperationV1::RevokeAll => {
                let mut scope = record.scope.clone();
                scope.sort_unstable();
                assert_eq!(scope, [SINK_A, SINK_B]);
                assert!(record.points.is_empty());
                assert!(record.composites.is_empty());
            }
        }
        state.attempts.push(record.clone());
        if core::mem::take(&mut state.reject_next) {
            return Err(ProgramPointSinkHostErrorV1::Rejected);
        }
        match record.operation {
            ProgramPointSinkOperationV1::SetAll => state.installed = Some(record.clone()),
            ProgramPointSinkOperationV1::RevokeAll => state.installed = None,
            ProgramPointSinkOperationV1::ConfirmExact => {
                let installed = state
                    .installed
                    .as_ref()
                    .expect("повтор подтверждает commit");
                assert_eq!(record.revision, installed.revision);
                assert_eq!(record.scope, installed.scope);
                assert_eq!(record.points, installed.points);
                assert_eq!(record.composites, installed.composites);
                assert_eq!(record.desired, installed.desired);
            }
        }
        state.stamp = Some(record.desired);
        Ok(())
    }
}

fn attach(
    stream: u32,
    reverse_declaration: bool,
    reverse_bindings: bool,
) -> (ProgramAttachmentV1<Host>, Rc<RefCell<HostState>>) {
    let state = Rc::new(RefCell::new(HostState::default()));
    let mut emissions = emissions();
    let mut presentations = presentations();
    if reverse_bindings {
        emissions.reverse();
        presentations.reverse();
    }
    let attachment = compile_program_wire_v1(&declaration(reverse_declaration))
        .expect("декларация компилируется")
        .attach_bindings(stream, &emissions, &presentations, Host(Rc::clone(&state)))
        .expect("полная область допускается");
    assert!(state.borrow().attempts.is_empty());
    (attachment, state)
}

fn assert_renders(
    attachment: &ProgramAttachmentV1<Host>,
    snapshot: &ProgramAttachedSnapshotV1,
    backgrounds: [Srgb8; 2],
) {
    assert_eq!(snapshot.snapshot().state(), ProgramSnapshotStateV1::Ready);
    assert!(snapshot.render().is_none());
    assert!(attachment.current_render().is_none());
    assert_eq!(snapshot.renders(), attachment.current_renders());
    assert_eq!(snapshot.renders().len(), 3);
    for ((slot, root, occurrence), (_, expected)) in
        BINDINGS.into_iter().zip(expected_composites(backgrounds))
    {
        let render = attachment
            .current_render_for(root, occurrence)
            .expect("точный binding доступен");
        assert_eq!(render.output().slot(), slot);
        assert_eq!(render.terminal_composite(), Some(expected));
        assert_eq!(
            snapshot
                .snapshot()
                .presentation_composite_rgb(root, SCENARIO),
            Some(expected)
        );
        assert_ne!(render.output().source(), expected);
    }
}

#[test]
fn complete_scope_is_observed_before_ack_and_keeps_alpha_under_permutations() {
    for (reverse_declaration, reverse_bindings) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let (mut attachment, state) = attach(31, reverse_declaration, reverse_bindings);
        state.borrow_mut().backgrounds = Some(BACKGROUNDS);
        let initial = attachment
            .update_observed(1, &observed(BACKGROUNDS))
            .expect("полный снимок установлен");
        assert_renders(&attachment, &initial, BACKGROUNDS);
        let initial_patch = state.borrow().installed.clone().unwrap();
        assert_eq!(initial_patch.operation, ProgramPointSinkOperationV1::SetAll);

        let swapped = [BACKGROUNDS[1], BACKGROUNDS[0]];
        state.borrow_mut().backgrounds = Some(swapped);
        let updated = attachment
            .update_observed(2, &observed(swapped))
            .expect("новый фон пересчитан");
        assert_renders(&attachment, &updated, swapped);
        let updated_patch = state.borrow().installed.clone().unwrap();
        assert_eq!(updated_patch.points, initial_patch.points);
        assert_ne!(updated_patch.composites, initial_patch.composites);
        assert_eq!(state.borrow().attempts.len(), 2);
    }
}

#[test]
fn authority_selects_one_presentation_and_refuses_foreign_or_stale_bindings() {
    let (mut attachment, state) = attach(32, false, false);
    state.borrow_mut().backgrounds = Some(BACKGROUNDS);
    let initial = attachment
        .update_observed(1, &observed(BACKGROUNDS))
        .unwrap();
    assert_eq!(
        attachment.current_materialization_authority(),
        Err(ProgramMaterializationAuthorityErrorV1::AmbiguousBindings)
    );
    for ((slot, root, occurrence), (_, expected)) in
        BINDINGS.into_iter().zip(expected_composites(BACKGROUNDS))
    {
        let authority = attachment
            .current_materialization_authority_for(root, occurrence)
            .unwrap();
        assert_eq!(authority.output().slot(), slot);
        assert_eq!(authority.presentation_root(), root);
        assert_eq!(authority.occurrence(), occurrence);
        assert_eq!(authority.terminal_composite(), expected);
        assert_eq!(
            authority.renderer_provenance(),
            ProgramRendererProvenanceV1::Unverified
        );
        assert_ne!(authority.output().source(), expected);
    }
    assert!(attachment.current_render_for(9, 18).is_none());
    assert_eq!(
        attachment.current_materialization_authority_for(9, 18),
        Err(ProgramMaterializationAuthorityErrorV1::TerminalBindingMismatch)
    );

    let (mut foreign, foreign_state) = attach(33, false, false);
    foreign_state.borrow_mut().backgrounds = Some(BACKGROUNDS);
    foreign.update_observed(1, &observed(BACKGROUNDS)).unwrap();
    let foreign_expectation = foreign.current_render_for(9, 8).unwrap().expectation();
    assert_eq!(
        attachment.materialization_authority(
            ProgramMaterializationCandidateV1::AttachedCommit,
            foreign_expectation,
        ),
        Err(ProgramMaterializationAuthorityErrorV1::ForeignBindingEpoch)
    );

    let swapped = [BACKGROUNDS[1], BACKGROUNDS[0]];
    state.borrow_mut().backgrounds = Some(swapped);
    attachment.update_observed(2, &observed(swapped)).unwrap();
    for old_render in initial.renders() {
        assert_eq!(
            attachment.materialization_authority(
                ProgramMaterializationCandidateV1::AttachedCommit,
                old_render.expectation(),
            ),
            Err(ProgramMaterializationAuthorityErrorV1::StaleRevision)
        );
    }
    let current = attachment.current_render_for(9, 8).unwrap();
    for candidate in [
        ProgramMaterializationCandidateV1::SourcePaint(current.output()),
        ProgramMaterializationCandidateV1::IntermediatePaint(current.output()),
    ] {
        assert_eq!(
            attachment.materialization_authority(candidate, current.expectation()),
            Err(ProgramMaterializationAuthorityErrorV1::SourceOrIntermediatePaint)
        );
    }
}

#[test]
fn rejection_replay_and_unknown_preserve_or_revoke_the_whole_scope() {
    let (mut attachment, state) = attach(34, false, false);
    state.borrow_mut().backgrounds = Some(BACKGROUNDS);
    let initial = attachment
        .update_observed(1, &observed(BACKGROUNDS))
        .unwrap();
    let old_host = state.borrow().installed.clone();
    let swapped = [BACKGROUNDS[1], BACKGROUNDS[0]];
    {
        let mut host = state.borrow_mut();
        host.backgrounds = Some(swapped);
        host.reject_next = true;
    }
    assert_eq!(
        attachment.update_observed(2, &observed(swapped)),
        Err(ProgramAttachmentUpdateErrorV1::Sink(
            ProgramPointSinkErrorV1::Host(ProgramPointSinkHostErrorV1::Rejected,)
        ))
    );
    assert_eq!(attachment.current_renders(), initial.renders());
    assert_eq!(state.borrow().installed, old_host);
    let retried = attachment.update_observed(2, &observed(swapped)).unwrap();
    assert_renders(&attachment, &retried, swapped);
    let committed = state.borrow().installed.clone();
    let replayed = attachment.update_observed(2, &observed(swapped)).unwrap();
    assert_eq!(replayed, retried);
    assert_eq!(state.borrow().installed, committed);
    assert_eq!(
        state.borrow().attempts.last().unwrap().operation,
        ProgramPointSinkOperationV1::ConfirmExact
    );

    let calls = state.borrow().attempts.len();
    assert_eq!(
        attachment.update_observed(1, &observed(BACKGROUNDS)),
        Err(ProgramAttachmentUpdateErrorV1::Update)
    );
    assert_eq!(
        attachment.update_observed(2, &observed(BACKGROUNDS)),
        Err(ProgramAttachmentUpdateErrorV1::Update)
    );
    assert_eq!(state.borrow().attempts.len(), calls);
    assert_eq!(attachment.current_renders(), retried.renders());
    assert_eq!(state.borrow().installed, committed);

    let stale = attachment.update_unknown(3, 41).unwrap();
    assert_eq!(stale.snapshot().state(), ProgramSnapshotStateV1::Stale);
    assert!(stale.renders().is_empty());
    assert!(attachment.current_renders().is_empty());
    assert_eq!(
        attachment.current_materialization_authority_for(9, 8),
        Err(ProgramMaterializationAuthorityErrorV1::NotReady)
    );
    assert!(state.borrow().installed.is_none());
    assert_eq!(
        state.borrow().attempts.last().unwrap().operation,
        ProgramPointSinkOperationV1::RevokeAll
    );
    state.borrow_mut().backgrounds = Some(BACKGROUNDS);
    let recovered = attachment
        .update_observed(4, &observed(BACKGROUNDS))
        .unwrap();
    assert_renders(&attachment, &recovered, BACKGROUNDS);
    assert_eq!(recovered.snapshot().revision(), 4);
}

#[test]
fn admission_refuses_missing_or_duplicate_coverage_without_calling_host() {
    let emissions = emissions();
    let presentations = presentations();
    for (emissions, presentations) in [
        (emissions[..1].to_vec(), presentations.to_vec()),
        (emissions.to_vec(), presentations[..2].to_vec()),
        (vec![emissions[0], emissions[0]], presentations.to_vec()),
        (
            emissions.to_vec(),
            vec![presentations[0], presentations[0], presentations[2]],
        ),
        (
            vec![
                emissions[0],
                ProgramPointEmissionBindingV1::new(SLOT_B, SINK_A),
            ],
            presentations.to_vec(),
        ),
    ] {
        let state = Rc::new(RefCell::new(HostState::default()));
        let result = compile_program_wire_v1(&declaration(false))
            .unwrap()
            .attach_bindings(35, &emissions, &presentations, Host(Rc::clone(&state)));
        assert!(result.is_err());
        assert!(state.borrow().attempts.is_empty());
    }
}
