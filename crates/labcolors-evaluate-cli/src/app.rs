//! Одно ограниченное сообщение, существующий Core и типизированный результат.
use crate::{
    request::{MAX_INPUT_BYTES, MAX_PROGRAM_BYTES, Request},
    sink::ModeledSink,
};
use labcolors_core::{
    Srgb8,
    point_evaluation::{
        DeclaredPointReportV1, PointEvaluationFailureKindV1, evaluate_declared_point_v1,
    },
    program_wire::{
        ProgramAttachErrorV1, ProgramAttachmentUpdateErrorV1, ProgramRuntimeErrorV1,
        ProgramScenarioV1, compile_program_wire_v1,
    },
};
use serde::Serialize;
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const HELP: &str = "\
labcolors-evaluate [--format json|jsonl] [FILE|-]\n\
\n\
Один запрос modeled sRGB8 + явная Declared-конвенция → отчёт и LCEN-сертификат.\n\
FILE по умолчанию — stdin; JSONL содержит ровно одну строку.\n\
Отчёт описывает этот вызов, не человеческое восприятие или состояние браузера.\n\
Копия сертификата требует проверки относительно нового текущего EVAL.\n\
\n\
Коды: 0 успех/помощь; 2 ввод; 3 unsupported; 4 оценка; 5 I/O; 6 ресурсы.\n";
const REQUEST_KIND: &str = "labcolors-declared-point-request-v1";
const REPORT_KIND: &str = "labcolors-declared-point-report-v1";
const SCOPE: &str = "modeled-srgb8-point";
const ADMISSION: &str = "declared-package-policy-candidate";
const HUMAN: &str = "not-requested";
// Постоянная схема отчёта + ограниченный LCEN-кортеж (r19), не размер из входа.
const MAX_OUTPUT_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Json,
    Jsonl,
}
struct Config {
    format: Format,
    input: Option<PathBuf>,
}
enum Args {
    Help,
    Run(Config),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Error {
    exit: i32,
    domain: &'static str,
    code: &'static str,
}
impl Error {
    const fn input(code: &'static str) -> Self {
        Self {
            exit: 2,
            domain: "input",
            code,
        }
    }
    const fn unsupported(code: &'static str) -> Self {
        Self {
            exit: 3,
            domain: "unsupported",
            code,
        }
    }
    const fn evaluate(code: &'static str) -> Self {
        Self {
            exit: 4,
            domain: "evaluation",
            code,
        }
    }
    const fn io(code: &'static str) -> Self {
        Self {
            exit: 5,
            domain: "io",
            code,
        }
    }
    const fn resource(code: &'static str) -> Self {
        Self {
            exit: 6,
            domain: "resource",
            code,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report<'a> {
    format_version: u8,
    kind: &'static str,
    ok: bool,
    scope: &'static str,
    admission: &'static str,
    human: &'static str,
    renderer_provenance: &'static str,
    terminal_srgb8: [u8; 3],
    revision: u64,
    program_identity_sha256: String,
    profile_identity_sha256: String,
    convention_release_sha256: String,
    subject_identity_sha256: String,
    certificate_hex: &'a str,
}

pub(crate) fn run<I: IntoIterator<Item = OsString>, R: Read, W: Write, E: Write>(
    args: I,
    mut stdin: R,
    mut stdout: W,
    mut stderr: E,
) -> i32 {
    let result = (|| {
        let config = match parse_args(args)? {
            Args::Help => {
                return stdout
                    .write_all(HELP.as_bytes())
                    .map_err(|_| Error::io("write_failed"));
            }
            Args::Run(config) => config,
        };
        let input = read_input(config.input.as_deref(), &mut stdin)?;
        if config.format == Format::Jsonl {
            let trimmed = input.trim_ascii();
            if trimmed.is_empty() || trimmed.contains(&b'\n') || trimmed.contains(&b'\r') {
                return Err(Error::input("invalid_jsonl_record"));
            }
        }
        let request: Request =
            serde_json::from_slice(&input).map_err(|_| Error::input("invalid_document"))?;
        let report = evaluate_request(&request)?;
        let output = encode_report(&report, config.format)?;
        stdout
            .write_all(&output)
            .map_err(|_| Error::io("write_failed"))
    })();
    match result {
        Ok(()) => 0,
        Err(err) => finish_error(err, &mut stderr),
    }
}

fn parse_args<I: IntoIterator<Item = OsString>>(args: I) -> Result<Args, Error> {
    let mut args = args.into_iter().peekable();
    if args
        .peek()
        .is_some_and(|a| a == OsStr::new("--help") || a == OsStr::new("-h"))
    {
        args.next();
        return if args.next().is_none() {
            Ok(Args::Help)
        } else {
            Err(Error::input("invalid_arguments"))
        };
    }
    let mut format = None;
    let mut input = None;
    while let Some(arg) = args.next() {
        if arg == OsStr::new("--format") {
            if format.is_some() {
                return Err(Error::input("duplicate_format"));
            }
            let next = args.next().ok_or(Error::input("missing_format"))?;
            format = Some(match next.to_str() {
                Some("json") => Format::Json,
                Some("jsonl") => Format::Jsonl,
                _ => return Err(Error::input("unsupported_format")),
            });
        } else {
            if arg.to_string_lossy().starts_with('-') && arg != OsStr::new("-") {
                return Err(Error::input("unknown_option"));
            }
            if input.replace(PathBuf::from(arg)).is_some() {
                return Err(Error::input("too_many_inputs"));
            }
        }
    }
    Ok(Args::Run(Config {
        format: format.unwrap_or(Format::Json),
        input,
    }))
}
fn read_input<R: Read>(path: Option<&Path>, stdin: &mut R) -> Result<Vec<u8>, Error> {
    match path {
        Some(p) if p.as_os_str() != OsStr::new("-") => {
            read_bounded(&mut File::open(p).map_err(|_| Error::io("read_failed"))?)
        }
        _ => read_bounded(stdin),
    }
}
fn read_bounded<R: Read>(reader: &mut R) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let mut buf = [0; 16 * 1024];
    loop {
        let count = buf.len().min(MAX_INPUT_BYTES + 1 - output.len());
        let read = match reader.read(&mut buf[..count]) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(Error::io("read_failed")),
            Ok(read) => read,
        };
        if read == 0 {
            return Ok(output);
        }
        if output.len() + read > MAX_INPUT_BYTES {
            return Err(Error::resource("input_too_large"));
        }
        output
            .try_reserve_exact(read)
            .map_err(|_| Error::resource("allocation_refused"))?;
        output.extend_from_slice(&buf[..read]);
    }
}

fn evaluate_request(request: &Request) -> Result<DeclaredPointReportV1, Error> {
    if request.format_version != 1 || request.kind != REQUEST_KIND {
        return Err(Error::unsupported("unsupported_request"));
    }
    let profile = &request.profile;
    if profile.scope != SCOPE {
        return Err(Error::unsupported("unsupported_scope"));
    }
    if profile.admission != ADMISSION {
        return Err(Error::unsupported("unsupported_admission"));
    }
    if profile.human != HUMAN {
        return Err(Error::unsupported("unsupported_human_evidence"));
    }
    if profile.convention_release_sha256.len() != 64 {
        return Err(Error::input("invalid_convention_digest"));
    }
    let release: [u8; 32] = decode_hex(&profile.convention_release_sha256, 32)?
        .try_into()
        .map_err(|_| Error::input("invalid_convention_digest"))?;
    let wire = decode_hex(&request.program_wire_hex, MAX_PROGRAM_BYTES)?;
    if request.observation.scenarios.0.is_empty() {
        return Err(Error::input("missing_scenarios"));
    }
    let mut scenarios = Vec::new();
    scenarios
        .try_reserve_exact(request.observation.scenarios.0.len())
        .map_err(|_| Error::resource("allocation_refused"))?;
    for s in &request.observation.scenarios.0 {
        let mut surfaces = Vec::new();
        surfaces
            .try_reserve_exact(s.surfaces.0.len())
            .map_err(|_| Error::resource("allocation_refused"))?;
        surfaces.extend(s.surfaces.0.iter().copied().map(Srgb8::new));
        scenarios.push(ProgramScenarioV1::new(s.id, surfaces));
    }
    let compiled = compile_program_wire_v1(&wire).map_err(runtime_error)?;
    let b = &request.binding;
    let mut attachment = compiled
        .attach(
            b.stream_id,
            b.output_slot,
            b.sink_output,
            b.presentation_root,
            b.occurrence,
            ModeledSink::new(b.sink_output),
        )
        .map_err(|e| match e {
            ProgramAttachErrorV1::ResourceExhausted => {
                Error::resource("attachment_resource_exhausted")
            }
            ProgramAttachErrorV1::NonTerminalTarget => Error::unsupported("non_terminal_target"),
            ProgramAttachErrorV1::Binding => Error::unsupported("attachment_binding_rejected"),
            _ => Error::evaluate("attachment_rejected"),
        })?;
    attachment
        .update_observed(request.observation.revision, &scenarios)
        .map_err(|e| match e {
            ProgramAttachmentUpdateErrorV1::ResourceExhausted => {
                Error::resource("observation_resource_exhausted")
            }
            ProgramAttachmentUpdateErrorV1::InternalInvariant => {
                Error::resource("internal_invariant")
            }
            _ => Error::evaluate("observation_rejected"),
        })?;
    evaluate_declared_point_v1(&attachment, release).map_err(|e| Error {
        exit: match e.kind() {
            PointEvaluationFailureKindV1::Unsupported => 3,
            PointEvaluationFailureKindV1::Evaluation => 4,
            PointEvaluationFailureKindV1::Resource | PointEvaluationFailureKindV1::Internal => 6,
            _ => 6,
        },
        domain: e.domain(),
        code: e.code(),
    })
}
fn runtime_error(error: ProgramRuntimeErrorV1) -> Error {
    match error {
        ProgramRuntimeErrorV1::Wire => Error::input("invalid_program_wire"),
        ProgramRuntimeErrorV1::Compile => Error::evaluate("program_compile_rejected"),
        ProgramRuntimeErrorV1::FamilyArtifactsRequired => {
            Error::unsupported("family_artifacts_required")
        }
        _ => Error::evaluate("program_runtime_rejected"),
    }
}
fn encode_hex(bytes: &[u8]) -> Result<String, Error> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let capacity = bytes
        .len()
        .checked_mul(2)
        .ok_or(Error::resource("output_too_large"))?;
    let mut text = String::new();
    text.try_reserve_exact(capacity)
        .map_err(|_| Error::resource("allocation_refused"))?;
    for byte in bytes {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 15) as usize] as char);
    }
    Ok(text)
}
fn decode_hex(input: &str, limit: usize) -> Result<Vec<u8>, Error> {
    let bytes = input.as_bytes();
    if bytes.len() > limit * 2 {
        return Err(Error::resource("wire_too_large"));
    }
    if bytes.len() % 2 != 0 {
        return Err(Error::input("invalid_hex"));
    }
    fn nibble(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            _ => None,
        }
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len() / 2)
        .map_err(|_| Error::resource("allocation_refused"))?;
    for pair in bytes.chunks_exact(2) {
        output.push(
            nibble(pair[0])
                .zip(nibble(pair[1]))
                .map(|(a, b)| (a << 4) | b)
                .ok_or(Error::input("invalid_hex"))?,
        );
    }
    Ok(output)
}

