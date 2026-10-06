import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  readReferenceCorpus,
  referenceDecode,
  renderReferenceCorpus,
} from "./helpers/certificate-wire-reference.mjs";

const certificateCorpusUrl = new URL(
  "../../../crates/labcolors-core/contracts/certificate-envelope-v1/reference-vectors.tsv",
  import.meta.url,
);

test("pinned LCEN corpus reproduces from the independent wire contract", () => {
  const pinned = readFileSync(certificateCorpusUrl, "utf8");
  assert.equal(pinned, renderReferenceCorpus());
  const cases = readReferenceCorpus(pinned);
  assert.equal(new Set(cases.map(({ name }) => name)).size, cases.length);
  const valid = cases.filter(({ result }) => result === "ok");
  assert(valid.length > 0);
  assert(cases.some(({ result }) => result !== "ok"));
  const composed = referenceDecode(valid.find(({ name }) => name === "utf8-composed-context").bytes);
  const decomposed = referenceDecode(valid.find(({ name }) => name === "utf8-decomposed-context").bytes);
  assert.notEqual(composed.contextId, decomposed.contextId);
  assert.notDeepEqual(composed.bindingSha256, decomposed.bindingSha256);
});
