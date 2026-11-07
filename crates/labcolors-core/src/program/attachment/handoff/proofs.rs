//! Реальный host writer после допуска. Подменяется только внешний порт:
//! хост либо принимает весь intent, либо отвергает его без публикации.
//! Это не доказательство браузера, выдачи epoch или полного Session-графа.
use super::super::{AttachedPointEmissionV1, PointSinkMutationStampV1, PointSinkPatchEntryV1};
use super::*;
use crate::Srgb8;
use crate::appearance::{EncodedPointPaintValueV1, PaintId};
use crate::composition::AdmittedOpacityV1;

// Область доказательства: один принадлежащий output, ноль или один
// опубликованный point, без composites. Ожидание хранится без heap-копии,
// но сравнивается с целым реальным slice, включая его длину.
#[derive(Clone, Copy)]
struct ExpectedPointState {
    stamp: PointSinkStampV1,
    revision: Option<u64>,
    point: Option<HandoffPublishedPointV1>,
}

fn points_match(
    expected: Option<HandoffPublishedPointV1>,
    actual: &[HandoffPublishedPointV1],
) -> bool {
    match expected {
        None => actual.is_empty(),
        Some(point) => actual.len() == 1 && actual.first() == Some(&point),
    }
}

impl ExpectedPointState {
    fn matches(self, actual: &HandoffCommittedPointStateV1) -> bool {
        actual.stamp == self.stamp
            && actual.revision == self.revision
            && points_match(self.point, &actual.points)
            && actual.composites.is_empty()
    }
}

// Host snapshot охватывает все поля команды в той же ограниченной области.
// Размеры не позволяют лишнему output, point или composite исчезнуть из oracle.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ObservedHostIntent {
    operation: u32,
    revision: u64,
    expected_sequence: u64,
    desired_sequence: u64,
    binding_epoch: PointSinkBindingEpochV1,
    scope_length: usize,
    scope: Option<HandoffPointSinkOutputIdV1>,
    points_length: usize,
    point: Option<HandoffPublishedPointV1>,
    composites_length: usize,
}

impl ObservedHostIntent {
    fn from_intent(intent: HandoffPointSinkHostIntentV1<'_>) -> Self {
        Self {
            operation: intent.operation(),
            revision: intent.revision(),
            expected_sequence: intent.expected_sequence(),
            desired_sequence: intent.desired_sequence(),
            binding_epoch: intent.binding_epoch(),
            scope_length: intent.scope().len(),
            scope: intent.scope().first().copied(),
            points_length: intent.points().len(),
            point: intent.points().first().copied(),
            composites_length: intent.presentation_composites().len(),
        }
    }
}

#[derive(Default)]
struct AtomicHost {
    calls: u8,
    accepted: Option<ObservedHostIntent>,
    failure: Option<HandoffPointSinkHostErrorV1>,
}

impl HandoffPointSinkHostV1 for AtomicHost {
    fn try_install(
        &mut self,
        intent: HandoffPointSinkHostIntentV1<'_>,
    ) -> Result<(), HandoffPointSinkHostErrorV1> {
        self.calls += 1;
        if let Some(error) = self.failure {
            return Err(error);
        }
        self.accepted = Some(ObservedHostIntent::from_intent(intent));
        Ok(())
    }
}

fn any_stamp() -> Option<PointSinkStampV1> {
    let epoch = PointSinkBindingEpochV1::new(NonZeroU64::new(kani::any())?);
    Some(PointSinkStampV1::new(kani::any(), epoch))
}

fn any_paint() -> Option<EncodedPointPaintV1> {
    let bits: u64 = kani::any();
    if bits > 1.0_f64.to_bits() {
        return None;
    }
    let opacity = AdmittedOpacityV1::new(f64::from_bits(bits)).expect("canonical opacity admits");
    Some(EncodedPointPaintV1::from_value(
        PaintId::new(kani::any()),
        EncodedPointPaintValueV1::from_admitted(Srgb8::new(kani::any()), opacity),
    ))
}

