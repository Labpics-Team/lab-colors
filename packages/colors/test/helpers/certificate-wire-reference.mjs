import { createHash } from "node:crypto";

// Независимый oracle wire-контракта r13. Production encoder/decoder и их
// константы не импортируются; тела — только непрозрачные тестовые bytes.
const PAYLOAD_DOMAIN = Buffer.from("labpics.colors/certificate-payload/v1\0", "ascii");
const BINDING_DOMAIN = Buffer.from("labpics.colors/certificate-envelope/v1\0", "ascii");
const REVISION = "0123456789abcdef0123456789abcdef01234567";
export const ENVELOPE_LIMIT = 2_097_152;
export const PAYLOAD_LIMIT = 1_048_576;

const hash = (...parts) => {
  const digest = createHash("sha256");
  for (const part of parts) digest.update(part);
  return digest.digest();
};
const u16 = (value) => {
  const bytes = Buffer.alloc(2);
  bytes.writeUInt16BE(value);
  return bytes;
};
const u32 = (value) => {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32BE(value);
  return bytes;
};

export function referencePacket({
  runtime = "labcolors-reference",
  revision = REVISION,
  identity = Buffer.alloc(32, 0x35),
  context = "wire-reference",
  payload = Buffer.from([0, 0xff, 0x4c, 0x43, 0x45, 0x4e]),
} = {}) {
  const parts = [];
  const offsets = {};
  let length = 0;
  function field(name, bytes) {
    offsets[name] = length;
    parts.push(bytes);
    length += bytes.length;
  }
  function text(name, value) {
    const bytes = Buffer.from(value, "utf8");
    field(`${name}Length`, u16(bytes.length));
    field(name, bytes);
  }
  field("magic", Buffer.from("LCEN", "ascii"));
  field("schema", u16(1));
  field("operation", Buffer.from([1]));
  field("authorityKind", Buffer.from([0]));
  field("authorityVersion", u16(1));
  text("runtime", runtime);
  text("revision", revision);
  field("identity", identity);
  text("context", context);
  field("payloadType", Buffer.from([1]));
  field("payloadVersion", u16(1));
  field("payloadLength", u32(payload.length));
  field("payload", payload);
  field("payloadDigest", hash(PAYLOAD_DOMAIN, payload));
  const prefix = Buffer.concat(parts);
  field("bindingDigest", hash(BINDING_DOMAIN, prefix));
  return { bytes: Buffer.concat(parts), offsets };
}

// Здесь сначала разбираются bounded spans, затем содержимое и selectors.
// Этот порядок отличает нормативную первую ошибку от parser-порядка полей.
export function referenceDecode(input) {
  const bytes = Buffer.from(input);
  const refuse = (code) => { throw Object.assign(new Error(code), { code }); };
  if (bytes.length > ENVELOPE_LIMIT) refuse("resource_limit_exceeded");
  let offset = 0;
  function take(length) {
    if (offset + length > bytes.length) refuse("truncated_input");
    const part = bytes.subarray(offset, offset + length);
    offset += length;
    return part;
  }
  const byte = () => take(1)[0];
  const word = () => take(2).readUInt16BE();
  function sized() {
    const length = word();
    return take(length);
  }
  if (!take(4).equals(Buffer.from("LCEN"))) refuse("invalid_magic");
  if (word() !== 1) refuse("unsupported_schema");
  const operation = byte();
  const authorityKind = byte();
  const authorityVersion = word();
  const runtimeBytes = sized();
  const revisionBytes = sized();
  const identity = take(32);
  const contextBytes = sized();
  const payloadType = byte();
  const payloadVersion = word();
  const payloadLength = take(4).readUInt32BE();
  const payload = take(payloadLength);
  const payloadDigest = take(32);
  const bindingOffset = offset;
  const bindingDigest = take(32);
  function length(value, limit) {
    if (value === 0) refuse("invalid_length");
    if (value > limit) refuse("resource_limit_exceeded");
  }
  length(runtimeBytes.length, 128);
  if (revisionBytes.length > 40) refuse("resource_limit_exceeded");
  length(contextBytes.length, 256);
  length(payloadLength, PAYLOAD_LIMIT);
  if (offset !== bytes.length) refuse("trailing_bytes");
  const utf8 = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
  function text(value, forbidNul = true) {
    let decoded;
    try { decoded = utf8.decode(value); } catch { refuse("invalid_utf8"); }
    if (forbidNul && value.includes(0)) refuse("invalid_length");
    return decoded;
  }
  const runtime = text(runtimeBytes);
  const revision = text(revisionBytes, false);
  if (!/^[0-9a-f]{40}$/u.test(revision)) refuse("non_canonical_revision");
  const context = text(contextBytes);
  if (operation !== 1) refuse("unknown_operation");
  if (authorityKind !== 0) refuse("unknown_authority_kind");
  if (authorityVersion !== 1) refuse("unsupported_authority_version");
  if (payloadType !== 1) refuse("invalid_payload_type");
  if (payloadVersion !== 1) refuse("unsupported_payload_version");
  if (!hash(PAYLOAD_DOMAIN, payload).equals(payloadDigest)) refuse("payload_digest_mismatch");
  if (!hash(BINDING_DOMAIN, bytes.subarray(0, bindingOffset)).equals(bindingDigest)) {
    refuse("binding_digest_mismatch");
  }
  return {
    schemaVersion: 1,
    operation: "issue-certificate",
    authorityKind: "generic-typed-certificate",
    authorityVersion: 1,
    runtimeArtifactId: runtime,
    producerRevision: revision,
    producerContentIdentity: [...identity],
    contextId: context,
    payloadType: "non-semantic-transport-v1",
    payloadVersion: 1,
    payloadLength,
    payloadSha256: [...payloadDigest],
    bindingSha256: [...bindingDigest],
  };
}

