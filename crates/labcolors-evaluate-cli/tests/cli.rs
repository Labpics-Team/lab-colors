//! Реальный собранный процесс и независимая прямая точка входа Core.
use labcolors_core::{
    Srgb8,
    point_evaluation::evaluate_declared_point_v1,
    program_wire::{
        ProgramPointSinkHostErrorV1, ProgramPointSinkHostV1, ProgramPointSinkIntentV1,
        ProgramPointSinkStampV1, ProgramScenarioV1, compile_program_wire_v1,
    },
};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
const GOOD: &str = include_str!("../examples/declared-point.json");
const REJECTED: &str = include_str!("../examples/rejected-point.json");
fn run(args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_labcolors-evaluate"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut pipe = child.stdin.take().unwrap();
    pipe.write_all(input).unwrap();
    drop(pipe);
    child.wait_with_output().unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}

/// Отдельный процесс начинает с новой эпохи attachment, как одноразовый CLI.
/// Этот oracle не использует Request, sink или сериализатор проверяемого CLI.
#[test]
fn direct_core_oracle() {
    struct Recorder(Option<ProgramPointSinkStampV1>);
    impl ProgramPointSinkHostV1 for Recorder {
        fn try_install(
            &mut self,
            intent: ProgramPointSinkIntentV1,
        ) -> Result<(), ProgramPointSinkHostErrorV1> {
            assert!(self.0.is_none_or(|p| p == intent.expected_stamp()));
            self.0 = Some(intent.desired_stamp());
            Ok(())
        }
    }
    let value: Value = serde_json::from_str(GOOD).unwrap();
    let bind = &value["binding"];
    let get = |key: &str| bind[key].as_u64().unwrap() as u32;
    let mut attachment = compile_program_wire_v1(&unhex(value["programWireHex"].as_str().unwrap()))
        .unwrap()
        .attach(
            get("streamId"),
            get("outputSlot"),
            get("sinkOutput"),
            get("presentationRoot"),
            get("occurrence"),
            Recorder(None),
        )
        .unwrap();
    attachment
        .update_observed(
            1,
            &[ProgramScenarioV1::new(1, vec![Srgb8::new([128, 128, 127])])],
        )
        .unwrap();
    let release = unhex(
        value["profile"]["conventionReleaseSha256"]
            .as_str()
            .unwrap(),
    )
    .try_into()
    .unwrap();
    let report = evaluate_declared_point_v1(&attachment, release).unwrap();
    let document = json!({"formatVersion":1,"kind":"labcolors-declared-point-report-v1","ok":true,
        "scope":"modeled-srgb8-point","admission":"declared-package-policy-candidate","human":"not-requested",
        "rendererProvenance":"unverified","terminalSrgb8":report.terminal_srgb8().bytes(),"revision":report.revision(),
        "programIdentitySha256":hex(report.program_identity()),"profileIdentitySha256":hex(report.profile_identity()),
        "conventionReleaseSha256":hex(report.convention_release()),"subjectIdentitySha256":hex(report.subject_identity()),
        "certificateHex":hex(report.certificate_bytes())});
    println!("DIRECT_REPORT={document}");
}

#[test]
fn binary_stdin_file_and_jsonl_equal_the_direct_fresh_process() {
    let oracle = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "direct_core_oracle", "--nocapture"])
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let text = String::from_utf8(oracle.stdout).unwrap();
    let expected: Value = serde_json::from_str(
        text.lines()
            .find_map(|l| l.strip_prefix("DIRECT_REPORT="))
            .unwrap(),
    )
    .unwrap();
    let standard = run(&[], GOOD.as_bytes());
    assert!(
        standard.status.success(),
        "{}",
        String::from_utf8_lossy(&standard.stderr)
    );
    assert!(standard.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&standard.stdout).unwrap(),
        expected
    );
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/declared-point.json");
    let from_file = run(&[file], b"");
    assert!(from_file.status.success());
    assert_eq!(from_file.stdout, standard.stdout);
    let compact = serde_json::to_vec(&serde_json::from_str::<Value>(GOOD).unwrap()).unwrap();
    let jsonl = run(&["--format", "jsonl", "-"], &compact);
    assert!(jsonl.status.success());
    assert!(jsonl.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&jsonl.stdout).unwrap(),
        expected
    );
    assert_eq!(jsonl.stdout.iter().filter(|&&b| b == b'\n').count(), 1);
}
#[test]
fn binary_failure_has_no_successful_output_or_secret_error_detail() {
    let bad = run(&[], REJECTED.as_bytes());
    assert_eq!(bad.status.code(), Some(4));
    assert!(bad.stdout.is_empty());
    let error: Value = serde_json::from_slice(&bad.stderr).unwrap();
    assert_eq!(error["ok"], false);
    assert_eq!(error["error"]["code"], "rejected_by_convention");
    let missing = run(&["/definitely-missing-labcolors-input"], b"");
    assert_eq!(missing.status.code(), Some(5));
    assert!(missing.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&missing.stderr).contains("definitely-missing"));
    let malformed = run(&[], b"{\"profile\":{}}");
    assert_eq!(malformed.status.code(), Some(2));
    assert!(malformed.stdout.is_empty());
    let help = run(&["--help"], b"");
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--format"));
}

#[cfg(target_os = "linux")]
#[test]
fn actual_process_cannot_succeed_when_stdout_refuses_bytes() {
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_labcolors-evaluate"))
        .stdin(Stdio::piped())
        .stdout(full)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(GOOD.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(5));
    let error: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(error["error"]["code"], "write_failed");
}

#[test]
fn real_process_refuses_oversized_input_without_unbounded_output() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_labcolors-evaluate"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Производитель может получить BrokenPipe после законного раннего отказа.
    // Решение читается из завершившегося потребителя, не из успеха записи входа.
    let _ = child
        .stdin
        .take()
        .unwrap()
        .write_all(&vec![b' '; 2 * 1024 * 1024 + 2]);
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(6));
    assert!(result.stdout.is_empty());
    let error: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(error["error"]["code"], "input_too_large");
    assert!(result.stderr.len() < 256);
}
