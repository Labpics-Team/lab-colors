import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, {
  MAX_CERTIFICATE_ENVELOPE_BYTES,
  decodeCertificateEnvelope,
  isCertificateError,
} from "../index.js";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });

const revision = "0123456789abcdef0123456789abcdef01234567";

function u16(value) {
  const output = Buffer.alloc(2);
  output.writeUInt16BE(value);
  return output;
}

function u32(value) {
  const output = Buffer.alloc(4);
  output.writeUInt32BE(value);
  return output;
}

function text(value) {
  const bytes = Buffer.from(value, "utf8");
  return Buffer.concat([u16(bytes.length), bytes]);
}

function sha256(...parts) {
  const hash = createHash("sha256");
  for (const part of parts) hash.update(part);
  return hash.digest();
}

// Independent reference construction: this deliberately does not call any
// lab-colors encoder or reuse generated binding code.
function referenceEnvelope() {
  const payload = Buffer.from("reference-body", "utf8");
  const payloadDigest = sha256(
    Buffer.from("labpics.colors/certificate-payload/v1\0", "ascii"),
    payload,
  );
  const prefix = Buffer.concat([
    Buffer.from("LCEN", "ascii"),
    u16(1),
    Buffer.from([0x01, 0x00]),
    u16(1),
    text("labcolors-wasm-test"),
    text(revision),
    Buffer.alloc(32, 0x11),
    text("context-v1"),
    Buffer.from([0x01]),
    u16(1),
    u32(payload.length),
    payload,
    payloadDigest,
  ]);
  return Buffer.concat([
    prefix,
    sha256(
      Buffer.from("labpics.colors/certificate-envelope/v1\0", "ascii"),
      prefix,
    ),
  ]);
}

test("WASM decodes an independently constructed envelope as untrusted metadata", () => {
  const bytes = referenceEnvelope();
  const envelope = decodeCertificateEnvelope(bytes);
  assert.equal(envelope.schemaVersion, 1);
  assert.equal(envelope.operation, "issue-certificate");
  assert.equal(envelope.authorityKind, "generic-typed-certificate");
  assert.equal(envelope.authorityVersion, 1);
  assert.equal(envelope.runtimeArtifactId, "labcolors-wasm-test");
  assert.equal(envelope.producerRevision, revision);
  assert.deepEqual([...envelope.producerContentIdentity], Array(32).fill(0x11));
  assert.equal(envelope.contextId, "context-v1");
  assert.equal(envelope.payloadType, "non-semantic-transport-v1");
  assert.equal(envelope.payloadVersion, 1);
  assert.equal(envelope.payloadLength, Buffer.byteLength("reference-body"));
  assert.equal(envelope.payloadSha256.length, 32);
  assert.equal(envelope.bindingSha256.length, 32);
  assert.equal(Object.isFrozen(envelope), true);
  assert.equal("admit" in envelope, false);

  const contentIdentity = [...envelope.producerContentIdentity];
  const payloadDigest = [...envelope.payloadSha256];
  const bindingDigest = [...envelope.bindingSha256];
  envelope.producerContentIdentity[0] ^= 0xff;
  envelope.payloadSha256[0] ^= 0xff;
  envelope.bindingSha256[0] ^= 0xff;
  const reread = decodeCertificateEnvelope(bytes);
  assert.notStrictEqual(envelope.producerContentIdentity, reread.producerContentIdentity);
  assert.notStrictEqual(envelope.payloadSha256, reread.payloadSha256);
  assert.notStrictEqual(envelope.bindingSha256, reread.bindingSha256);
  assert.deepEqual([...reread.producerContentIdentity], contentIdentity);
  assert.deepEqual([...reread.payloadSha256], payloadDigest);
  assert.deepEqual([...reread.bindingSha256], bindingDigest);
});

test("WASM rejects malformed bytes with a typed, redacted error", () => {
  assert.throws(
    () => decodeCertificateEnvelope(new Uint8Array([1, 2, 3])),
    (error) => {
      assert.equal(isCertificateError(error), true);
      assert.equal(error.code, "certificate_truncated_input");
      assert.equal(error.operation, "decodeCertificateEnvelope");
      assert.equal(error.message.includes("1,2,3"), false);
      return true;
    },
  );
});

test("WASM ingress refuses oversized input before linear-memory copy", () => {
  const input = new Uint8Array(MAX_CERTIFICATE_ENVELOPE_BYTES + 1);
  assert.throws(
    () => decodeCertificateEnvelope(input),
    (error) => {
      assert.equal(isCertificateError(error), true);
      assert.equal(error.code, "certificate_resource_limit_exceeded");
      return true;
    },
  );
  const oversizedSubclass = new (class extends Uint8Array {})(
    MAX_CERTIFICATE_ENVELOPE_BYTES + 1,
  );
  assert.throws(
    () => decodeCertificateEnvelope(oversizedSubclass),
    (error) => {
      assert.equal(isCertificateError(error), true);
      assert.equal(error.code, "certificate_resource_limit_exceeded");
      return true;
    },
  );
  assert.throws(
    () => decodeCertificateEnvelope({ byteLength: 1 }),
    (error) => {
      assert.equal(isCertificateError(error), true);
      assert.equal(error.code, "certificate_invalid_input");
      return true;
    },
  );
  const revoked = Proxy.revocable(new Uint8Array(), {});
  revoked.revoke();
  assert.throws(
    () => decodeCertificateEnvelope(revoked.proxy),
    (error) => {
      assert.equal(isCertificateError(error), true);
      assert.equal(error.code, "certificate_invalid_input");
      return true;
    },
  );
  const lyingSubclass = new (class extends Uint8Array {
    get byteLength() {
      return 1;
    }

    get length() {
      return 1_073_741_824;
    }
  })(1);
  assert.throws(
    () => decodeCertificateEnvelope(lyingSubclass),
    (error) => {
      assert.equal(isCertificateError(error), true);
      assert.equal(error.code, "certificate_invalid_input");
      return true;
    },
  );
  const lyingInstance = new Uint8Array(1);
  Object.defineProperties(lyingInstance, {
    byteLength: { value: 1 },
    length: { value: 1_073_741_824 },
  });
  assert.throws(
    () => decodeCertificateEnvelope(lyingInstance),
    (error) => {
      assert.equal(isCertificateError(error), true);
      assert.equal(error.code, "certificate_invalid_input");
      return true;
    },
  );
});

test("certificate error classifier is closed over operation and code", () => {
  assert.equal(
    isCertificateError({
      name: "Error",
      message: "candidate",
      code: "certificate_truncated_input",
      operation: "decodeCertificateEnvelope",
    }),
    false,
  );
  assert.equal(
    isCertificateError(
      Object.assign(new Error("candidate"), {
        code: "certificate_truncated_input",
        operation: "other",
      }),
    ),
    false,
  );
});
