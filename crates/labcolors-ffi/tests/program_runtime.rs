//! Внешний Rust-потребитель тех же функций, которые экспортирует UniFFI.
use labcolors::*;
use std::sync::{Arc, Mutex, Weak};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn composite_projection_keeps_root_scenario_and_owned_snapshot() -> TestResult {
    let runtime = compile_program_wire(wire("adaptive"), 1)?;
    let first = runtime.update_observed(1, vec![101, 301], vec![255; 6], 1)?;
    assert_eq!(first.presentation_composite_rgb(71, 101)?, [0; 3]);
    assert_eq!(first.presentation_composite_rgb(71, 301)?, [0; 3]);
    let mut copied = first.presentation_composite_rgb(71, 101)?;
    copied.fill(255);
    assert_eq!(first.presentation_composite_rgb(71, 101)?, [0; 3]);
    expect_error(
        first.presentation_composite_rgb(91, 101),
        "presentation_composite_unavailable",
    );
    expect_error(
        first.presentation_composite_rgb(71, 0),
        "presentation_composite_unavailable",
    );
    let stale = runtime.update_unknown(2, 7)?;
    expect_error(
        stale.presentation_composite_rgb(71, 101),
        "presentation_composite_unavailable",
    );
    drop(runtime);
    assert_eq!(first.presentation_composite_rgb(71, 301)?, [0; 3]);
    Ok(())
}

fn fixtures() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../conformance/program-runtime-v1.json"))
        .expect("committed Program fixtures must be valid JSON")
}
fn wire(name: &str) -> Vec<u8> {
    let fixtures = fixtures();
    let hex = fixtures[name]
        .as_str()
        .expect("named wire fixture must be a hex string");
    assert_eq!(hex.len() % 2, 0, "wire fixture must contain whole bytes");
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16).expect("wire fixture must use hexadecimal bytes")
        })
        .collect()
}
fn code(error: LabColorsError) -> String {
    let LabColorsError::Refused { code, .. } = error;
    code
}
fn expect_error<T>(result: Result<T, LabColorsError>, expected: &str) {
    match result {
        Ok(_) => panic!("expected {expected}"),
        Err(error) => assert_eq!(code(error), expected),
    }
}

#[test]
fn portable_history_and_owned_snapshots_survive_thread_migration() -> TestResult {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ProgramRuntime>();
    send_sync::<ProgramAttachment>();
    send_sync::<ProgramSnapshot>();
    let runtime = compile_program_wire(wire("adaptive"), 1)?;
    let first = runtime.update_observed(1, vec![1], vec![255; 3], 1)?;
    assert_eq!(first.state(), "ready");
    assert_eq!(first.output_rgb(0)?, [0; 3]);
    let worker = runtime.clone();
    let second = std::thread::spawn(move || worker.update_observed(2, vec![1], vec![0; 3], 1))
        .join()
        .expect("migrated runtime must not panic")?;
    assert_eq!(second.output_rgb(0)?, [255; 3]);
    assert_eq!(first.output_rgb(0)?, [0; 3]);
    assert_eq!(first.output_slot(0)?, 91);
    assert_eq!(first.output_opacity(0)?, 1.0);
    expect_error(first.output_rgb(1), "invalid_output_index");
    for row in fixtures()["history"]
        .as_array()
        .expect("history must be an array")
        .iter()
        .skip(2)
    {
        let revision = row["revision"]
            .as_u64()
            .expect("fixture revision must be UInt64");
        let snapshot = if let Some(reason) = row["unknownReason"].as_u64() {
            runtime.update_unknown(revision, u32::try_from(reason)?)?
        } else {
            let backgrounds = row["backgrounds"]
                .as_array()
                .expect("observed step must declare backgrounds");
            let ids = (1..=u32::try_from(backgrounds.len())?).collect();
            let mut bytes = Vec::new();
            for background in backgrounds {
                for channel in background
                    .as_array()
                    .expect("background fixture must contain RGB channels")
                {
                    bytes.push(u8::try_from(
                        channel.as_u64().expect("fixture channel must be unsigned"),
                    )?);
                }
            }
            runtime.update_observed(revision, ids, bytes, 1)?
        };
        assert_eq!(
            snapshot.state(),
            row["state"]
                .as_str()
                .expect("fixture state must be a string")
        );
        if let Some(rgb) = row.get("rgb") {
            let expected: [u8; 3] = serde_json::from_value(rgb.clone())?;
            assert_eq!(snapshot.output_rgb(0)?, expected);
        }
    }
    Ok(())
}

#[test]
fn malformed_conflicting_and_old_observations_do_not_consume_a_revision() -> TestResult {
    let runtime = compile_program_wire(wire("adaptive"), 1)?;
    expect_error(
        runtime.update_observed(1, vec![1], vec![255; 2], 1),
        "program_update",
    );
    assert_eq!(
        runtime
            .update_observed(1, vec![1], vec![255; 3], 1)?
            .state(),
        "ready"
    );
    assert_eq!(
        runtime
            .update_observed(1, vec![1], vec![255; 3], 1)?
            .state(),
        "ready"
    );
    expect_error(
        runtime.update_observed(1, vec![1], vec![0; 3], 1),
        "program_update",
    );
    assert_eq!(
        runtime
            .update_observed(2, vec![1], vec![0; 3], 1)?
            .output_rgb(0)?,
        [255; 3]
    );
    expect_error(runtime.update_unknown(1, 7), "program_update");
    expect_error(compile_program_wire(vec![0; 80], 1), "program_wire");
    Ok(())
}

