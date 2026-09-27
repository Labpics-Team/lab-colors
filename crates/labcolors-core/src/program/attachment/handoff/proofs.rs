//! Реальный host writer после допуска. Подменяется только внешний порт:
//! хост либо принимает весь intent, либо отвергает его без публикации.
//! Это не доказательство браузера, выдачи epoch или полного Session-графа.
use super::super::{AttachedPointEmissionV1, PointSinkMutationStampV1, PointSinkPatchEntryV1};
use super::*;
use crate::Srgb8;
use crate::appearance::{EncodedPointPaintValueV1, PaintId};
use crate::composition::AdmittedOpacityV1;

#[derive(Default)]
struct AtomicHost {
    calls: u8,
    accepted: Option<HandoffPointSinkHostIntentV1>,
    failure: Option<HandoffPointSinkHostErrorV1>,
}

impl HandoffPointSinkHostV1 for AtomicHost {
    fn try_install(
        &mut self,
        intent: HandoffPointSinkHostIntentV1,
    ) -> Result<(), HandoffPointSinkHostErrorV1> {
        self.calls += 1;
        if let Some(error) = self.failure {
            return Err(error);
        }
        self.accepted = Some(intent);
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

fn any_writer() -> Option<AdmittedHandoffPointSinkWriterV1<AtomicHost>> {
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
    Some(AdmittedHandoffPointSinkWriterV1 {
        owned_scope: [owned],
        binding_epoch: stamp.binding_epoch(),
        committed: HandoffCommittedPointStateV1 {
            stamp,
            revision,
            point,
        },
        host: AtomicHost::default(),
    })
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

#[kani::proof]
#[kani::unwind(4)]
fn preparation_is_exact_and_has_no_publication_effect() {
    let Some(mut writer) = any_writer() else {
        return;
    };
    let old = writer.committed;
    let Some(query) = (if kani::any() {
        Some(old.stamp)
    } else {
        any_stamp()
    }) else {
        return;
    };
    let revision: u64 = kani::any();
    let Some(paint) = any_paint() else {
        return;
    };
    let point = patch(HandoffPointSinkOutputIdV1::new(kani::any()), paint);
    let patches = [point, point];
    let length = usize::from(kani::any::<u8>() % 3);
    let kind: u8 = kani::any();
    let intent = match kind {
        0 => PointSinkIntentV1::SetAll {
            revision,
            stamp: match PointSinkMutationStampV1::new(query) {
                Some(stamp) => stamp,
                None => return,
            },
            patch: &patches[..length],
        },
        1 => PointSinkIntentV1::RevokeAll {
            revision,
            stamp: match PointSinkMutationStampV1::new(query) {
                Some(stamp) => stamp,
                None => return,
            },
        },
        _ => PointSinkIntentV1::ConfirmExact {
            revision,
            published_stamp: query,
        },
    };
    let expected_error = if query != old.stamp {
        Some(HandoffPointSinkErrorV1::StampMismatch)
    } else if kind == 0 && (length != 1 || point.sink_output() != writer.owned_scope[0]) {
        Some(HandoffPointSinkErrorV1::PatchScopeMismatch)
    } else if kind > 1 && old.revision != Some(revision) {
        Some(HandoffPointSinkErrorV1::RevisionMismatch)
    } else {
        None
    };
    let expected = if kind < 2 {
        HandoffCommittedPointStateV1 {
            stamp: PointSinkStampV1::new(query.sequence() + 1, query.binding_epoch()),
            revision: Some(revision),
            point: if kind == 0 {
                Some(HandoffPublishedPointV1 {
                    output: point.output(),
                    sink_output: point.sink_output(),
                    paint,
                })
            } else {
                None
            },
        }
    } else {
        old
    };
    let actual = match writer.prepare(intent) {
        Err(error) => Some(error),
        Ok(prepared) => {
            let staged = prepared
                .staged
                .expect("successful preparation has a pending command");
            assert!(
                staged.expected_stamp == old.stamp && staged.desired == expected,
                "handoff preparation must retain the exact previous and desired state"
            );
            assert!(
                staged.host_intent.revision() == revision
                    && staged.host_intent.expected_sequence() == query.sequence()
                    && staged.host_intent.desired_sequence() == expected.stamp.sequence()
                    && staged.host_intent.binding_epoch() == query.binding_epoch()
                    && staged.host_intent.point()
                        == expected.point.map(|p| (p.output, p.sink_output, p.paint)),
                "handoff command must preserve every publication field"
            );
            assert!(
                staged.host_intent.operation()
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
        writer.committed == old && writer.host.calls == 0 && writer.host.accepted.is_none(),
        "preparing or abandoning a handoff must never publish or advance state"
    );
    kani::cover!(actual.is_none() && kind == 0, "valid set prepared");
    kani::cover!(actual.is_none() && kind == 1, "valid revoke prepared");
    kani::cover!(
        actual.is_none() && kind > 1 && old.point.is_some(),
        "published value confirmed"
    );
    kani::cover!(
        actual.is_none() && kind > 1 && old.point.is_none(),
        "empty value confirmed"
    );
    kani::cover!(
        actual == Some(HandoffPointSinkErrorV1::StampMismatch),
        "foreign stamp rejected"
    );
    kani::cover!(
        actual == Some(HandoffPointSinkErrorV1::PatchScopeMismatch),
        "foreign scope rejected"
    );
    kani::cover!(
        actual == Some(HandoffPointSinkErrorV1::RevisionMismatch),
        "foreign revision rejected"
    );
}

fn valid_intent<'a>(
    old: HandoffCommittedPointStateV1,
    point: &'a [PointSinkPatchEntryV1<HandoffPointSinkOutputIdV1>],
    kind: u8,
    revision: u64,
) -> Option<PointSinkIntentV1<'a, HandoffPointSinkOutputIdV1>> {
    Some(match kind {
        0 => PointSinkIntentV1::SetAll {
            revision,
            stamp: PointSinkMutationStampV1::new(old.stamp)?,
            patch: point,
        },
        1 => PointSinkIntentV1::RevokeAll {
            revision,
            stamp: PointSinkMutationStampV1::new(old.stamp)?,
        },
        _ => PointSinkIntentV1::ConfirmExact {
            revision: old.revision?,
            published_stamp: old.stamp,
        },
    })
}

#[kani::proof]
#[kani::unwind(4)]
fn rejected_install_is_atomic_retryable_and_success_is_once_only() {
    let Some(mut writer) = any_writer() else {
        return;
    };
    let old = writer.committed;
    let Some(paint) = any_paint() else {
        return;
    };
    let points = [patch(writer.owned_scope[0], paint)];
    let kind: u8 = kani::any();
    let Some(intent) = valid_intent(old, &points, kind, kani::any()) else {
        return;
    };
    let error = if kani::any() {
        HandoffPointSinkHostErrorV1::Rejected
    } else {
        HandoffPointSinkHostErrorV1::Protocol
    };
    writer.host.failure = Some(error);
    let mut prepared = writer.prepare(intent).expect("valid exact intent prepares");
    let staged = prepared.staged;
    let expected = staged.expect("prepared command exists");
    assert!(
        prepared.try_install() == Err(HandoffPointSinkErrorV1::Host(error)),
        "handoff must propagate the exact host refusal"
    );
    assert!(
        prepared.writer.committed == old
            && prepared.staged == staged
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
        prepared.writer.committed == expected.desired
            && prepared.staged.is_none()
            && prepared.writer.host.calls == 2
            && prepared.writer.host.accepted == Some(expected.host_intent),
        "successful handoff must commit the complete acknowledged command exactly once"
    );
    assert!(
        prepared.try_install() == Err(HandoffPointSinkErrorV1::AlreadyInstalled)
            && prepared.writer.host.calls == 2
            && prepared.writer.committed == expected.desired,
        "a consumed handoff must never call the host again"
    );
    prepared.finish_after_session();
    assert!(
        writer.committed == expected.desired && writer.host.calls == 2,
        "finishing the handoff must preserve its acknowledged result"
    );
    kani::cover!(kind == 0, "rejected set retried");
    kani::cover!(kind == 1, "rejected revoke retried");
    kani::cover!(
        kind > 1 && old.stamp.sequence() == u64::MAX,
        "exhausted stamp still permits confirmation"
    );
    kani::cover!(
        error == HandoffPointSinkHostErrorV1::Protocol,
        "protocol refusal is retryable"
    );
}

#[kani::proof]
#[kani::unwind(4)]
fn stale_prepared_command_never_reaches_the_host() {
    let Some(mut writer) = any_writer() else {
        return;
    };
    let old = writer.committed;
    let Some(paint) = any_paint() else {
        return;
    };
    let points = [patch(writer.owned_scope[0], paint)];
    let Some(intent) = valid_intent(old, &points, kani::any(), kani::any()) else {
        return;
    };
    let Some(changed) = any_stamp() else {
        return;
    };
    if changed == old.stamp {
        return;
    }
    let mut prepared = writer.prepare(intent).expect("valid exact intent prepares");
    // Hostile внутренний drift: обычное эксклюзивное Rust-заимствование его
    // запрещает, но install обязан самостоятельно проверить ожидаемый stamp.
    prepared.writer.committed.stamp = changed;
    let before = prepared.writer.committed;
    let pending = prepared.staged;
    assert!(
        prepared.try_install() == Err(HandoffPointSinkErrorV1::StampMismatch)
            && prepared.writer.host.calls == 0
            && prepared.writer.host.accepted.is_none()
            && prepared.writer.committed == before
            && prepared.staged == pending,
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
