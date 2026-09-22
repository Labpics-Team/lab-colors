use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use labcolors_core::certificate::{
    CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1, CertificateErrorV1, MAX_ENVELOPE_BYTES_V1,
    UntrustedEnvelopeV1,
};
use serde::{Deserialize, Serialize};

const DOCUMENT_FORMAT_VERSION: u8 = 1;
const TRANSPORT_KIND: &str = "labcolors-certificate-envelope-v1";
const INSPECTION_KIND: &str = "labcolors-certificate-envelope-inspection-v1";
const TEXT_OVERHEAD_BYTES: usize = 4_096;
const MAX_TEXT_INPUT_BYTES: usize = MAX_ENVELOPE_BYTES_V1 * 2 + TEXT_OVERHEAD_BYTES;
const HELP: &str = "\
labcolors-transport <parse|serialize|inspect> [--format json|jsonl] [FILE|-]\n\
\n\
parse      validate binary LCEN v1 and emit a lossless JSON transport document\n\
serialize  validate a transport document and emit the exact canonical LCEN bytes\n\
inspect    validate binary LCEN v1 and emit metadata only\n\
\n\
One bounded envelope is processed per invocation. FILE defaults to stdin.\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Parse,
    Serialize,
    Inspect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Jsonl,
}

struct Config {
    command: Command,
    format: OutputFormat,
    input: Option<PathBuf>,
}

enum ParsedArgs {
    Help,
    Run(Config),
}

#[derive(Debug)]
enum AppError {
    Usage(&'static str),
    Transport(&'static str),
    Certificate(CertificateErrorV1),
    Io(&'static str),
    Internal(&'static str),
}

impl AppError {
    fn domain(&self) -> &'static str {
        match self {
            Self::Certificate(_) => "certificate",
            Self::Io(_) => "io",
            Self::Usage(_) | Self::Transport(_) => "transport",
            Self::Internal(_) => "internal",
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Usage(code) | Self::Transport(code) | Self::Io(code) | Self::Internal(code) => {
                code
            }
            Self::Certificate(error) => error.code(),
        }
    }

    fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) | Self::Transport(_) => 2,
            Self::Certificate(_) => 3,
            Self::Io(_) => 4,
            Self::Internal(_) => 5,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorDocument<'a> {
    format_version: u8,
    ok: bool,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    domain: &'a str,
    code: &'a str,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransportDocumentV1 {
    format_version: u8,
    kind: String,
    wire_hex: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectionDocumentV1<'a> {
    format_version: u8,
    kind: &'static str,
    certificate: InspectionCertificateV1<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectionCertificateV1<'a> {
    schema_version: u16,
    operation: &'a str,
    authority_kind: &'a str,
    authority_version: u16,
    runtime_artifact_id: &'a str,
    producer_revision: &'a str,
    producer_content_identity_hex: String,
    context_id: &'a str,
    payload_type: &'a str,
    payload_version: u16,
    payload_length: u32,
    payload_sha256_hex: String,
    binding_sha256_hex: String,
}

pub(crate) fn run<I, R, W, E>(args: I, mut stdin: R, mut stdout: W, mut stderr: E) -> i32
where
    I: IntoIterator<Item = OsString>,
    R: Read,
    W: Write,
    E: Write,
{
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(error) => return finish_error(error, &mut stderr),
    };
    let config = match parsed {
        ParsedArgs::Help => {
            if stdout.write_all(HELP.as_bytes()).is_ok() {
                return 0;
            }
            return finish_error(AppError::Io("write_failed"), &mut stderr);
        }
        ParsedArgs::Run(config) => config,
    };

    let result = match config.command {
        Command::Parse => parse_command(&config, &mut stdin, &mut stdout),
        Command::Serialize => serialize_command(&config, &mut stdin, &mut stdout),
        Command::Inspect => inspect_command(&config, &mut stdin, &mut stdout),
    };
    match result {
        Ok(()) => 0,
        Err(error) => finish_error(error, &mut stderr),
    }
}

fn parse_args<I>(args: I) -> Result<ParsedArgs, AppError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Err(AppError::Usage("missing_command"));
    };
    if first == OsStr::new("--help") || first == OsStr::new("-h") {
        if args.next().is_some() {
            return Err(AppError::Usage("invalid_arguments"));
        }
        return Ok(ParsedArgs::Help);
    }
    let command = match first.to_str() {
        Some("parse") => Command::Parse,
        Some("serialize") => Command::Serialize,
        Some("inspect") => Command::Inspect,
        _ => return Err(AppError::Usage("unknown_command")),
    };
    let mut format = OutputFormat::Json;
    let mut input = None;
    while let Some(argument) = args.next() {
        if argument == OsStr::new("--format") {
            let Some(value) = args.next() else {
                return Err(AppError::Usage("missing_format"));
            };
            format = match value.to_str() {
                Some("json") => OutputFormat::Json,
                Some("jsonl") => OutputFormat::Jsonl,
                _ => return Err(AppError::Usage("unsupported_format")),
            };
            continue;
        }
        if argument == OsStr::new("--help") || argument == OsStr::new("-h") {
            return Ok(ParsedArgs::Help);
        }
        if argument.to_string_lossy().starts_with('-') && argument != OsStr::new("-") {
            return Err(AppError::Usage("unknown_option"));
        }
        if input.replace(PathBuf::from(argument)).is_some() {
            return Err(AppError::Usage("too_many_inputs"));
        }
    }
    Ok(ParsedArgs::Run(Config {
        command,
        format,
        input,
    }))
}