#[derive(Default)]
struct HostState {
    reject: bool,
    fail: bool,
    reenter: bool,
    sequence: u64,
    installed: Option<Vec<u8>>,
    calls: usize,
}
struct Host {
    state: Arc<Mutex<HostState>>,
    owner: Arc<Mutex<Weak<ProgramAttachment>>>,
}
impl ProgramPointSinkHost for Host {
    fn try_install(
        &self,
        intent: ProgramPointSinkIntent,
    ) -> Result<bool, ProgramPointSinkCallbackError> {
        let mut state = self
            .state
            .lock()
            .expect("test host lock must not be poisoned");
        state.calls += 1;
        if state.reenter {
            let owner = self
                .owner
                .lock()
                .expect("test owner lock must not be poisoned")
                .upgrade()
                .expect("test attachment must remain alive during callback");
            expect_error(owner.update_unknown(99, 7), "program_attachment_busy");
            expect_error(owner.materialization_authority(), "program_attachment_busy");
            expect_error(owner.dispose(false), "program_attachment_busy");
        }
        assert_eq!(intent.expected_sequence, state.sequence);
        if state.fail {
            return Err(ProgramPointSinkCallbackError::InvocationFailed);
        }
        if state.reject {
            return Ok(false);
        }
        if intent.operation != ProgramPointSinkOperation::ConfirmExact {
            assert_eq!(intent.desired_sequence, intent.expected_sequence + 1);
        }
        state.sequence = intent.desired_sequence;
        state.installed = intent.point.map(|point| point.source);
        Ok(true)
    }
}

#[test]
fn host_rejection_reentry_authority_and_dispose_preserve_one_committed_head() -> TestResult {
    let state = Arc::new(Mutex::new(HostState::default()));
    let owner = Arc::new(Mutex::new(Weak::new()));
    let attachment = attach_program_wire(
        wire("adaptive"),
        1,
        91,
        501,
        71,
        61,
        Box::new(Host {
            state: state.clone(),
            owner: owner.clone(),
        }),
    )?;
    *owner.lock().expect("test owner lock must not be poisoned") = Arc::downgrade(&attachment);
    state
        .lock()
        .expect("test host lock must not be poisoned")
        .reenter = true;
    let first = attachment.update_observed(1, vec![1], vec![255; 3], 1)?;
    let render = first
        .render()
        .expect("ready attached snapshot must expose its installed render");
    let authority = attachment.materialization_authority()?;
    assert_eq!(authority.terminal_composite_rgb(), [0; 3]);
    assert_eq!(authority.renderer_provenance()?, "unverified");
    assert_eq!(render.sink_output(), 501);
    assert_eq!(render.content_identity(), authority.content_identity());
    assert_eq!(render.context_identity(), authority.context_identity());
    state
        .lock()
        .expect("test host lock must not be poisoned")
        .fail = true;
    expect_error(
        attachment.update_observed(2, vec![1], vec![0; 3], 1),
        "program_attachment_host_protocol",
    );
    assert_eq!(attachment.materialization_authority()?.revision(), 1);
    {
        let mut host = state.lock().expect("test host lock must not be poisoned");
        assert_eq!(host.installed, Some(vec![0; 3]));
        host.fail = false;
        host.reject = true;
    }
    expect_error(
        attachment.update_observed(2, vec![1], vec![0; 3], 1),
        "program_attachment_host_rejected",
    );
    assert_eq!(attachment.materialization_authority()?.revision(), 1);
    {
        let mut host = state.lock().expect("test host lock must not be poisoned");
        assert_eq!(host.installed, Some(vec![0; 3]));
        host.reject = false;
    }
    assert_eq!(
        attachment
            .update_observed(2, vec![1], vec![0; 3], 1)?
            .output_rgb(0)?,
        [255; 3]
    );
    expect_error(
        attachment.materialization_authority_for(
            render.revision(),
            render.content_identity(),
            render.binding_epoch(),
        ),
        "program_materialization_stale_revision",
    );
    assert_eq!(authority.terminal_composite_rgb(), [0; 3]);
    expect_error(
        attachment.dispose(false),
        "program_attachment_revoke_unconfirmed",
    );
    assert_eq!(attachment.materialization_authority()?.revision(), 2);
    state
        .lock()
        .expect("test host lock must not be poisoned")
        .installed = None;
    attachment.dispose(true)?;
    expect_error(
        attachment.update_unknown(3, 7),
        "program_attachment_already_disposed",
    );
    expect_error(
        attachment.dispose(true),
        "program_attachment_already_disposed",
    );
    assert!(
        state
            .lock()
            .expect("test host lock must not be poisoned")
            .calls
            >= 4
    );
    Ok(())
}

#[test]
fn metadata_is_projected_from_core_and_untrusted_envelopes_remain_untrusted() {
    let got = numerical_capability_manifest();
    let reference = labcolors_core::numerical_capability_manifest_v2();
    assert_eq!(got.schema_version, reference.schema_version);
    assert_eq!(got.checksum, reference.checksum.hex());
    assert_eq!(got.sites.len(), reference.sites.len());
    expect_error(
        decode_certificate_envelope(vec![0; 20]),
        "certificate_invalid_magic",
    );
}