export function referenceCases() {
  const cases = [];
  const add = (name, result, bytes) => cases.push({ name, result, bytes });
  const base = referencePacket();
  const edit = (name, result, change) => {
    const bytes = Buffer.from(base.bytes);
    change(bytes, base.offsets);
    add(name, result, bytes);
  };
  add("canonical-binary-body", "ok", base.bytes);
  for (const [name, options] of [
    ["one-byte-body", { payload: Buffer.from([0]) }],
    ["all-byte-values", { payload: Buffer.from(Array.from({ length: 256 }, (_, i) => i)) }],
    ["utf8-literal-context", { runtime: "ядро-🧪", context: "контекст-é" }],
    ["utf8-decomposed-context", { context: "e\u0301" }],
    ["utf8-composed-context", { context: "é" }],
    ["utf8-bom-is-literal", { context: "\ufeffcontext" }],
    ["identifier-byte-limits", { runtime: "r".repeat(128), context: "c".repeat(256) }],
    ["different-exact-tuple", { runtime: "other-runtime", context: "other-context", identity: Buffer.alloc(32, 0x71) }],
  ]) add(name, "ok", referencePacket(options).bytes);

  // Полное множество byte-cut позиций одного небольшого canonical packet.
  // Это проверяет реальные field boundaries без случайного seed.
  for (let length = 0; length < base.bytes.length; length += 1) {
    add(`truncated-at-${length}`, "truncated_input", base.bytes.subarray(0, length));
  }
  edit("invalid-magic", "invalid_magic", (b) => { b[0] ^= 1; });
  edit("future-schema", "unsupported_schema", (b, o) => b.writeUInt16BE(2, o.schema));
  edit("unknown-operation", "unknown_operation", (b, o) => { b[o.operation] = 0xff; });
  for (const kind of [1, 2, 3, 255]) {
    edit(`reserved-authority-${kind}`, "unknown_authority_kind", (b, o) => { b[o.authorityKind] = kind; });
  }
  edit("future-authority-version", "unsupported_authority_version", (b, o) => b.writeUInt16BE(2, o.authorityVersion));
  edit("unknown-payload-type", "invalid_payload_type", (b, o) => { b[o.payloadType] = 0xff; });
  edit("future-payload-version", "unsupported_payload_version", (b, o) => b.writeUInt16BE(2, o.payloadVersion));
  add("zero-runtime-length", "invalid_length", referencePacket({ runtime: "" }).bytes);
  add("zero-context-length", "invalid_length", referencePacket({ context: "" }).bytes);
  add("runtime-length-cap", "resource_limit_exceeded", referencePacket({ runtime: "r".repeat(129) }).bytes);
  add("revision-length-cap", "resource_limit_exceeded", referencePacket({ revision: "a".repeat(41) }).bytes);
  add("context-length-cap", "resource_limit_exceeded", referencePacket({ context: "c".repeat(257) }).bytes);
  add("zero-payload-length", "invalid_length", referencePacket({ payload: Buffer.alloc(0) }).bytes);
  edit("payload-cap-with-truncation", "truncated_input", (b, o) => b.writeUInt32BE(PAYLOAD_LIMIT + 1, o.payloadLength));
  edit("payload-length-u32-max", "truncated_input", (b, o) => b.writeUInt32BE(0xffff_ffff, o.payloadLength));
  edit("length-is-big-endian", "truncated_input", (b, o) => b.writeUInt16LE(19, o.runtimeLength));
  edit("runtime-invalid-utf8", "invalid_utf8", (b, o) => { b[o.runtime] = 0xff; });
  edit("context-invalid-utf8", "invalid_utf8", (b, o) => { b[o.context] = 0xff; });
  edit("runtime-nul", "invalid_length", (b, o) => { b[o.runtime] = 0; });
  edit("context-nul", "invalid_length", (b, o) => { b[o.context] = 0; });
  edit("uppercase-revision", "non_canonical_revision", (b, o) => { b[o.revision] = 0x41; });
  edit("nonhex-revision", "non_canonical_revision", (b, o) => { b[o.revision] = 0x67; });
  edit("nul-revision", "non_canonical_revision", (b, o) => { b[o.revision] = 0; });
  edit("payload-replaced", "payload_digest_mismatch", (b, o) => { b[o.payload] ^= 1; });
  edit("payload-digest-replaced", "payload_digest_mismatch", (b, o) => { b[o.payloadDigest] ^= 1; });
  edit("binding-digest-replaced", "binding_digest_mismatch", (b, o) => { b[o.bindingDigest] ^= 1; });
  edit("tuple-substitution", "binding_digest_mismatch", (b, o) => { b[o.identity] ^= 1; });
  edit("context-substitution", "binding_digest_mismatch", (b, o) => { b[o.context] ^= 1; });
  edit("payload-hash-without-domain", "payload_digest_mismatch", (b, o) => {
    hash(b.subarray(o.payload, o.payloadDigest)).copy(b, o.payloadDigest);
  });
  edit("binding-hash-without-domain", "binding_digest_mismatch", (b, o) => {
    hash(b.subarray(0, o.bindingDigest)).copy(b, o.bindingDigest);
  });
  edit("binding-omits-context", "binding_digest_mismatch", (b, o) => {
    hash(BINDING_DOMAIN, b.subarray(0, o.contextLength), b.subarray(o.payloadType, o.bindingDigest)).copy(b, o.bindingDigest);
  });
  add("trailing-field", "trailing_bytes", Buffer.concat([base.bytes, Buffer.from([0])]));
  edit("utf8-before-operation", "invalid_utf8", (b, o) => { b[o.runtime] = 0xff; b[o.operation] = 0xff; });
  edit("revision-before-authority", "non_canonical_revision", (b, o) => { b[o.revision] = 0x41; b[o.authorityKind] = 1; });
  edit("length-before-payload-type", "invalid_length", (b, o) => { b.writeUInt32BE(0, o.payloadLength); b[o.payloadType] = 0xff; });
  const overlong = referencePacket({ runtime: "r".repeat(129) });
  overlong.bytes[overlong.offsets.context] = 0xff;
  add("length-before-utf8", "resource_limit_exceeded", overlong.bytes);
  const truncatedSelector = Buffer.from(base.bytes.subarray(0, 7));
  truncatedSelector[6] = 0xff;
  add("truncation-before-operation", "truncated_input", truncatedSelector);
  return cases;
}

