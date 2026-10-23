//! Внешний Rust-потребитель тех же функций, которые экспортирует UniFFI.
use labcolors::*;
use std::sync::{Arc, Mutex, Weak};

type TestResult = Result<(), Box<dyn std::error::Error>>;

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