// Байты объявлены публичным ProgramWireBuilderV1; численный expected не
// генерируется движком. Swift/Kotlin ниже объявляют тот же граф своими builders.
fn assessment_wire(name: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    let hex = match name {
        "rejected" => concat!(
            "4c4350570100db000000010000000b00000000c7400100000015000000010b00000000000000000000000100",
            "00001f0000000100000020000000000000000000f03f020000002a000000011500000029000000022a000000",
            "200000000100000033000000011f000000010000003d000000290000003300000000000000000050409a9999",
            "999999c93f0101000000470000003d00000001000000470000003d0000000100000052000000013d00000000",
            "c7400200000053000000013d000000ffffff510000000a470000003d000000010000005b00000029000000",
        ),
        "alpha" => concat!(
            "4c4350570100db000000010000000b0000008080810100000015000000010b00000000000000000000000100",
            "00001f0000000100000020000000000000000000e03f020000002a000000011500000029000000022a000000",
            "200000000100000033000000011f000000010000003d000000290000003300000000000000000050409a9999",
            "999999c93f0101000000470000003d00000001000000470000003d0000000100000052000000013d00000080",
            "80800200000053000000013d000000ffffff510000000a470000003d000000010000005b00000029000000",
        ),
        "absent" => concat!(
            "4c4350570100db000000010000000b0000000000000100000015000000010b00000000000000000000000100",
            "00001f0000000100000020000000000000000000f03f020000002a000000011500000029000000022a000000",
            "200000000100000033000000011f000000010000003d000000290000003300000000000000000050409a9999",
            "999999c93f0101000000470000003d00000001000000470000003d0000000100000052000000013d00000000",
            "00000200000053000000013d000000ffffff510000000a470000003d000000010000005b00000029000000",
        ),
        _ => panic!("unknown assessment wire"),
    };
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect()
}

#[test]
fn native_assessment_report_preserves_duplicate_provenance_and_every_cell() -> TestResult {
    let mut held = None;
    // Перестановка одинаковых наблюдений не меняет case и не теряет scenario id.
    for ids in [vec![9, 7], vec![7, 9]] {
        let runtime = compile_program_wire(assessment_wire("rejected")?, u32::MAX)?;
        let snapshot = runtime.update_observed(u64::MAX, ids, vec![0; 6], 1)?;
        assert_eq!(snapshot.state(), "ready");
        assert_eq!(snapshot.output_slot(0)?, 91);
        assert_eq!(snapshot.output_rgb(0)?, [0, 199, 64]);
        assert_eq!(snapshot.output_opacity(0)?, 1.0);
        let report = snapshot.assessment_report()?.ok_or("Ready has a report")?;
        assert_eq!(report.content_identity.len(), 32);
        assert_eq!(report.stream_id, u32::MAX);
        assert_eq!(report.revision, u64::MAX);
        assert_eq!(
            report.scenario_provenance,
            vec![
                ProgramScenarioProvenance {
                    case_index: 0,
                    scenario_id: 7
                },
                ProgramScenarioProvenance {
                    case_index: 0,
                    scenario_id: 9
                },
            ]
        );
        assert_eq!(
            report.cells,
            vec![
                ProgramAssessmentCell {
                    constraint_id: 81,
                    case_index: 0,
                    hard: false,
                    assessment: ProgramConstraintAssessment::DeclaredSrgb8CleanSetRejected {
                        visible: ProgramAssessmentSrgb8 {
                            red: 0,
                            green: 199,
                            blue: 64
                        },
                        rejected_blue_interval: ProgramAssessmentBlueInterval {
                            lower: 64,
                            upper: 108
                        },
                    },
                },
                ProgramAssessmentCell {
                    constraint_id: 82,
                    case_index: 0,
                    hard: true,
                    assessment: ProgramConstraintAssessment::ExactSrgb8 {
                        verdict: ProgramAssessmentVerdict::Pass
                    },
                },
                // Итоговый [0,199,64] нарушает report-only равенство белому.
                ProgramAssessmentCell {
                    constraint_id: 83,
                    case_index: 0,
                    hard: false,
                    assessment: ProgramConstraintAssessment::ExactSrgb8 {
                        verdict: ProgramAssessmentVerdict::Violation
                    },
                },
            ]
        );
        if let Some(previous) = held {
            assert_eq!(report, previous);
        }
        held = Some(report);
    }
    Ok(())
}