fn any_state() -> Option<(HandoffPointSinkOutputIdV1, ExpectedPointState)> {
    let owned = HandoffPointSinkOutputIdV1::new(kani::any());
    let stamp = any_stamp()?;
    let revision = if stamp.sequence() == 0 {
        None
    } else {
        Some(kani::any())
    };
    let point = if revision.is_some() && kani::any() {
        Some(HandoffPublishedPointV1 {
            output: OutputSlotIdV1::new(kani::any()),
            sink_output: owned,
            paint: any_paint()?,
        })
    } else {
        None
    };
    Some((
        owned,
        ExpectedPointState {
            stamp,
            revision,
            point,
        },
    ))
}

// Недопустимые входы отсекаются до создания writer: Drop его Vec относится
// к проверяемому prepare/install/finish, а не к генератору.
fn writer_from_state(
    owned: HandoffPointSinkOutputIdV1,
    state: ExpectedPointState,
) -> AdmittedHandoffPointSinkWriterV1<AtomicHost> {
    AdmittedHandoffPointSinkWriterV1 {
        owned_scope: vec![owned],
        binding_epoch: state.stamp.binding_epoch(),
        committed: HandoffCommittedPointStateV1 {
            stamp: state.stamp,
            revision: state.revision,
            points: match state.point {
                Some(point) => vec![point],
                None => Vec::new(),
            },
            composites: Vec::new(),
        },
        retired: None,
        host: AtomicHost::default(),
    }
}

fn patch(
    owned: HandoffPointSinkOutputIdV1,
    paint: EncodedPointPaintV1,
) -> PointSinkPatchEntryV1<HandoffPointSinkOutputIdV1> {
    PointSinkPatchEntryV1 {
        emission: AttachedPointEmissionV1 {
            output_ordinal: 0,
            output: OutputSlotIdV1::new(kani::any()),
            sink_output: owned,
        },
        paint,
    }
}

// kind=0, kind=1 и остальные u8 различаются только операцией. Три
// мономорфизации сохраняют полный домен остальных входов и все guards.
// Wrapper передаёт свои свидетели отдельным типом замыкания: Kani включает
// даже недостижимые cover-ветки общего тела в отчёт.
fn preparation_is_exact_and_has_no_publication_effect<const KIND: u8>(
    operation_witness: impl FnOnce(u8, Option<HandoffPointSinkErrorV1>, bool),
) {
    let Some((owned, old)) = any_state() else {
        return;
    };
    let Some(query) = (if kani::any() {
        Some(old.stamp)
    } else {
        any_stamp()
    }) else {
        return;
    };
    let revision: u64 = kani::any();
    let kind = KIND;
    let patches;
    let expected;
    let mut expected_error = (query != old.stamp).then_some(HandoffPointSinkErrorV1::StampMismatch);
    let intent = if KIND == 0 {
        let Some(stamp) = PointSinkMutationStampV1::new(query) else {
            return;
        };
        let Some(paint) = any_paint() else {
            return;
        };
        let point = patch(HandoffPointSinkOutputIdV1::new(kani::any()), paint);
        patches = [point, point];
        let length = usize::from(kani::any::<u8>() % 3);
        if expected_error.is_none() && (length != 1 || point.sink_output() != owned) {
            expected_error = Some(HandoffPointSinkErrorV1::PatchScopeMismatch);
        }
        expected = expected_mutation(
            query,
            revision,
            Some(HandoffPublishedPointV1 {
                output: point.output(),
                sink_output: point.sink_output(),
                paint,
            }),
        );
        PointSinkIntentV1::SetAll {
            revision,
            stamp,
            patch: &patches[..length],
            presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
        }
    } else if KIND == 1 {
        let Some(stamp) = PointSinkMutationStampV1::new(query) else {
            return;
        };
        expected = expected_mutation(query, revision, None);
        PointSinkIntentV1::RevokeAll { revision, stamp }
    } else {
        if expected_error.is_none() && old.revision != Some(revision) {
            expected_error = Some(HandoffPointSinkErrorV1::RevisionMismatch);
        }
        expected = old;
        PointSinkIntentV1::ConfirmExact {
            revision,
            published_stamp: query,
        }
    };
    let mut writer = writer_from_state(owned, old);
    let actual = match writer.prepare(intent) {
        Err(error) => Some(error),
        Ok(prepared) => {
            let staged = prepared
                .staged
                .as_ref()
                .expect("successful preparation has a pending command");
            let command =
                staged.host_intent(&prepared.writer.owned_scope, &prepared.writer.committed);
            assert!(
                staged.expected_stamp() == old.stamp
                    && expected.matches(staged.desired(&prepared.writer.committed)),
                "handoff preparation must retain the exact previous and desired state"
            );
            assert!(
                command.revision() == revision
                    && command.expected_sequence() == query.sequence()
                    && command.desired_sequence() == expected.stamp.sequence()
                    && command.binding_epoch() == query.binding_epoch()
                    && command.scope() == prepared.writer.owned_scope
                    && points_match(expected.point, command.points())
                    && command.presentation_composites().is_empty(),
                "handoff command must preserve every publication field"
            );
            assert!(
                command.operation()
                    == if kind == 0 {
                        1
                    } else if kind == 1 {
                        2
                    } else {
                        3
                    },
                "handoff command must preserve the requested operation"
            );
            None
        }
    };
    assert!(
        actual == expected_error,
        "handoff preparation must reject exactly wrong stamp scope or revision"
    );
    assert!(
        old.matches(&writer.committed) && writer.host.calls == 0 && writer.host.accepted.is_none(),
        "preparing or abandoning a handoff must never publish or advance state"
    );
    operation_witness(kind, actual, old.point.is_some());
    kani::cover!(
        actual == Some(HandoffPointSinkErrorV1::StampMismatch),
        "foreign stamp rejected"
    );
}

