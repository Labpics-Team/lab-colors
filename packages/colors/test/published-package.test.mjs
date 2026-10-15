import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";
import { downloadPublishedPackage } from "../../../scripts/verify-published-package.mjs";

const bytes = Buffer.from("independently verified archive");
const sha = (algorithm, value, encoding = "hex") => createHash(algorithm).update(value).digest(encoding);
const expected = { name: "@example/colors", version: "1.2.3", sha256: sha("sha256", bytes) };
const archiveUrl = "https://registry.npmjs.org/@example/colors/-/colors-1.2.3.tgz";
const metadataUrl = "https://registry.npmjs.org/%40example%2Fcolors/1.2.3";
const metadata = () => ({ name: expected.name, version: expected.version, dist: {
  tarball: archiveUrl, integrity: `sha512-${sha("sha512", bytes, "base64")}`,
} });

function registry(record = metadata(), archive = () => new Response(bytes)) {
  const calls = [];
  const fetcher = async (url, options) => {
    calls.push(url);
    assert.equal(options.redirect, "error");
    assert.ok(options.signal instanceof AbortSignal);
    assert.equal(options.headers, undefined);
    if (url === metadataUrl) return Response.json(record);
    assert.equal(url, archiveUrl);
    return archive();
  };
  return { calls, fetcher };
}

test("published bytes retain the independent preparation identity", async () => {
  const fixture = registry();
  assert.deepEqual(await downloadPublishedPackage(expected, fixture.fetcher), bytes);
  assert.deepEqual(fixture.calls, [metadataUrl, archiveUrl]);
});

test("a self-consistent substituted archive cannot authorize itself", async () => {
  const other = Buffer.from("substituted archive");
  const record = metadata();
  record.dist.integrity = `sha512-${sha("sha512", other, "base64")}`;
  await assert.rejects(downloadPublishedPackage(expected,
    registry(record, () => new Response(other)).fetcher), /differs from the verified publication/u);
});

test("missing identity refuses before network IO and foreign metadata before archive IO", async () => {
  for (const identity of [undefined, "", "A".repeat(64), "0".repeat(63)]) {
    const fixture = registry();
    await assert.rejects(downloadPublishedPackage({ ...expected, sha256: identity }, fixture.fetcher), /identity is incomplete/u);
    assert.deepEqual(fixture.calls, []);
  }
  for (const key of ["name", "version"]) {
    const record = metadata(); record[key] = "other";
    const fixture = registry(record);
    await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher), /package identity differs/u);
    assert.deepEqual(fixture.calls, [metadataUrl]);
  }
});

test("untrusted locations never receive a second request", async () => {
  for (const url of ["https://other.example/archive.tgz", "http://registry.npmjs.org/archive.tgz",
    "https://user:password@registry.npmjs.org/archive.tgz", archiveUrl + "#fragment"]) {
    const record = metadata(); record.dist.tarball = url;
    const fixture = registry(record);
    await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher), /outside the public registry/u);
    assert.deepEqual(fixture.calls, [metadataUrl]);
  }
});

test("registry failures and mismatched integrity remain explicit failures", async () => {
  for (const status of [404, 429, 500, 503]) {
    const fixture = registry(metadata(), () => new Response("unavailable", { status }));
    await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher), new RegExp(`HTTP ${status}`, "u"));
  }
  const record = metadata(); record.dist.integrity = "sha512-wrong";
  await assert.rejects(downloadPublishedPackage(expected, registry(record).fetcher), /integrity differs/u);
});

test("declared and streamed limits stop oversized responses without accepting a prefix", async () => {
  const fixture = registry(metadata(), () => new Response(bytes, {
    headers: { "content-length": String(16 * 1024 * 1024 + 1) },
  }));
  await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher), /byte limit/u);
  let cancelled = false;
  let reads = 0;
  const overflow = registry(metadata(), () => new Response(new ReadableStream({
    pull(controller) { reads += 1; controller.enqueue(new Uint8Array(1024 * 1024)); },
    cancel() { cancelled = true; },
  }), { headers: { "content-length": "1" } }));
  await assert.rejects(downloadPublishedPackage(expected, overflow.fetcher), /byte limit/u);
  assert.equal(cancelled, true);
  assert.ok(reads <= 19, "readback must stop rather than drain unbounded input");
});

test("publication readback consumes the publisher digest in its own credential-free job", () => {
  const workflow = readFileSync(new URL("../../../.github/workflows/publish-worker.yml", import.meta.url), "utf8");
  const readback = workflow.split("\n  verify-publication:\n");
  assert.equal(readback.length, 2);
  assert.match(readback[0], /tarball_sha256: \$\{\{ steps\.verified-artifact\.outputs\.sha256 \}\}/u);
  assert.match(readback[1], /needs: \[prepare, publish\]/u);
  assert.match(readback[1], /EXPECTED_TARBALL_SHA256: \$\{\{ needs\.publish\.outputs\.tarball_sha256 \}\}/u);
  assert.match(readback[1], /node scripts\/verify-published-package\.mjs/u);
  assert.doesNotMatch(readback[1], /secrets\.|environment:|NODE_AUTH_TOKEN/u);
});