#[test]
fn native_assessment_report_follows_final_alpha_and_never_promotes_retained_paint() -> TestResult {
    let runtime = compile_program_wire(assessment_wire("alpha")?, 100)?;
    let waiting = runtime.update_unknown(1, 99)?;
    assert_eq!(waiting.state(), "waiting");
    assert!(waiting.assessment_report()?.is_none());
    let first = runtime.update_observed(2, vec![7], vec![128, 128, 127], 1)?;
    assert_eq!(first.state(), "ready");
    let report = first
        .assessment_report()?
        .ok_or("observed Ready has a report")?;
    assert_eq!(report.cells[2].constraint_id, 83);
    assert!(!report.cells[2].hard);
    assert_eq!(
        report.cells[2].assessment,
        ProgramConstraintAssessment::ExactSrgb8 {
            verdict: ProgramAssessmentVerdict::Violation
        }
    );
    // В encoded sRGB (129 + 127)/2 = 128; source 129 не является финальным цветом.
    assert_eq!(
        report.cells[0].assessment,
        ProgramConstraintAssessment::DeclaredSrgb8CleanSetPass {
            visible: ProgramAssessmentSrgb8 {
                red: 128,
                green: 128,
                blue: 128
            },
        }
    );
    assert_eq!(first.output_rgb(0)?, [128, 128, 129]);
    assert_eq!(first.output_opacity(0)?, 0.5);
    let stale = runtime.update_unknown(3, 99)?;
    assert_eq!(stale.state(), "stale");
    assert!(stale.assessment_report()?.is_none());
    assert_eq!(stale.output_rgb(0)?, first.output_rgb(0)?);
    let fresh = runtime.update_observed(4, vec![7], vec![128, 128, 127], 1)?;
    assert_eq!(fresh.state(), "ready");
    assert_eq!(
        fresh.assessment_report()?.ok_or("fresh report")?.revision,
        4
    );
    let failed = runtime.update_observed(5, vec![7], vec![0; 3], 1)?;
    assert_eq!(failed.state(), "failed");
    assert!(failed.assessment_report()?.is_none());
    assert_eq!(failed.output_rgb(0)?, first.output_rgb(0)?);
    let recovered = runtime.update_observed(6, vec![7], vec![128, 128, 127], 1)?;
    assert_eq!(recovered.state(), "ready");
    assert_eq!(
        recovered
            .assessment_report()?
            .ok_or("recovered report")?
            .revision,
        6
    );
    assert_eq!(
        recovered
            .assessment_report()?
            .ok_or("recovered report")?
            .content_identity,
        report.content_identity
    );
    let other = compile_program_wire(assessment_wire("alpha")?, 101)?.update_observed(
        2,
        vec![7],
        vec![128, 128, 127],
        1,
    )?;
    let other_report = other
        .assessment_report()?
        .ok_or("independent stream report")?;
    assert_eq!(other_report.content_identity, report.content_identity);
    assert_eq!(other_report.stream_id, 101);
    let different = compile_program_wire(assessment_wire("rejected")?, 100)?.update_observed(
        2,
        vec![7],
        vec![0; 3],
        1,
    )?;
    assert_ne!(
        different
            .assessment_report()?
            .ok_or("different graph report")?
            .content_identity,
        report.content_identity
    );
    let mut copied = first.assessment_report()?.ok_or("owned report copy")?;
    copied.content_identity[0] ^= 1;
    copied.stream_id = 0;
    copied.revision = 99;
    copied.scenario_provenance[0].scenario_id = 0;
    copied.cells.clear();
    assert_ne!(copied, report);
    assert_eq!(first.assessment_report()?, Some(report.clone()));
    drop(runtime);
    assert_eq!(first.assessment_report()?, Some(report));
    Ok(())
}

#[test]
fn native_assessment_report_distinguishes_final_owned_domain_absence() -> TestResult {
    let snapshot = compile_program_wire(assessment_wire("absent")?, 102)?.update_observed(
        1,
        vec![7],
        vec![0; 3],
        1,
    )?;
    assert_eq!(snapshot.state(), "ready");
    let report = snapshot
        .assessment_report()?
        .ok_or("report-only absence retains Ready")?;
    assert_eq!(report.cells.len(), 3);
    assert_eq!(report.cells[0].constraint_id, 81);
    assert!(!report.cells[0].hard);
    assert_eq!(
        report.cells[0].assessment,
        ProgramConstraintAssessment::DeclaredSrgb8CleanSetFinalOwnedDomainAbsent
    );
    assert_eq!(
        report.cells[1].assessment,
        ProgramConstraintAssessment::ExactSrgb8 {
            verdict: ProgramAssessmentVerdict::Pass
        }
    );
    Ok(())
}

#[test]
fn native_attached_report_reads_do_not_install_or_reissue_authority() -> TestResult {
    let state = Arc::new(Mutex::new(HostState::default()));
    let owner = Arc::new(Mutex::new(Weak::new()));
    let attachment = attach_program_wire(
        wire("adaptive"),
        103,
        91,
        501,
        71,
        61,
        Box::new(Host {
            state: state.clone(),
            owner,
        }),
    )?;
    let first = attachment.update_observed(1, vec![7], vec![255; 3], 1)?;
    assert_eq!(first.state(), "ready");
    let render = first.render().ok_or("Ready installs a render")?;
    let before = state.lock().map_err(|_| "host state")?.calls;
    let report = first.assessment_report()?.ok_or("attached Ready report")?;
    assert_eq!(report.content_identity, render.content_identity());
    assert_eq!(report.stream_id, 103);
    assert_eq!(report.revision, render.revision());
    assert_eq!(
        report.cells,
        vec![ProgramAssessmentCell {
            constraint_id: 81,
            case_index: 0,
            hard: true,
            assessment: ProgramConstraintAssessment::Wcag22Srgb8 {
                verdict: ProgramAssessmentVerdict::Pass
            },
        }]
    );
    assert_eq!(state.lock().map_err(|_| "host state")?.calls, before);
    state.lock().map_err(|_| "host state")?.reject = true;
    expect_error(
        attachment.update_observed(2, vec![7], vec![0; 3], 1),
        "program_attachment_host_rejected",
    );
    assert_eq!(first.assessment_report()?, Some(report.clone()));
    assert_eq!(attachment.materialization_authority()?.revision(), 1);
    state.lock().map_err(|_| "host state")?.reject = false;
    let revoked = attachment.update_unknown(2, 99)?;
    assert_eq!(revoked.state(), "stale");
    assert!(revoked.assessment_report()?.is_none());
    assert!(!revoked.has_render());
    assert!(state.lock().map_err(|_| "host state")?.installed.is_none());
    expect_error(
        attachment.materialization_authority_for(
            report.revision,
            report.content_identity.clone(),
            render.binding_epoch(),
        ),
        "program_materialization_not_ready",
    );
    let before = state.lock().map_err(|_| "host state")?.calls;
    let mut copied = first
        .assessment_report()?
        .ok_or("historical report after revoke")?;
    copied.content_identity.clear();
    copied.scenario_provenance.clear();
    copied.cells.clear();
    assert_ne!(copied, report);
    assert_eq!(first.assessment_report()?, Some(report.clone()));
    assert_eq!(state.lock().map_err(|_| "host state")?.calls, before);
    let failed = attachment.update_observed(3, vec![7, 9], vec![0, 0, 0, 255, 255, 255], 1)?;
    assert_eq!(failed.state(), "failed");
    assert!(failed.assessment_report()?.is_none());
    assert!(!failed.has_render());
    assert_eq!(first.assessment_report()?, Some(report));
    attachment.dispose(true)?;
    Ok(())
}