#[kani::proof]
#[kani::unwind(4)]
fn preparation_is_exact_and_has_no_publication_effect_set_all() {
    preparation_is_exact_and_has_no_publication_effect::<0>(|kind, actual, _| {
        kani::cover!(actual.is_none() && kind == 0, "valid set prepared");
        kani::cover!(
            actual == Some(HandoffPointSinkErrorV1::PatchScopeMismatch),
            "foreign scope rejected"
        );
    });
}

#[kani::proof]
#[kani::unwind(4)]
fn preparation_is_exact_and_has_no_publication_effect_revoke_all() {
    preparation_is_exact_and_has_no_publication_effect::<1>(|kind, actual, _| {
        kani::cover!(actual.is_none() && kind == 1, "valid revoke prepared");
    });
}

#[kani::proof]
#[kani::unwind(4)]
fn preparation_is_exact_and_has_no_publication_effect_confirm_exact() {
    preparation_is_exact_and_has_no_publication_effect::<2>(|kind, actual, point_present| {
        kani::cover!(
            actual.is_none() && kind > 1 && point_present,
            "published value confirmed"
        );
        kani::cover!(
            actual.is_none() && kind > 1 && !point_present,
            "empty value confirmed"
        );
        kani::cover!(
            actual == Some(HandoffPointSinkErrorV1::RevisionMismatch),
            "foreign revision rejected"
        );
    });
}

fn expected_mutation(
    stamp: PointSinkStampV1,
    revision: u64,
    point: Option<HandoffPublishedPointV1>,
) -> ExpectedPointState {
    ExpectedPointState {
        stamp: PointSinkStampV1::new(stamp.sequence() + 1, stamp.binding_epoch()),
        revision: Some(revision),
        point,
    }
}

fn pending_matches(
    pending: Option<&HandoffStagedPointStateV1>,
    old_stamp: PointSinkStampV1,
    kind: u8,
    revision: u64,
    expected: ExpectedPointState,
) -> bool {
    match pending {
        Some(HandoffStagedPointStateV1::SetAll {
            revision: actual_revision,
            expected_stamp,
            desired,
        }) => {
            kind == 0
                && *actual_revision == revision
                && *expected_stamp == old_stamp
                && expected.matches(desired)
        }
        Some(HandoffStagedPointStateV1::RevokeAll {
            revision: actual_revision,
            expected_stamp,
            desired,
        }) => {
            kind == 1
                && *actual_revision == revision
                && *expected_stamp == old_stamp
                && expected.matches(desired)
        }
        Some(HandoffStagedPointStateV1::ConfirmExact {
            expected_stamp,
            revision: actual_revision,
        }) => kind > 1 && *actual_revision == revision && *expected_stamp == old_stamp,
        None => false,
    }
}

