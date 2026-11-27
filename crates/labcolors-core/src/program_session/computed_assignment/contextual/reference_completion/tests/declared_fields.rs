use super::*;
use crate::observation::Revision;
use crate::program_wire::ProgramDeclaredFieldV1;

const UNEQUAL: [&[[u8; 3]]; 2] = [&[[241; 3], [240; 3]], &[[250; 3], [247; 3], [249; 3]]];

/// Independent exact-rational oracle. Visit every half-integer crossing and
/// one rational point between crossings, including singleton mixed-direction
/// ties. It never uses the production partition, witnesses or retained support.
fn support(stops: &[[u8; 3]]) -> Vec<Srgb8> {
    let mut colors = std::collections::BTreeSet::new();
    for pair in stops.windows(2) {
        let [from, to] = [pair[0], pair[1]];
        let mut cuts = vec![(0_i64, 1_i64), (1, 1)];
        for channel in 0..3 {
            let a = i64::from(from[channel]);
            let b = i64::from(to[channel]);
            for byte in a.min(b)..a.max(b) {
                let mut n = 2 * byte + 1 - 2 * a;
                let mut d = 2 * (b - a);
                if d < 0 {
                    n = -n;
                    d = -d;
                }
                cuts.push((n, d));
            }
        }
        cuts.sort_unstable_by(|&(a, b), &(c, d)| (a * d).cmp(&(c * b)));
        cuts.dedup_by(|(a, b), (c, d)| *a * *d == *c * *b);
        let midpoints: Vec<_> = cuts
            .windows(2)
            .map(|w| (w[0].0 * w[1].1 + w[1].0 * w[0].1, 2 * w[0].1 * w[1].1))
            .collect();
        for (n, d) in cuts.into_iter().chain(midpoints) {
            colors.insert(Srgb8::new(std::array::from_fn(|i| {
                let value = i64::from(from[i]) * d + (i64::from(to[i]) - i64::from(from[i])) * n;
                ((2 * value + d) / (2 * d)) as u8
            })));
        }
    }
    colors.into_iter().collect()
}

fn cartesian(stops: [&[[u8; 3]]; 2]) -> Vec<[Srgb8; 2]> {
    let first = support(stops[0]);
    let second = support(stops[1]);
    first
        .into_iter()
        .flat_map(|a| second.iter().map(move |&b| [a, b]))
        .collect()
}

fn commit_fields(
    session: &mut Session<ProgramSessionPlan<Wcag22Srgb8V1>>,
    revision: u64,
    stops: [&[[u8; 3]]; 2],
) -> Result<(), String> {
    let positions: Vec<_> = stops
        .iter()
        .flat_map(|field| (0..field.len()).map(|i| i as f64 / (field.len() - 1) as f64))
        .collect();
    let colors: Vec<_> = stops
        .iter()
        .flat_map(|field| field.iter().flatten().copied())
        .collect();
    let offsets = [0, stops[0].len() as u32, positions.len() as u32];
    // Both varying slots deliberately contain unreachable caller placeholders.
    let field = ProgramDeclaredFieldV1::from_independent_gradients_rgb8(
        &[0; 6],
        &[0, 1],
        &offsets,
        &positions,
        &colors,
    )
    .map_err(fixture_error)?;
    session
        .prepare_schema_ordered_field(Revision::new(revision), &field)
        .map_err(fixture_error)?
        .commit();
    Ok(())
}

fn commit_cartesian(
    session: &mut Session<ProgramSessionPlan<Wcag22Srgb8V1>>,
    cases: &[[Srgb8; 2]],
) -> Result<(), String> {
    let mut input = update(1, 255);
    let ObservationPayloadInput::Scenarios(set) = &mut input.payload else {
        panic!()
    };
    set.scenarios = cases
        .iter()
        .enumerate()
        .map(|(i, values)| ScenarioInput {
            id: ScenarioId::new(i as u32),
            bindings: [2, 12]
                .into_iter()
                .zip(values)
                .map(|(port, &value)| {
                    SurfaceInputBinding::new(
                        SurfaceInputPortId::new(port),
                        ColorSignal::from_srgb8(value),
                    )
                })
                .collect(),
        })
        .collect();
    session.commit(input).map_err(fixture_error)?;
    Ok(())
}