#[derive(Default)]
struct RasterHostState {
    calls: usize,
    reject: bool,
    fail: bool,
    reenter: bool,
    received: Vec<ProgramFinalRasterBytes>,
}
struct RasterHost {
    state: Arc<Mutex<RasterHostState>>,
    owner: Weak<ProgramRuntime>,
}
impl ProgramFinalRasterConsumer for RasterHost {
    fn consume(
        &self,
        raster: ProgramFinalRasterBytes,
    ) -> Result<bool, ProgramFinalRasterCallbackError> {
        let mut state = self.state.lock().expect("raster host state");
        state.calls += 1;
        if state.reenter {
            let owner = self
                .owner
                .upgrade()
                .expect("runtime remains alive during callback");
            expect_error(owner.update_unknown(99, 7), "program_attachment_busy");
            expect_error(
                owner.final_raster_content_identity(),
                "program_attachment_busy",
            );
            expect_error(owner.final_raster_refusal(), "program_attachment_busy");
        }
        if state.fail {
            return Err(ProgramFinalRasterCallbackError::InvocationFailed);
        }
        if state.reject {
            return Ok(false);
        }
        state.received.push(raster);
        Ok(true)
    }
}
fn raster_runtime(
    requirement: ProgramFinalRasterCleanlinessRequirement,
) -> Result<Arc<ProgramRuntime>, LabColorsError> {
    compile_program_wire(final_raster_wire(1, 2, 3, 17, requirement), 100)
}
fn consume_raster(
    runtime: &Arc<ProgramRuntime>,
    state: &Arc<Mutex<RasterHostState>>,
    revision: u64,
    context: Vec<u8>,
) -> Result<(), LabColorsError> {
    runtime.consume_final_raster(
        revision,
        runtime.final_raster_content_identity()?,
        17,
        context,
        Box::new(RasterHost {
            state: state.clone(),
            owner: Arc::downgrade(runtime),
        }),
    )
}
fn update_four_pixels(
    runtime: &ProgramRuntime,
    revision: u64,
    context: Vec<u8>,
    source: Vec<u8>,
    destination: Vec<u8>,
) -> Result<Arc<ProgramSnapshot>, LabColorsError> {
    runtime.update_final_raster(revision, 2, 2, 1, context, 1, source, 2, destination)
}
fn saved_raster(state: &Arc<Mutex<RasterHostState>>) -> ProgramFinalRasterBytes {
    state
        .lock()
        .expect("raster host state")
        .received
        .last()
        .expect("consumer received raster")
        .clone()
}

#[test]
fn native_final_raster_delivers_every_locked_pixel_as_owned_core_result() -> TestResult {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ProgramFinalRasterBytes>();
    let runtime =
        raster_runtime(ProgramFinalRasterCleanlinessRequirement::MandatoryExactNominalSrgb8V2)?;
    let state = Arc::new(Mutex::new(RasterHostState::default()));
    // 64 + round(128 * 127 / 255) = 128; остальные pixels проверяют полный row-major payload.
    let source = vec![64, 64, 64, 128, 0, 0, 0, 0, 32, 32, 32, 255, 0, 0, 0, 0];
    let destination = vec![
        128, 128, 128, 255, 64, 64, 64, 255, 0, 0, 0, 255, 0, 199, 63, 255,
    ];
    let snapshot =
        runtime.update_final_raster(1, 2, 2, 2, vec![7; 32], 1, source, 2, destination)?;
    assert_eq!(snapshot.state(), "ready");
    assert!(snapshot.assessment_report()?.is_none());
    consume_raster(&runtime, &state, 1, vec![7; 32])?;
    let original = saved_raster(&state);
    assert_eq!(
        (
            original.width,
            original.height,
            original.device_pixel_ratio,
            original.revision,
            original.output_slot
        ),
        (2, 2, 2, 1, 17)
    );
    assert_eq!(original.scope, "ModeledFinalRasterV1");
    assert_eq!(
        original.pixels,
        vec![128, 128, 128, 64, 64, 64, 32, 32, 32, 0, 199, 63]
    );
    assert_eq!(
        original.cleanliness.requirement,
        ProgramFinalRasterCleanlinessRequirement::MandatoryExactNominalSrgb8V2
    );
    assert_eq!(
        original.cleanliness.verdict,
        ProgramFinalRasterAssessmentVerdict::Pass
    );
    assert_eq!(original.cleanliness.checked_pixel_count, 4);
    assert_eq!(original.cleanliness.violation_count, 0);
    assert!(original.cleanliness.first_violation.is_none());
    assert_eq!(
        original.cleanliness.profile.as_deref(),
        Some("ExactNominalSrgb8CleanSetV2")
    );
    assert_eq!(
        original
            .cleanliness
            .profile_release
            .as_ref()
            .ok_or("profile release")?
            .len(),
        32
    );
    let mut copied = original.clone();
    copied.pixels.fill(0);
    copied
        .cleanliness
        .profile_release
        .as_mut()
        .ok_or("profile release")?
        .fill(0);
    consume_raster(&runtime, &state, 1, vec![7; 32])?;
    assert_eq!(saved_raster(&state), original);
    assert_ne!(copied, original);
    drop(runtime);
    assert_eq!(saved_raster(&state), original);
    Ok(())
}