fn rejected_install_is_atomic_retryable_and_success_is_once_only<const KIND: u8>(
    operation_witness: impl FnOnce(u8, u64),
) {
    let Some((owned, old)) = any_state() else {
        return;
    };
    let kind = KIND;
    let points;
    let expected;
    let intent;
    if KIND == 0 {
        let Some(stamp) = PointSinkMutationStampV1::new(old.stamp) else {
            return;
        };
        let Some(paint) = any_paint() else {
            return;
        };
        points = [patch(owned, paint)];
        let revision = kani::any();
        intent = PointSinkIntentV1::SetAll {
            revision,
            stamp,
            patch: &points,
            presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
        };
        expected = expected_mutation(
            old.stamp,
            revision,
            Some(HandoffPublishedPointV1 {
                output: points[0].output(),
                sink_output: points[0].sink_output(),
                paint,
            }),
        );
    } else if KIND == 1 {
        let Some(stamp) = PointSinkMutationStampV1::new(old.stamp) else {
            return;
        };
        let revision = kani::any();
        intent = PointSinkIntentV1::RevokeAll { revision, stamp };
        expected = expected_mutation(old.stamp, revision, None);
    } else {
        let Some(revision) = old.revision else {
            return;
        };
        intent = PointSinkIntentV1::ConfirmExact {
            revision,
            published_stamp: old.stamp,
        };
        expected = old;
    }
    let installed_revision = expected
        .revision
        .expect("valid exact intent has a revision");
    let expected_command = ObservedHostIntent {
        operation: if kind == 0 {
            1
        } else if kind == 1 {
            2
        } else {
            3
        },
        revision: installed_revision,
        expected_sequence: old.stamp.sequence(),
        desired_sequence: expected.stamp.sequence(),
        binding_epoch: old.stamp.binding_epoch(),
        scope_length: 1,
        scope: Some(owned),
        points_length: usize::from(expected.point.is_some()),
        point: expected.point,
        composites_length: 0,
    };
    let error = match kani::any::<u8>() {
        0 => HandoffPointSinkHostErrorV1::Rejected,
        1 => HandoffPointSinkHostErrorV1::Protocol,
        _ => HandoffPointSinkHostErrorV1::ResourceExhausted,
    };
    let mut writer = writer_from_state(owned, old);
    writer.host.failure = Some(error);
    let mut prepared = writer.prepare(intent).expect("valid exact intent prepares");
    assert!(
        prepared.try_install() == Err(HandoffPointSinkErrorV1::Host(error)),
        "handoff must propagate the exact host refusal"
    );
    assert!(
        old.matches(&prepared.writer.committed)
            && pending_matches(
                prepared.staged.as_ref(),
                old.stamp,
                kind,
                installed_revision,
                expected,
            )
            && prepared.writer.host.calls == 1
            && prepared.writer.host.accepted.is_none(),
        "failed handoff must preserve committed and retryable state"
    );
    prepared.writer.host.failure = None;
    assert!(
        prepared.try_install().is_ok(),
        "exact handoff retry must remain usable"
    );
    assert!(
        expected.matches(&prepared.writer.committed)
            && prepared.staged.is_none()
            && prepared.writer.host.calls == 2
            && prepared.writer.host.accepted == Some(expected_command),
        "successful handoff must commit the complete acknowledged command exactly once"
    );
    assert!(
        prepared.try_install() == Err(HandoffPointSinkErrorV1::AlreadyInstalled)
            && prepared.writer.host.calls == 2
            && expected.matches(&prepared.writer.committed),
        "a consumed handoff must never call the host again"
    );
    prepared.finish_after_session();
    assert!(
        expected.matches(&writer.committed) && writer.host.calls == 2,
        "finishing the handoff must preserve its acknowledged result"
    );
    operation_witness(kind, old.stamp.sequence());
    kani::cover!(
        error == HandoffPointSinkHostErrorV1::Protocol,
        "protocol refusal is retryable"
    );
}