fn field_program(nested: bool, tone: bool) -> Result<Program<Wcag22Srgb8V1>, String> {
    let mut p = program(
        target(10, 64, 64, 66, 0),
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22Srgb8V1,
    );
    p.observation_group
        .surface_input_ports
        .push(SurfaceInputPortId::new(12));
    p.surfaces.push(Surface::Input {
        id: SurfaceId::new(14),
        input: SurfaceInputPortId::new(12),
    });
    p.targets.push(target(20, 80, 80, 82, 1));
    p.paints.push(Paint::Solid {
        id: PaintId::new(23),
        target: TargetId::new(20),
    });
    p.occurrences.push(Occurrence::new(
        OccurrenceId::new(25),
        PaintId::new(23),
        SurfaceId::new(14),
        CompositionProfile::EncodedSrgb8SourceOverV1,
        p.occurrences[0].context,
    ));
    p.constraints
        .hard
        .push(ConstraintInvocation::visible_unary_hard(
            ConstraintId::new(26),
            OccurrenceId::new(25),
            Wcag22CriterionV1::Sc143TextDefault,
        ));
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(27), PaintId::new(23)));
    // A source-only objective has one fixed metric row, even with many fields.
    p.targets.push(target(30, 50, 50, 52, 2));
    p.paints.push(Paint::Solid {
        id: PaintId::new(33),
        target: TargetId::new(30),
    });
    p.outputs
        .push(OutputBinding::new(OutputSlotId::new(37), PaintId::new(33)));
    for (index, occurrence, input) in [(0, 5, 2), (1, 25, 12)] {
        let TargetIntentV1::Computed(value) = &mut p.targets[index].intent else {
            panic!()
        };
        value.intent = Some(ColorUseIntentV1 {
            identity: ColorIdentityDomainV1::new(value.anchor, true, IdentityOptionsV1::default())
                .map_err(fixture_error)?,
            appearance: Some(OccurrenceId::new(occurrence)),
            tone: tone.then(|| ToneCurveV1::new(input, vec![[0.0, 0.5], [1.0, 0.25]]).unwrap()),
        });
    }
    if nested {
        for (input, source, paint, occurrence, alpha) in
            [(1, 3, 11, 0, 0.875), (2, 23, 21, 1, 0.9375)]
        {
            p.opacities
                .push(OpacityInput::new(OpacityInputId::new(input), alpha));
            p.paints.push(Paint::Opacity {
                id: PaintId::new(paint),
                source: PaintId::new(source),
                opacity: OpacityInputId::new(input),
            });
            p.occurrences[occurrence].subject = PaintId::new(paint);
        }
        p.sources.push(Source::new(
            SourceId::new(90),
            ColorSignal::from_srgb8(Srgb8::new([210; 3])),
        ));
        p.targets
            .push(Target::fixed(TargetId::new(90), SourceId::new(90)));
        p.paints.push(Paint::Solid {
            id: PaintId::new(93),
            target: TargetId::new(90),
        });
        p.opacities
            .push(OpacityInput::new(OpacityInputId::new(90), 0.5));
        p.paints.push(Paint::Opacity {
            id: PaintId::new(94),
            source: PaintId::new(93),
            opacity: OpacityInputId::new(90),
        });
        p.occurrences.push(Occurrence::new(
            OccurrenceId::new(95),
            PaintId::new(94),
            SurfaceId::new(14),
            CompositionProfile::EncodedSrgb8SourceOverV1,
            p.occurrences[0].context,
        ));
        p.surfaces.push(Surface::FromOccurrence {
            id: SurfaceId::new(94),
            occurrence: OccurrenceId::new(95),
        });
        p.occurrences[1].against = SurfaceId::new(94);
    }
    Ok(p)
}

fn decode(channel: f64) -> f64 {
    let value = channel / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

// Independent binary64 implementation of Ottosson's 2021 matrices. This
// regression oracle's tolerance never participates in production admission.
fn lab(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(decode);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum()
}

fn blend(source: [f64; 3], alpha: f64, backdrop: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| alpha * source[i] + (1.0 - alpha) * backdrop[i])
}