#[test]
fn native_final_raster_policy_modes_keep_last_pixel_witness_and_identity() -> TestResult {
    use ProgramFinalRasterCleanlinessRequirement as C;
    let mut identities = Vec::new();
    for requirement in [
        C::Off,
        C::LintExactNominalSrgb8V2,
        C::MandatoryExactNominalSrgb8V2,
    ] {
        let runtime = raster_runtime(requirement)?;
        let identity = runtime.final_raster_content_identity()?;
        assert!(!identities.contains(&identity));
        identities.push(identity);
        let state = Arc::new(Mutex::new(RasterHostState::default()));
        assert_eq!(
            update_four_pixels(
                &runtime,
                1,
                vec![7; 32],
                vec![0; 16],
                [0, 199, 63, 255].repeat(4)
            )?
            .state(),
            "ready"
        );
        consume_raster(&runtime, &state, 1, vec![7; 32])?;
        let old = saved_raster(&state);
        let mut last_dirty = [0, 199, 63, 255].repeat(4);
        last_dirty[14] = 64;
        let next = update_four_pixels(&runtime, 2, vec![7; 32], vec![0; 16], last_dirty)?;
        if requirement == C::MandatoryExactNominalSrgb8V2 {
            assert_eq!(next.state(), "failed");
            let refusal = runtime.final_raster_refusal()?.ok_or("mandatory refusal")?;
            assert_eq!(refusal.code, "program_field_convention");
            assert_eq!(refusal.pixel_index, Some(3));
            assert_eq!(
                refusal.composite,
                Some(ProgramAssessmentSrgb8 {
                    red: 0,
                    green: 199,
                    blue: 64
                })
            );
            assert_eq!(
                refusal.rejected_blue_interval,
                Some(ProgramAssessmentBlueInterval {
                    lower: 64,
                    upper: 108
                })
            );
            for revision in [1, 2] {
                expect_error(
                    consume_raster(&runtime, &state, revision, vec![7; 32]),
                    "program_field_stale",
                );
            }
            assert_eq!(state.lock().map_err(|_| "host state")?.calls, 1);
            assert_eq!(saved_raster(&state), old);
            assert_eq!(
                update_four_pixels(
                    &runtime,
                    3,
                    vec![7; 32],
                    vec![0; 16],
                    [0, 199, 63, 255].repeat(4)
                )?
                .state(),
                "ready"
            );
            consume_raster(&runtime, &state, 3, vec![7; 32])?;
            assert!(runtime.final_raster_refusal()?.is_none());
            continue;
        }
        assert_eq!(next.state(), "ready");
        assert!(runtime.final_raster_refusal()?.is_none());
        consume_raster(&runtime, &state, 2, vec![7; 32])?;
        let raster = saved_raster(&state);
        assert_eq!(
            raster.pixels,
            vec![0, 199, 63, 0, 199, 63, 0, 199, 63, 0, 199, 64]
        );
        assert_eq!(raster.cleanliness.requirement, requirement);
        if requirement == C::Off {
            assert_eq!(
                raster.cleanliness.verdict,
                ProgramFinalRasterAssessmentVerdict::NotApplied
            );
            assert_eq!(raster.cleanliness.checked_pixel_count, 0);
            assert_eq!(raster.cleanliness.violation_count, 0);
            assert!(raster.cleanliness.profile.is_none());
            assert!(raster.cleanliness.profile_release.is_none());
            assert!(raster.cleanliness.first_violation.is_none());
        } else {
            assert_eq!(
                raster.cleanliness.verdict,
                ProgramFinalRasterAssessmentVerdict::Violation
            );
            assert_eq!(raster.cleanliness.checked_pixel_count, 4);
            assert_eq!(raster.cleanliness.violation_count, 1);
            assert_eq!(
                raster.cleanliness.first_violation,
                Some(ProgramFinalRasterViolation {
                    pixel_index: 3,
                    composite: ProgramAssessmentSrgb8 {
                        red: 0,
                        green: 199,
                        blue: 64
                    },
                    rejected_blue_interval: ProgramAssessmentBlueInterval {
                        lower: 64,
                        upper: 108
                    },
                })
            );
        }
    }
    Ok(())
}

