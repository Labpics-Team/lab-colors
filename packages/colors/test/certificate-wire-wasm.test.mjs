import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import init, { decodeCertificateEnvelope, isCertificateError } from "../index.js";
import { readReferenceCorpus, referenceDecode } from "./helpers/certificate-wire-reference.mjs";

await init({ module_or_path: readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url)) });
const corpus = readReferenceCorpus(readFileSync(new URL(
  "../../../crates/labcolors-core/contracts/certificate-envelope-v1/reference-vectors.tsv",
  import.meta.url,
), "utf8"));

test("real WASM and public Core consume the same independent LCEN corpus", () => {
  for (const { name, result, bytes } of corpus) {
    if (result !== "ok") {
      assert.throws(() => decodeCertificateEnvelope(bytes), (error) => {
        assert.equal(isCertificateError(error), true, name);
        assert.equal(error.code, `certificate_${result}`, name);
        assert.equal(error.operation, "decodeCertificateEnvelope", name);
        assert(Buffer.byteLength(error.message, "utf8") <= 256, name);
        return true;
      }, name);
      continue;
    }
    const actual = decodeCertificateEnvelope(bytes);
    const projection = {
      ...actual,
      producerContentIdentity: [...actual.producerContentIdentity],
      payloadSha256: [...actual.payloadSha256],
      bindingSha256: [...actual.bindingSha256],
    };
    assert.deepEqual(projection, referenceDecode(bytes), name);
    assert.equal(Object.isFrozen(actual), true, name);
    assert.equal("admit" in actual, false, name);
    assert.equal("attestation" in actual, false, name);
  }
});
