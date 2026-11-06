//! Полный field payload использует тот же атомарный observation owner.
use super::*;
pub(crate) fn empty_field_schema() -> CanonicalObservationSchemaV1 {
    CanonicalObservationSchemaV1(Shared::from(Vec::new().into_boxed_slice()))
}
pub(crate) fn prepare_field_observation<'owner, Owner: ObservationOwnerV1>(
    owner: &'owner mut Owner,
    arenas: &mut ObservationArenaPoolV1,
    stream: ObservationStreamId,
    schema: &CanonicalObservationSchemaV1,
    revision: Revision,
    payload: Shared<crate::field_effect::final_raster::FieldObservedPayloadV1>,
) -> Result<PreparedObservationUpdateV1<'owner, Owner>, ObservationError> {
    if !schema.as_slice().is_empty() || !arenas.shares_schema_backing_with(schema) {
        return Err(ObservationError::InternalInvariant);
    }
    let current = owner.observation_head().revision();
    if let Some(current) = current {
        if revision < current {
            return Err(ObservationError::RevisionOutOfOrder {
                current,
                incoming: revision,
            });
        }
    }
    if current == Some(revision) {
        let exact = matches!(owner.observation_head(), ObservationHeadViewV1::Observed(observation)
            if observation.field_payload() == Some(payload.as_ref()));
        return if exact {
            Ok(PreparedObservationUpdateV1::Idempotent(
                PreparedIdempotentV1 { owner },
            ))
        } else {
            Err(ObservationError::RevisionConflict { revision })
        };
    }
    let backing = arenas.materialize_into(|set| {
        *set = ObservedScenarioSet::empty();
        Ok(())
    })?;
    Ok(PreparedObservationUpdateV1::Observed(PreparedObservedV1 {
        owner,
        observation: RevisionBoundObservationV1 {
            stream,
            revision,
            backing,
            field: Some(payload),
        },
    }))
}