#[test]
fn native_final_raster_freshness_rechecks_background_and_refuses_point_scope() -> TestResult {
    let runtime =
        raster_runtime(ProgramFinalRasterCleanlinessRequirement::LintExactNominalSrgb8V2)?;
    let state = Arc::new(Mutex::new(RasterHostState::default()));
    let mut background = [0, 199, 63, 255].repeat(4);
    background[14] = 64;
    update_four_pixels(&runtime, 1, vec![7; 32], vec![0; 16], background.clone())?;
    consume_raster(&runtime, &state, 1, vec![7; 32])?;
    let old = saved_raster(&state);
    assert_eq!(
        old.cleanliness
            .first_violation
            .ok_or("last-pixel witness")?
            .pixel_index,
        3
    );
    let stale = runtime.update_unknown(2, 55)?;
    assert_eq!(stale.state(), "stale");
    expect_error(
        consume_raster(&runtime, &state, 1, vec![7; 32]),
        "program_field_stale",
    );
    update_four_pixels(
        &runtime,
        3,
        vec![8; 32],
        [32, 32, 32, 255].repeat(4),
        background,
    )?;
    expect_error(
        consume_raster(&runtime, &state, 3, vec![7; 32]),
        "program_field_context",
    );
    expect_error(
        consume_raster(&runtime, &state, 1, vec![8; 32]),
        "program_field_stale",
    );
    let host = || {
        Box::new(RasterHost {
            state: state.clone(),
            owner: Arc::downgrade(&runtime),
        }) as Box<dyn ProgramFinalRasterConsumer>
    };
    expect_error(
        runtime.consume_final_raster(3, vec![0; 32], 17, vec![8; 32], host()),
        "program_field_program",
    );
    expect_error(
        runtime.consume_final_raster(
            3,
            runtime.final_raster_content_identity()?,
            18,
            vec![8; 32],
            host(),
        ),
        "program_field_output",
    );
    expect_error(
        runtime.consume_final_raster(3, vec![0; 31], 17, vec![8; 32], host()),
        "program_field_length",
    );
    expect_error(
        consume_raster(&runtime, &state, 3, vec![8; 31]),
        "program_field_length",
    );
    assert_eq!(state.lock().map_err(|_| "host state")?.calls, 1);
    consume_raster(&runtime, &state, 3, vec![8; 32])?;
    let fresh = saved_raster(&state);
    assert_eq!(fresh.pixels, [32, 32, 32].repeat(4));
    assert_eq!(
        fresh.cleanliness.verdict,
        ProgramFinalRasterAssessmentVerdict::Pass
    );
    assert_eq!(fresh.cleanliness.checked_pixel_count, 4);
    assert_eq!(fresh.cleanliness.violation_count, 0);
    assert_eq!(
        old.cleanliness.verdict,
        ProgramFinalRasterAssessmentVerdict::Violation
    );
    let point = compile_program_wire(wire("adaptive"), 200)?;
    point.update_observed(1, vec![7], vec![255; 3], 1)?;
    let point_host = Box::new(RasterHost {
        state: state.clone(),
        owner: Arc::downgrade(&point),
    });
    expect_error(
        point.consume_final_raster(
            1,
            point.final_raster_content_identity()?,
            91,
            vec![8; 32],
            point_host,
        ),
        "program_field_unsupported",
    );
    assert_eq!(state.lock().map_err(|_| "host state")?.calls, 2);
    Ok(())
}