fn parse_command<R: Read, W: Write>(
    config: &Config,
    stdin: &mut R,
    stdout: &mut W,
) -> Result<(), AppError> {
    let bytes = read_binary_input(config.input.as_deref(), stdin)?;
    UntrustedEnvelopeV1::decode(&bytes).map_err(AppError::Certificate)?;
    let document = TransportDocumentV1 {
        format_version: DOCUMENT_FORMAT_VERSION,
        kind: TRANSPORT_KIND.to_owned(),
        wire_hex: encode_hex(&bytes)?,
    };
    write_document(stdout, &document, config.format)
}

fn serialize_command<R: Read, W: Write>(
    config: &Config,
    stdin: &mut R,
    stdout: &mut W,
) -> Result<(), AppError> {
    let input = read_text_input(config.input.as_deref(), stdin)?;
    if config.format == OutputFormat::Jsonl {
        ensure_single_jsonl_record(&input)?;
    }
    let document: TransportDocumentV1 =
        serde_json::from_slice(&input).map_err(|_| AppError::Transport("invalid_document"))?;
    if document.format_version != DOCUMENT_FORMAT_VERSION || document.kind != TRANSPORT_KIND {
        return Err(AppError::Transport("unsupported_document"));
    }
    let bytes = decode_hex(&document.wire_hex)?;
    UntrustedEnvelopeV1::decode(&bytes).map_err(AppError::Certificate)?;
    stdout
        .write_all(&bytes)
        .map_err(|_| AppError::Io("write_failed"))
}

fn inspect_command<R: Read, W: Write>(
    config: &Config,
    stdin: &mut R,
    stdout: &mut W,
) -> Result<(), AppError> {
    let bytes = read_binary_input(config.input.as_deref(), stdin)?;
    let envelope = UntrustedEnvelopeV1::decode(&bytes).map_err(AppError::Certificate)?;
    let key = envelope.admission_key();
    let certificate = InspectionCertificateV1 {
        schema_version: CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1,
        operation: key.operation().key(),
        authority_kind: key.authority_kind().key(),
        authority_version: key.authority_version(),
        runtime_artifact_id: key.runtime_artifact_id(),
        producer_revision: key.producer_revision(),
        producer_content_identity_hex: encode_hex(key.producer_content_identity())?,
        context_id: key.context_id(),
        payload_type: key.payload_type().key(),
        payload_version: key.payload_version(),
        payload_length: envelope.payload_len(),
        payload_sha256_hex: encode_hex(envelope.payload_sha256())?,
        binding_sha256_hex: encode_hex(envelope.binding_sha256())?,
    };
    let document = InspectionDocumentV1 {
        format_version: DOCUMENT_FORMAT_VERSION,
        kind: INSPECTION_KIND,
        certificate,
    };
    write_document(stdout, &document, config.format)
}