/// Ошибка локальной сериализации не может начать внешнюю запись.
#[derive(Default)]
struct OutputBuffer(Vec<u8>);
impl Write for OutputBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_OUTPUT_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("output_limit"));
        }
        self.0
            .try_reserve_exact(bytes.len())
            .map_err(|_| io::Error::other("allocation_refused"))?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode_report(report: &DeclaredPointReportV1, format: Format) -> Result<Vec<u8>, Error> {
    let certificate = encode_hex(report.certificate_bytes())?;
    let value = Report {
        format_version: 1,
        kind: REPORT_KIND,
        ok: true,
        scope: SCOPE,
        admission: ADMISSION,
        human: HUMAN,
        renderer_provenance: "unverified",
        terminal_srgb8: report.terminal_srgb8().bytes(),
        revision: report.revision(),
        program_identity_sha256: encode_hex(report.program_identity())?,
        profile_identity_sha256: encode_hex(report.profile_identity())?,
        convention_release_sha256: encode_hex(report.convention_release())?,
        subject_identity_sha256: encode_hex(report.subject_identity())?,
        certificate_hex: &certificate,
    };
    let mut output = OutputBuffer::default();
    match format {
        Format::Json => serde_json::to_writer_pretty(&mut output, &value),
        Format::Jsonl => serde_json::to_writer(&mut output, &value),
    }
    .map_err(|_| Error::resource("report_unavailable"))?;
    output
        .write_all(b"\n")
        .map_err(|_| Error::resource("report_unavailable"))?;
    Ok(output.0)
}
fn finish_error(error: Error, stderr: &mut impl Write) -> i32 {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Fault {
        format_version: u8,
        ok: bool,
        error: Reason,
    }
    #[derive(Serialize)]
    struct Reason {
        domain: &'static str,
        code: &'static str,
    }
    let document = Fault {
        format_version: 1,
        ok: false,
        error: Reason {
            domain: error.domain,
            code: error.code,
        },
    };
    if serde_json::to_writer(&mut *stderr, &document).is_err() || stderr.write_all(b"\n").is_err() {
        5
    } else {
        error.exit
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