#[test]
fn native_final_raster_shape_limits_and_conflicts_do_not_commit_a_revision() -> TestResult {
    let runtime = raster_runtime(ProgramFinalRasterCleanlinessRequirement::Off)?;
    for (
        width,
        height,
        dpr,
        context,
        source_port,
        source,
        destination_port,
        destination,
        expected,
    ) in [
        (
            0,
            2,
            1,
            vec![7; 32],
            1,
            vec![],
            2,
            vec![],
            "program_field_extent",
        ),
        (
            65537,
            1,
            1,
            vec![7; 32],
            1,
            vec![],
            2,
            vec![],
            "program_resource_exhausted",
        ),
        (
            u32::MAX,
            u32::MAX,
            1,
            vec![7; 32],
            1,
            vec![],
            2,
            vec![],
            "program_resource_exhausted",
        ),
        (
            2,
            2,
            5,
            vec![7; 32],
            1,
            vec![0; 16],
            2,
            [64, 64, 64, 255].repeat(4),
            "program_field_dpr",
        ),
        (
            2,
            2,
            1,
            vec![7; 31],
            1,
            vec![0; 16],
            2,
            [64, 64, 64, 255].repeat(4),
            "program_field_length",
        ),
        (
            2,
            2,
            1,
            vec![7; 32],
            1,
            vec![0; 15],
            2,
            [64, 64, 64, 255].repeat(4),
            "program_field_length",
        ),
        (
            2,
            2,
            1,
            vec![7; 32],
            1,
            vec![0; 16],
            2,
            vec![0; 17],
            "program_field_length",
        ),
        (
            2,
            2,
            1,
            vec![7; 32],
            1,
            [128, 0, 0, 127].repeat(4),
            2,
            [64, 64, 64, 255].repeat(4),
            "program_field_premultiplied",
        ),
        (
            2,
            2,
            1,
            vec![7; 32],
            1,
            vec![0; 16],
            1,
            [64, 64, 64, 255].repeat(4),
            "program_field_ports",
        ),
    ] {
        expect_error(
            runtime.update_final_raster(
                1,
                width,
                height,
                dpr,
                context,
                source_port,
                source,
                destination_port,
                destination,
            ),
            expected,
        );
    }
    let source = vec![0; 16];
    let destination = [64, 64, 64, 255].repeat(4);
    update_four_pixels(
        &runtime,
        1,
        vec![7; 32],
        source.clone(),
        destination.clone(),
    )?;
    update_four_pixels(
        &runtime,
        1,
        vec![7; 32],
        source.clone(),
        destination.clone(),
    )?;
    let mut changed = destination.clone();
    changed[12] = 65;
    for (width, height, dpr, context, pixels) in [
        (2, 2, 1, vec![7; 32], changed),
        (1, 4, 1, vec![7; 32], destination.clone()),
        (2, 2, 2, vec![7; 32], destination.clone()),
        (2, 2, 1, vec![8; 32], destination.clone()),
    ] {
        expect_error(
            runtime.update_final_raster(
                1,
                width,
                height,
                dpr,
                context,
                1,
                source.clone(),
                2,
                pixels,
            ),
            "program_field_update",
        );
    }
    let state = Arc::new(Mutex::new(RasterHostState::default()));
    consume_raster(&runtime, &state, 1, vec![7; 32])?;
    assert_eq!(saved_raster(&state).pixels, [64, 64, 64].repeat(4));
    let maximum = runtime.update_final_raster(
        2,
        256,
        256,
        1,
        vec![8; 32],
        1,
        vec![0; 65536 * 4],
        2,
        [64, 64, 64, 255].repeat(65536),
    )?;
    assert_eq!(maximum.state(), "ready");
    consume_raster(&runtime, &state, 2, vec![8; 32])?;
    let maximum = saved_raster(&state);
    assert_eq!(maximum.pixels.len(), 65536 * 3);
    assert_eq!(&maximum.pixels[maximum.pixels.len() - 3..], &[64; 3]);
    Ok(())
}

#[test]
fn native_final_raster_callback_reentry_reject_and_error_preserve_current_head() -> TestResult {
    let runtime = raster_runtime(ProgramFinalRasterCleanlinessRequirement::Off)?;
    update_four_pixels(
        &runtime,
        1,
        vec![7; 32],
        vec![0; 16],
        [64, 64, 64, 255].repeat(4),
    )?;
    let state = Arc::new(Mutex::new(RasterHostState {
        reenter: true,
        reject: true,
        ..Default::default()
    }));
    expect_error(
        consume_raster(&runtime, &state, 1, vec![7; 32]),
        "program_field_consumer_rejected",
    );
    state.lock().map_err(|_| "host state")?.reject = false;
    state.lock().map_err(|_| "host state")?.fail = true;
    expect_error(
        consume_raster(&runtime, &state, 1, vec![7; 32]),
        "program_field_consumer_failed",
    );
    state.lock().map_err(|_| "host state")?.fail = false;
    consume_raster(&runtime, &state, 1, vec![7; 32])?;
    assert_eq!(state.lock().map_err(|_| "host state")?.calls, 3);
    assert_eq!(saved_raster(&state).revision, 1);
    update_four_pixels(
        &runtime,
        2,
        vec![8; 32],
        vec![0; 16],
        [65, 65, 65, 255].repeat(4),
    )?;
    Ok(())
}

#[test]
fn native_final_raster_off_and_lint_never_publish_transparent_final_domain() -> TestResult {
    for requirement in [
        ProgramFinalRasterCleanlinessRequirement::Off,
        ProgramFinalRasterCleanlinessRequirement::LintExactNominalSrgb8V2,
    ] {
        let runtime = raster_runtime(requirement)?;
        let state = Arc::new(Mutex::new(RasterHostState::default()));
        update_four_pixels(
            &runtime,
            1,
            vec![7; 32],
            vec![0; 16],
            [64, 64, 64, 255].repeat(4),
        )?;
        consume_raster(&runtime, &state, 1, vec![7; 32])?;
        let old = saved_raster(&state);
        let mut transparent = [64, 64, 64, 255].repeat(4);
        transparent[15] = 128;
        assert_eq!(
            update_four_pixels(&runtime, 2, vec![7; 32], vec![0; 16], transparent)?.state(),
            "failed"
        );
        let refusal = runtime
            .final_raster_refusal()?
            .ok_or("transparent refusal")?;
        assert_eq!(refusal.code, "program_field_transparent");
        assert_eq!(refusal.pixel_index, Some(3));
        assert!(refusal.composite.is_none());
        expect_error(
            consume_raster(&runtime, &state, 2, vec![7; 32]),
            "program_field_stale",
        );
        assert_eq!(state.lock().map_err(|_| "host state")?.calls, 1);
        assert_eq!(saved_raster(&state), old);
        update_four_pixels(
            &runtime,
            3,
            vec![7; 32],
            vec![0; 16],
            [64, 64, 64, 255].repeat(4),
        )?;
        consume_raster(&runtime, &state, 3, vec![7; 32])?;
        assert!(runtime.final_raster_refusal()?.is_none());
    }
    Ok(())
}