fn read_binary_input<R: Read>(path: Option<&Path>, stdin: &mut R) -> Result<Vec<u8>, AppError> {
    read_input(path, stdin, MAX_ENVELOPE_BYTES_V1, true)
}

fn read_text_input<R: Read>(path: Option<&Path>, stdin: &mut R) -> Result<Vec<u8>, AppError> {
    read_input(path, stdin, MAX_TEXT_INPUT_BYTES, false)
}

fn read_input<R: Read>(
    path: Option<&Path>,
    stdin: &mut R,
    limit: usize,
    certificate_limit: bool,
) -> Result<Vec<u8>, AppError> {
    match path {
        Some(path) if path.as_os_str() != OsStr::new("-") => {
            let mut file = File::open(path).map_err(|_| AppError::Io("read_failed"))?;
            read_bounded(&mut file, limit, certificate_limit)
        }
        _ => read_bounded(stdin, limit, certificate_limit),
    }
}

fn read_bounded<R: Read>(
    reader: &mut R,
    limit: usize,
    certificate_limit: bool,
) -> Result<Vec<u8>, AppError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| AppError::Io("read_failed"))?;
        if read == 0 {
            return Ok(output);
        }
        let next = output
            .len()
            .checked_add(read)
            .ok_or(AppError::Internal("resource_exhausted"))?;
        if next > limit {
            return if certificate_limit {
                Err(AppError::Certificate(
                    CertificateErrorV1::ResourceLimitExceeded,
                ))
            } else {
                Err(AppError::Transport("input_too_large"))
            };
        }
        output
            .try_reserve_exact(read)
            .map_err(|_| AppError::Internal("resource_exhausted"))?;
        output.extend_from_slice(&buffer[..read]);
    }
}

fn ensure_single_jsonl_record(input: &[u8]) -> Result<(), AppError> {
    let trimmed = trim_ascii_whitespace(input);
    if trimmed.is_empty() || trimmed.contains(&b'\n') || trimmed.contains(&b'\r') {
        return Err(AppError::Transport("invalid_jsonl_record"));
    }
    Ok(())
}

fn trim_ascii_whitespace(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn encode_hex(bytes: &[u8]) -> Result<String, AppError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let capacity = bytes
        .len()
        .checked_mul(2)
        .ok_or(AppError::Internal("resource_exhausted"))?;
    let mut output = String::new();
    output
        .try_reserve_exact(capacity)
        .map_err(|_| AppError::Internal("resource_exhausted"))?;
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(output)
}

fn decode_hex(input: &str) -> Result<Vec<u8>, AppError> {
    let bytes = input.as_bytes();
    let max_hex = MAX_ENVELOPE_BYTES_V1
        .checked_mul(2)
        .ok_or(AppError::Internal("resource_exhausted"))?;
    if bytes.len() > max_hex {
        return Err(AppError::Certificate(
            CertificateErrorV1::ResourceLimitExceeded,
        ));
    }
    if bytes.len() % 2 != 0 {
        return Err(AppError::Transport("invalid_wire_hex"));
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len() / 2)
        .map_err(|_| AppError::Internal("resource_exhausted"))?;
    for pair in bytes.chunks_exact(2) {
        let high = decode_hex_nibble(pair[0]).ok_or(AppError::Transport("invalid_wire_hex"))?;
        let low = decode_hex_nibble(pair[1]).ok_or(AppError::Transport("invalid_wire_hex"))?;
        output.push((high << 4) | low);
    }
    Ok(output)
}

fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn write_document<W: Write, T: Serialize>(
    writer: &mut W,
    value: &T,
    format: OutputFormat,
) -> Result<(), AppError> {
    match format {
        OutputFormat::Json => serde_json::to_writer_pretty(&mut *writer, value),
        OutputFormat::Jsonl => serde_json::to_writer(&mut *writer, value),
    }
    .map_err(|_| AppError::Internal("projection_failed"))?;
    writer
        .write_all(b"\n")
        .map_err(|_| AppError::Io("write_failed"))
}

fn finish_error<E: Write>(error: AppError, stderr: &mut E) -> i32 {
    let code = error.exit_code();
    let document = ErrorDocument {
        format_version: DOCUMENT_FORMAT_VERSION,
        ok: false,
        error: ErrorBody {
            domain: error.domain(),
            code: error.code(),
        },
    };
    if serde_json::to_writer(&mut *stderr, &document).is_ok() {
        let _ = stderr.write_all(b"\n");
    }
    code
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use labcolors_core::certificate::{
        MAX_ENVELOPE_BYTES_V1, UntrustedEnvelopeV1, issue_source_certificate_v1,
    };
    use serde_json::Value;

    use super::*;

    fn fixture() -> Vec<u8> {
        issue_source_certificate_v1()
            .expect("source descriptor must be available in repository tests")
            .try_to_bytes()
            .unwrap()
    }

    fn reference_wire() -> (Vec<u8>, String) {
        let row = include_str!(
            "../../labcolors-core/contracts/certificate-envelope-v1/reference-vectors.tsv"
        )
        .lines()
        .find(|line| line.starts_with("canonical-binary-body\t"))
        .expect("canonical certificate reference vector");
        let wire_hex = row
            .split('\t')
            .nth(2)
            .expect("reference vector wire column")
            .to_owned();
        let wire = decode_hex(&wire_hex).expect("reference vector has canonical lowercase hex");
        (wire, wire_hex)
    }

    fn execute(args: &[&str], input: &[u8]) -> (i32, Vec<u8>, Vec<u8>) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(
            args.iter().map(|argument| OsString::from(*argument)),
            Cursor::new(input),
            &mut stdout,
            &mut stderr,
        );
        (code, stdout, stderr)
    }

    #[test]
    fn parse_and_serialize_are_lossless_without_a_second_wire_encoder() {
        let wire = fixture();
        let (code, document, stderr) = execute(&["parse", "-"], &wire);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        let parsed: TransportDocumentV1 = serde_json::from_slice(&document).unwrap();
        assert_eq!(parsed.format_version, DOCUMENT_FORMAT_VERSION);
        assert_eq!(parsed.kind, TRANSPORT_KIND);
        assert_eq!(decode_hex(&parsed.wire_hex).unwrap(), wire);

        let (code, serialized, stderr) = execute(&["serialize", "-"], &document);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(serialized, wire);
        assert!(UntrustedEnvelopeV1::decode(&serialized).is_ok());
    }

    #[test]
    fn jsonl_transport_projection_is_byte_stable_on_reference_vector() {
        let (wire, wire_hex) = reference_wire();
        let (code, stdout, stderr) = execute(&["parse", "--format", "jsonl", "-"], &wire);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        let expected = format!(
            "{{\"formatVersion\":1,\"kind\":\"{TRANSPORT_KIND}\",\"wireHex\":\"{wire_hex}\"}}\n"
        );
        assert_eq!(stdout, expected.as_bytes());
    }

    #[test]
    fn inspect_is_metadata_only_and_matches_core_projection() {
        let wire = fixture();
        let expected = UntrustedEnvelopeV1::decode(&wire).unwrap();
        let (code, stdout, stderr) = execute(&["inspect", "-"], &wire);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        let value: Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(value["formatVersion"], DOCUMENT_FORMAT_VERSION);
        assert_eq!(value["kind"], INSPECTION_KIND);
        assert_eq!(
            value["certificate"]["runtimeArtifactId"],
            expected.admission_key().runtime_artifact_id()
        );
        assert_eq!(
            value["certificate"]["payloadLength"],
            expected.payload_len()
        );
        assert!(value["certificate"].get("payload").is_none());
        assert!(value.get("wireHex").is_none());
        assert!(value.get("authorityResult").is_none());
    }

    #[test]
    fn jsonl_is_one_compact_record_and_round_trips() {
        let wire = fixture();
        let (code, document, stderr) = execute(&["parse", "--format", "jsonl", "-"], &wire);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(document.ends_with(b"\n"));
        assert_eq!(
            document[..document.len() - 1]
                .iter()
                .filter(|&&b| b == b'\n')
                .count(),
            0
        );

        let (code, serialized, stderr) =
            execute(&["serialize", "--format", "jsonl", "-"], &document);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(serialized, wire);
    }

    #[test]
    fn certificate_rejection_keeps_static_typed_error_and_no_output() {
        let mut wire = fixture();
        let last = wire.last_mut().unwrap();
        *last ^= 1;
        let (code, stdout, stderr) = execute(&["parse", "-"], &wire);
        assert_eq!(code, 3);
        assert!(stdout.is_empty());
        let value: Value = serde_json::from_slice(&stderr).unwrap();
        assert_eq!(value["error"]["domain"], "certificate");
        assert_eq!(value["error"]["code"], "binding_digest_mismatch");
        assert!(!String::from_utf8_lossy(&stderr).contains("labcolors-core-source-v1"));
    }

    #[test]
    fn binary_input_is_bounded_before_core_allocation() {
        let oversized = vec![0_u8; MAX_ENVELOPE_BYTES_V1 + 1];
        let (code, stdout, stderr) = execute(&["inspect", "-"], &oversized);
        assert_eq!(code, 3);
        assert!(stdout.is_empty());
        let value: Value = serde_json::from_slice(&stderr).unwrap();
        assert_eq!(value["error"]["code"], "resource_limit_exceeded");
    }

    #[test]
    fn serialize_rejects_noncanonical_hex_and_unknown_fields() {
        let wire = fixture();
        let (code, document, _) = execute(&["parse", "--format", "jsonl", "-"], &wire);
        assert_eq!(code, 0);
        let mut value: Value = serde_json::from_slice(&document).unwrap();
        value["wireHex"] = Value::String(value["wireHex"].as_str().unwrap().to_uppercase());
        let uppercase = serde_json::to_vec(&value).unwrap();
        let (code, stdout, stderr) = execute(&["serialize", "-"], &uppercase);
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&stderr).unwrap()["error"]["code"],
            "invalid_wire_hex"
        );

        value["wireHex"] = serde_json::from_slice::<Value>(&document).unwrap()["wireHex"].clone();
        value["extra"] = Value::Bool(true);
        let unknown = serde_json::to_vec(&value).unwrap();
        let (code, stdout, stderr) = execute(&["serialize", "-"], &unknown);
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&stderr).unwrap()["error"]["code"],
            "invalid_document"
        );
    }

    #[test]
    fn jsonl_input_rejects_multiple_records() {
        let wire = fixture();
        let (_, document, _) = execute(&["parse", "--format", "jsonl", "-"], &wire);
        let mut multiple = document.clone();
        multiple.extend_from_slice(&document);
        let (code, stdout, stderr) = execute(&["serialize", "--format", "jsonl", "-"], &multiple);
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&stderr).unwrap()["error"]["code"],
            "invalid_jsonl_record"
        );
    }

    #[test]
    fn cli_argument_failures_are_closed_and_machine_readable() {
        let (code, stdout, stderr) = execute(&["inspect", "--format", "yaml", "-"], &[]);
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        let value: Value = serde_json::from_slice(&stderr).unwrap();
        assert_eq!(value["error"]["domain"], "transport");
        assert_eq!(value["error"]["code"], "unsupported_format");
    }
}