fn appearance(
    source: [f64; 3],
    input: Srgb8,
    index: usize,
    nested: bool,
    bytes: bool,
) -> ([f64; 3], [f64; 3]) {
    let mut backdrop = input.bytes().map(f64::from);
    if nested && index == 1 {
        backdrop = blend([210.0; 3], 0.5, backdrop);
        if bytes {
            backdrop = backdrop.map(f64::round);
        }
    }
    let mut visible = blend(
        source,
        if nested { [0.875, 0.9375][index] } else { 1.0 },
        backdrop,
    );
    if bytes {
        visible = visible.map(f64::round);
    }
    (visible, backdrop)
}

fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let luminance = |rgb: [f64; 3]| {
        let [r, g, b] = rgb.map(decode);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn finite_best(ideal: &[ContinuousSrgbV1], cases: &[[Srgb8; 2]], nested: bool) -> [Srgb8; 3] {
    let mut choices = Vec::new();
    for first in 64..=66 {
        for second in 80..=82 {
            for fixed in 50..=52 {
                let sources = [first, second, fixed].map(|v| Srgb8::new([v; 3]));
                if !cases.iter().all(|case| {
                    (0..2).all(|i| {
                        let (visible, backdrop) =
                            appearance(sources[i].bytes().map(f64::from), case[i], i, nested, true);
                        contrast(visible, backdrop) >= 4.5
                    })
                }) {
                    continue;
                }
                let mut profile: Vec<_> = sources
                    .iter()
                    .zip(ideal)
                    .map(|(source, goal)| {
                        distance(lab(source.bytes().map(f64::from)), lab(goal.channels()))
                    })
                    .collect();
                profile.sort_by(|a, b| b.total_cmp(a));
                choices.push((profile, sources));
            }
        }
    }
    choices.sort_by(|a, b| {
        a.0.iter()
            .zip(&b.0)
            .map(|(a, b)| a.total_cmp(b))
            .find(|o| !o.is_eq())
            .unwrap_or(Ordering::Equal)
    });
    assert!(choices.len() > 1);
    assert!(
        choices[1].0[0] - choices[0].0[0] > 1e-7,
        "oracle fixture must have a well-separated optimum"
    );
    choices[0].1
}

fn same_measurement(
    actual: &CompletionPreferenceEvidenceV1,
    expected: &CompletionPreferenceEvidenceV1,
) {
    assert_eq!(actual.target, expected.target);
    assert_eq!(actual.occurrence, expected.occurrence);
    assert_eq!(actual.visible_lab, expected.visible_lab);
    assert_eq!(actual.backdrop_lab, expected.backdrop_lab);
    assert_eq!(actual.preferred, expected.preferred);
    assert_eq!(
        actual.distance.as_ref().map(|v| v.bounds),
        expected.distance.as_ref().map(|v| v.bounds)
    );
}

#[test]
fn unequal_supports_match_cartesian_metrics_and_exhaustive_rgb8_oracle() -> Result<(), String> {
    for nested in [false, true] {
        for tone in [false, true] {
            let p = field_program(nested, tone)?;
            let (owner, mut field_session) = fixture_session(p, 64)?;
            commit_fields(&mut field_session, 1, UNEQUAL)?;
            let cases = cartesian(UNEQUAL);
            assert_eq!(cases.len(), 8);
            let mut finite_session = owner
                .instantiate(ObservationStreamId::new(8))
                .map_err(fixture_error)?;
            commit_cartesian(&mut finite_session, &cases)?;
            let mut field = owner
                .prepare_reference_completion(&field_session)
                .map_err(fixture_error)?;
            let mut finite = owner
                .prepare_reference_completion(&finite_session)
                .map_err(fixture_error)?;
            let anchor = [point([66.0; 3]), point([82.0; 3]), point([52.0; 3])];
            let proposal = [point([64.25; 3]), point([80.25; 3]), point([50.25; 3])];
            PREFERENCE_ROW_VISITS.with(|value| value.set(0));
            let actual = field
                .realize(&anchor, &proposal, 64)
                .map_err(fixture_error)?;
            // An opaque source with no tone has a context-independent
            // objective even when its contrast constraint reads the field.
            let rows = if nested || tone { 7 } else { 3 };
            assert_eq!(PREFERENCE_ROW_VISITS.with(core::cell::Cell::get), rows);
            let expected = finite
                .realize(&anchor, &proposal, 64)
                .map_err(fixture_error)?;
            assert_eq!(
                actual.admission(),
                CompletionAdmissionV1::PreferenceImproved
            );
            assert_eq!(actual.admission(), expected.admission());
            assert_eq!(actual.ideal(), proposal);
            assert_eq!(actual.emitted(), expected.emitted());
            assert_eq!(
                actual.emitted(),
                Some(finite_best(&proposal, &cases, nested).as_slice())
            );
            assert!(actual.rgb8_search_exhausted());
            assert!(!actual.numerical_order_unresolved());
            assert_eq!(actual.visited(), 9);
            assert_eq!(
                actual.continuous_optimality(),
                ComputedOptimalityV1::Unfinished
            );
            assert_eq!(
                actual
                    .use_tones()
                    .iter()
                    .map(|v| v.cases.clone())
                    .collect::<Vec<_>>(),
                if nested || tone {
                    vec![0..2, 2..6, 0..1]
                } else {
                    vec![0..1, 0..1, 0..1]
                }
            );
            for (a, b, values) in [
                (
                    &actual.point_evidence().0,
                    &expected.point_evidence().0,
                    &anchor,
                ),
                (
                    &actual.point_evidence().1.unwrap(),
                    &expected.point_evidence().1.unwrap(),
                    &proposal,
                ),
            ] {
                assert_eq!(a.preferences.len(), rows);
                assert_eq!(b.preferences.len(), 24);
                assert_eq!(a.source_profile, b.source_profile);
                for row in &a.preferences {
                    let index = if row.target == TargetId::new(10) {
                        0
                    } else if row.target == TargetId::new(20) {
                        1
                    } else {
                        2
                    };
                    let inputs = actual.observation().physical_values(row.case).unwrap();
                    let expected_row = b
                        .preferences
                        .iter()
                        .find(|other| {
                            other.target == row.target
                                && (index == 2
                                    || expected.observation().physical_values(other.case).unwrap()
                                        [index]
                                        .srgb8()
                                        == inputs[index].srgb8())
                        })
                        .ok_or("marginal row absent from Cartesian oracle")?;
                    same_measurement(row, expected_row);
                    let visible = if index < 2 {
                        appearance(
                            values[index].channels(),
                            inputs[index].srgb8(),
                            index,
                            nested,
                            false,
                        )
                        .0
                    } else {
                        values[index].channels()
                    };
                    let mut preferred = lab([[64.0; 3], [80.0; 3], [50.0; 3]][index]);
                    if tone && index < 2 {
                        let curve = ToneCurveV1::new([2, 12][index], vec![[0.0, 0.5], [1.0, 0.25]])
                            .map_err(fixture_error)?;
                        preferred[0] = curve.preferred_lightness(inputs[index].srgb8());
                        let local = row.case - actual.use_tones()[index].cases.start;
                        assert_eq!(
                            actual.use_tones()[index].per_case_bits[local],
                            Some(preferred[0].to_bits())
                        );
                    }
                    let scalar = row.distance.as_ref().unwrap().cell.map_err(fixture_error)?;
                    assert!(
                        (f64::from_bits(scalar.cell_bits) - distance(lab(visible), preferred))
                            .abs()
                            < 4e-14
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn mixed_direction_singleton_is_included_in_full_completion() -> Result<(), String> {
    let stops: [&[[u8; 3]]; 2] = [&[[240, 241, 240], [241, 240, 240]], UNEQUAL[1]];
    assert_eq!(
        support(stops[0]),
        vec![
            Srgb8::new([240, 241, 240]),
            Srgb8::new([241, 240, 240]),
            Srgb8::new([241, 241, 240])
        ]
    );
    let (owner, mut session) = fixture_session(field_program(true, true)?, 64)?;
    commit_fields(&mut session, 1, stops)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let values = [point([65.0; 3]), point([81.0; 3]), point([51.0; 3])];
    let result = prepared
        .realize(&values, &values, 64)
        .map_err(fixture_error)?;
    let rows: Vec<_> = result
        .point_evidence()
        .0
        .preferences
        .iter()
        .filter(|row| row.target == TargetId::new(10))
        .collect();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| result.observation().physical_values(row.case).unwrap()[0].srgb8())
            .collect::<Vec<_>>(),
        support(stops[0])
    );
    assert_eq!(result.use_tones()[1].cases, 3..7);
    let cases = cartesian(stops);
    assert_eq!(
        result.emitted(),
        Some(finite_best(&values, &cases, true).as_slice())
    );
    Ok(())
}

#[test]
fn changed_field_supports_refresh_reused_scratch_and_preserve_old_evidence() -> Result<(), String> {
    let (owner, mut session) = fixture_session(field_program(true, true)?, 64)?;
    commit_fields(&mut session, 1, UNEQUAL)?;
    let values = [point([65.0; 3]), point([81.0; 3]), point([51.0; 3])];
    let (scratch, old) = {
        let mut prepared = owner
            .prepare_reference_completion(&session)
            .map_err(fixture_error)?;
        let old = prepared
            .realize(&values, &values, 64)
            .map_err(fixture_error)?;
        (prepared.scratch, old)
    };
    let uses = scratch.storage.as_ref().unwrap().uses.as_ptr();
    let old_measurement = old.point_evidence().0.preferences[2].visible_lab;
    let changed: [&[[u8; 3]]; 2] = [&[[244; 3], [241; 3]], &[[252; 3], [252; 3]]];
    commit_fields(&mut session, 2, changed)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    prepared.scratch = scratch;
    let new = prepared
        .realize(&values, &values, 64)
        .map_err(fixture_error)?;
    assert_eq!(
        prepared.scratch.storage.as_ref().unwrap().uses.as_ptr(),
        uses
    );
    assert_eq!(new.observation().revision().value(), 2);
    assert_eq!(
        new.use_tones()
            .iter()
            .map(|v| v.cases.clone())
            .collect::<Vec<_>>(),
        vec![0..4, 4..5, 0..1]
    );
    assert_eq!(new.point_evidence().0.preferences.len(), 6);
    assert_eq!(old.observation().revision().value(), 1);
    assert_eq!(old.use_tones()[1].cases, 2..6);
    assert_eq!(
        old.point_evidence().0.preferences[2].visible_lab,
        old_measurement
    );
    let new_second = new
        .point_evidence()
        .0
        .preferences
        .iter()
        .find(|row| row.target == TargetId::new(20))
        .unwrap();
    assert_ne!(new_second.visible_lab, old_measurement);
    assert_eq!(
        new.emitted(),
        Some(finite_best(&values, &cartesian(changed), true).as_slice())
    );
    Ok(())
}

#[test]
fn complete_marginal_scan_rejects_an_infeasible_interior_case() -> Result<(), String> {
    let mut p = field_program(false, false)?;
    p.targets[1] = target(20, 0, 0, 2, 1);
    let stops: [&[[u8; 3]]; 2] = [UNEQUAL[0], &[[119; 3], [116; 3], [119; 3]]];
    let (owner, mut session) = fixture_session(p, 64)?;
    commit_fields(&mut session, 1, stops)?;
    let mut prepared = owner
        .prepare_reference_completion(&session)
        .map_err(fixture_error)?;
    let values = [point([64.0; 3]), point([0.0; 3]), point([50.0; 3])];
    let result = prepared
        .realize(&values, &values, 64)
        .map_err(fixture_error)?;
    assert!(contrast([0.0; 3], [119.0; 3]) > 4.5);
    for source in 0..=2 {
        assert!(contrast([f64::from(source); 3], [116.0; 3]) < 4.5);
    }
    assert!(result.emitted().is_none());
    assert!(result.rgb8_search_exhausted());
    let mut finite_session = owner
        .instantiate(ObservationStreamId::new(8))
        .map_err(fixture_error)?;
    commit_cartesian(&mut finite_session, &cartesian(stops))?;
    let expected = owner
        .prepare_reference_completion(&finite_session)
        .map_err(fixture_error)?
        .realize(&values, &values, 64)
        .map_err(fixture_error)?;
    assert_eq!(result.emitted(), expected.emitted());
    Ok(())
}