const hex = (bytes) => Buffer.from(bytes).toString("hex");
export function renderReferenceCorpus() {
  const lines = [
    "# LCEN r13: name, outcome, wire hex, runtime UTF-8 hex, revision, content identity hex, context UTF-8 hex, payload bytes, payload SHA-256, binding SHA-256",
    "# Non-semantic transport fixtures. Valid decode confers no producer attestation or authority.",
  ];
  for (const { name, result, bytes } of referenceCases()) {
    let actual;
    let projection;
    try { projection = referenceDecode(bytes); actual = "ok"; } catch (error) { actual = error.code; }
    if (actual !== result) throw new Error(`${name}: independent oracle returned ${actual}, expected ${result}`);
    const metadata = projection === undefined ? Array(7).fill("-") : [
      hex(Buffer.from(projection.runtimeArtifactId, "utf8")), projection.producerRevision,
      hex(projection.producerContentIdentity), hex(Buffer.from(projection.contextId, "utf8")),
      String(projection.payloadLength), hex(projection.payloadSha256), hex(projection.bindingSha256),
    ];
    lines.push([name, result, hex(bytes), ...metadata].join("\t"));
  }
  return `${lines.join("\n")}\n`;
}

export function readReferenceCorpus(text) {
  return text.split("\n").filter((line) => line !== "" && !line.startsWith("#")).map((line) => {
    const fields = line.split("\t");
    if (fields.length !== 10 || !/^(?:[0-9a-f]{2})*$/u.test(fields[2])) {
      throw new Error("Malformed certificate reference corpus row");
    }
    return { name: fields[0], result: fields[1], bytes: Buffer.from(fields[2], "hex") };
  });
}