#[kani::proof]
#[kani::unwind(4)]
fn rejected_install_is_atomic_retryable_and_success_is_once_only_set_all() {
    rejected_install_is_atomic_retryable_and_success_is_once_only::<0>(|kind, _| {
        kani::cover!(kind == 0, "rejected set retried");
    });
}

#[kani::proof]
#[kani::unwind(4)]
fn rejected_install_is_atomic_retryable_and_success_is_once_only_revoke_all() {
    rejected_install_is_atomic_retryable_and_success_is_once_only::<1>(|kind, _| {
        kani::cover!(kind == 1, "rejected revoke retried");
    });
}

#[kani::proof]
#[kani::unwind(4)]
fn rejected_install_is_atomic_retryable_and_success_is_once_only_confirm_exact() {
    rejected_install_is_atomic_retryable_and_success_is_once_only::<2>(|kind, sequence| {
        kani::cover!(
            kind > 1 && sequence == u64::MAX,
            "exhausted stamp still permits confirmation"
        );
    });
}

fn stale_prepared_command_never_reaches_the_host<const KIND: u8>() {
    let Some((owned, old)) = any_state() else {
        return;
    };
    let kind = KIND;
    let intent;
    let expected;
    let points;
    if KIND == 0 {
        let Some(stamp) = PointSinkMutationStampV1::new(old.stamp) else {
            return;
        };
        let Some(paint) = any_paint() else {
            return;
        };
        points = [patch(owned, paint)];
        let revision = kani::any();
        intent = PointSinkIntentV1::SetAll {
            revision,
            stamp,
            patch: &points,
            presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
        };
        expected = expected_mutation(
            old.stamp,
            revision,
            Some(HandoffPublishedPointV1 {
                output: points[0].output(),
                sink_output: points[0].sink_output(),
                paint,
            }),
        );
    } else if KIND == 1 {
        let Some(stamp) = PointSinkMutationStampV1::new(old.stamp) else {
            return;
        };
        let revision = kani::any();
        intent = PointSinkIntentV1::RevokeAll { revision, stamp };
        expected = expected_mutation(old.stamp, revision, None);
    } else {
        let Some(revision) = old.revision else {
            return;
        };
        intent = PointSinkIntentV1::ConfirmExact {
            revision,
            published_stamp: old.stamp,
        };
        expected = old;
    }
    let installed_revision = expected
        .revision
        .expect("valid exact intent has a revision");
    let Some(changed) = any_stamp() else {
        return;
    };
    if changed == old.stamp {
        return;
    }
    let mut writer = writer_from_state(owned, old);
    let mut prepared = writer.prepare(intent).expect("valid exact intent prepares");
    // Hostile внутренний drift: обычное эксклюзивное Rust-заимствование его
    // запрещает, но install обязан самостоятельно проверить ожидаемый stamp.
    prepared.writer.committed.stamp = changed;
    let before = ExpectedPointState {
        stamp: changed,
        ..old
    };
    assert!(
        prepared.try_install() == Err(HandoffPointSinkErrorV1::StampMismatch)
            && prepared.writer.host.calls == 0
            && prepared.writer.host.accepted.is_none()
            && before.matches(&prepared.writer.committed)
            && pending_matches(
                prepared.staged.as_ref(),
                old.stamp,
                kind,
                installed_revision,
                expected,
            ),
        "stale prepared handoff must fail before any host call or state mutation"
    );
    kani::cover!(
        changed.sequence() == old.stamp.sequence(),
        "same sequence foreign epoch rejected"
    );
    kani::cover!(
        changed.binding_epoch() == old.stamp.binding_epoch(),
        "same epoch foreign sequence rejected"
    );
}

#[kani::proof]
#[kani::unwind(4)]
fn stale_prepared_command_never_reaches_the_host_set_all() {
    stale_prepared_command_never_reaches_the_host::<0>();
}

#[kani::proof]
#[kani::unwind(4)]
fn stale_prepared_command_never_reaches_the_host_revoke_all() {
    stale_prepared_command_never_reaches_the_host::<1>();
}

#[kani::proof]
#[kani::unwind(4)]
fn stale_prepared_command_never_reaches_the_host_confirm_exact() {
    stale_prepared_command_never_reaches_the_host::<2>();
}
