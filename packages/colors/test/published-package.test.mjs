import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { access, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { runInNewContext } from "node:vm";
import {
  downloadPublishedPackage, waitForPublishedPackage, verifyPublishedPackage,
} from "../../../scripts/verify-published-package.mjs";

const bytes = Buffer.from("independently verified archive");
const sha = (algorithm, value, encoding = "hex") => createHash(algorithm).update(value).digest(encoding);
const expected = { name: "@example/colors", version: "1.2.3", sha256: sha("sha256", bytes), gitSha: "a".repeat(40) };
const archiveUrl = "https://registry.npmjs.org/@example/colors/-/colors-1.2.3.tgz";
const metadataUrl = "https://registry.npmjs.org/%40example%2Fcolors/1.2.3";
const attestationUrl = "https://registry.npmjs.org/-/npm/v1/attestations/@example%2fcolors@1.2.3";
const slsaType = "https://slsa.dev/provenance/v1";
const repository = "https://github.com/Labpics-Team/lab-colors";
const tag = "refs/tags/colors-v1.2.3";
const provenance = () => ({
  _type: "https://in-toto.io/Statement/v1",
  subject: [{ name: "pkg:npm/%40example/colors@1.2.3", digest: { sha512: sha("sha512", bytes) } }],
  predicateType: slsaType,
  predicate: {
    buildDefinition: {
      buildType: "https://slsa-framework.github.io/github-actions-buildtypes/workflow/v1",
      externalParameters: { workflow: { ref: tag, repository, path: ".github/workflows/publish.yml" } },
      internalParameters: { github: { event_name: "push" } },
      resolvedDependencies: [{ uri: `git+${repository}@${tag}`, digest: { gitCommit: expected.gitSha } }],
    },
    runDetails: { builder: { id: "https://github.com/actions/runner/github-hosted" } },
  },
});
const attestations = (statement = provenance()) => ({ attestations: [{
  predicateType: slsaType,
  bundle: { mediaType: "application/vnd.dev.sigstore.bundle.v0.3+json", dsseEnvelope: {
    payloadType: "application/vnd.in-toto+json",
    payload: Buffer.from(JSON.stringify(statement)).toString("base64"),
    signatures: [{ sig: "fixture-signature" }],
  } },
}] });
const metadata = () => ({ name: expected.name, version: expected.version, dist: {
  tarball: archiveUrl, integrity: `sha512-${sha("sha512", bytes, "base64")}`,
  attestations: { url: attestationUrl, provenance: { predicateType: slsaType } },
} });

function registry(record = metadata(), archive = () => new Response(bytes), signed = attestations()) {
  const calls = [];
  const fetcher = async (url, options) => {
    calls.push(url);
    assert.equal(options.redirect, "error");
    assert.ok(options.signal instanceof AbortSignal);
    assert.equal(options.headers, undefined);
    if (url === metadataUrl) return Response.json(record);
    if (url === attestationUrl) return Response.json(signed);
    assert.equal(url, archiveUrl);
    return archive();
  };
  return { calls, fetcher };
}

test("published bytes retain the independent preparation identity", async () => {
  const fixture = registry();
  assert.deepEqual((await downloadPublishedPackage(expected, fixture.fetcher)).bytes, bytes);
  assert.deepEqual(fixture.calls, [metadataUrl, archiveUrl, attestationUrl]);
});

test("one npm response owns the selected signed SLSA payload", async () => {
  const signed = attestations();
  signed.attestations.unshift({ predicateType: "https://github.com/npm/attestation/tree/main/specs/publish/v0.1", bundle: {} });
  const fixture = registry(metadata(), () => new Response(bytes), signed);
  const snapshot = await downloadPublishedPackage(expected, fixture.fetcher);
  assert.deepEqual(snapshot.bundle, signed.attestations[1].bundle);
  assert.deepEqual(fixture.calls, [metadataUrl, archiveUrl, attestationUrl]);
});

test("empty, non-SLSA and ambiguous attestations refuse without retries", async () => {
  for (const signed of [{}, { attestations: [] }, { attestations: [{ predicateType: "other", bundle: {} }] },
    { attestations: [...attestations().attestations, ...attestations().attestations] }]) {
    const fixture = registry(metadata(), () => new Response(bytes), signed);
    await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher, async () => assert.fail("semantic retry")), /SLSA/u);
    assert.deepEqual(fixture.calls, [metadataUrl, archiveUrl, attestationUrl]);
  }
});

test("attestation endpoint is restricted to this exact public package version", async () => {
  for (const url of ["https://foreign.example/attestation", archiveUrl, attestationUrl.replace("1.2.3", "1.2.4"),
    attestationUrl + "?redirect=other", attestationUrl + "#fragment", attestationUrl.replace("https:", "http:")]) {
    const record = metadata(); record.dist.attestations.url = url;
    const fixture = registry(record);
    await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher), /attestation URL/u);
    assert.deepEqual(fixture.calls, [metadataUrl]);
  }
});

test("signed policy binds subject, archive, source, recipe and release tag", async () => {
  const changes = [
    [(row) => { row._type = "https://in-toto.io/Statement/v0.1"; }, /statement type/u],
    [(row) => { row.predicateType = "other"; }, /predicate type/u],
    [(row) => { row.subject = []; }, /subject/u],
    [(row) => { row.subject.push(row.subject[0]); }, /subject/u],
    [(row) => { row.subject[0].name = "pkg:npm/%40example/foreign@1.2.3"; }, /subject/u],
    [(row) => { row.subject[0].digest.sha512 = "0".repeat(128); }, /digest/u],
    [(row) => { row.predicate.buildDefinition.buildType = "other"; }, /build type/u],
    [(row) => { row.predicate.buildDefinition.externalParameters.workflow.repository = "https://github.com/foreign/colors"; }, /workflow/u],
    [(row) => { row.predicate.buildDefinition.externalParameters.workflow.ref = "refs/tags/colors-v1.2.4"; }, /workflow/u],
    [(row) => { row.predicate.buildDefinition.externalParameters.workflow.path = ".github/workflows/foreign.yml"; }, /workflow/u],
    [(row) => { row.predicate.buildDefinition.resolvedDependencies[0].uri = `git+${repository}@refs/heads/main`; }, /source/u],
    [(row) => { row.predicate.buildDefinition.resolvedDependencies[0].digest.gitCommit = "b".repeat(40); }, /source/u],
    [(row) => { row.predicate.buildDefinition.resolvedDependencies = []; }, /source/u],
    [(row) => { row.predicate.buildDefinition.internalParameters.github.event_name = "workflow_dispatch"; }, /trigger/u],
  ];
  for (const [edit, problem] of changes) {
    const row = provenance(); edit(row);
    const fixture = registry(metadata(), () => new Response(bytes), attestations(row));
    await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher, async () => assert.fail("semantic retry")), problem);
    assert.deepEqual(fixture.calls, [metadataUrl, archiveUrl, attestationUrl]);
  }
});

test("official verifier receives the same bounded bundle, exact signer and archive before the consumer", async () => {
  const signed = attestations();
  const fixture = registry(metadata(), () => new Response(bytes), signed);
  const calls = [];
  let temporaryRoot;
  const cosignBin = join(tmpdir(), "pinned-release-cosign");
  await verifyPublishedPackage(expected, {
    fetcher: fixture.fetcher, cosignBin,
    executeVerifier: async (command, args, options) => {
      calls.push("verifier");
      assert.equal(command, cosignBin);
      assert.deepEqual(args.slice(0, 2), ["verify-blob-attestation", "--bundle"]);
      temporaryRoot = dirname(args[2]);
      assert.deepEqual(JSON.parse(await readFile(args[2], "utf8")), signed.attestations[0].bundle);
      assert.deepEqual(args.slice(3, -1), [
        "--certificate-oidc-issuer", "https://token.actions.githubusercontent.com",
        "--certificate-identity", `${repository}/.github/workflows/publish-worker.yml@${tag}`,
        "--type", slsaType,
      ]);
      assert.deepEqual(await readFile(args.at(-1)), bytes);
      assert.equal(options.shell, false);
      assert.equal(options.timeout, 180_000);
      assert.ok(options.maxBuffer <= 1024 * 1024);
    },
    smoke: async (tarball) => {
      calls.push("consumer");
      assert.equal(dirname(tarball), temporaryRoot);
      assert.deepEqual(await readFile(tarball), bytes);
    },
  });
  assert.deepEqual(calls, ["verifier", "consumer"]);
  assert.deepEqual(fixture.calls, [metadataUrl, archiveUrl, attestationUrl]);
  await assert.rejects(access(temporaryRoot), { code: "ENOENT" });
});

test("verifier refusal, including wrong certificate identity, never invokes a consumer", async () => {
  for (const reason of ["signature rejected", "certificate identity rejected"]) {
    const fixture = registry();
    let temporaryRoot;
    let verifications = 0;
    let consumers = 0;
    await assert.rejects(verifyPublishedPackage(expected, {
      fetcher: fixture.fetcher, cosignBin: join(tmpdir(), "pinned-release-cosign"),
      executeVerifier: (command, args) => {
        verifications += 1;
        temporaryRoot = dirname(args[2]);
        throw new Error(reason);
      },
      smoke: async () => { consumers += 1; },
    }), new RegExp(reason, "u"));
    assert.equal(verifications, 1);
    assert.equal(consumers, 0);
    assert.deepEqual(fixture.calls, [metadataUrl, archiveUrl, attestationUrl]);
    await assert.rejects(access(temporaryRoot), { code: "ENOENT" });
  }
});

test("invalid signed policy stops before verifier and consumer execution", async () => {
  const row = provenance(); row.predicate.buildDefinition.resolvedDependencies[0].digest.gitCommit = "b".repeat(40);
  const fixture = registry(metadata(), () => new Response(bytes), attestations(row));
  await assert.rejects(verifyPublishedPackage(expected, {
    fetcher: fixture.fetcher, cosignBin: join(tmpdir(), "pinned-release-cosign"),
    executeVerifier: () => assert.fail("invalid policy reached verifier"),
    smoke: () => assert.fail("invalid policy reached consumer"),
  }), /source differs/u);
});

test("readback refuses missing pinned verifier before registry IO", async () => {
  for (const cosignBin of [undefined, "", "cosign"]) {
    const fixture = registry();
    await assert.rejects(verifyPublishedPackage(expected, {
      fetcher: fixture.fetcher, cosignBin: cosignBin ?? "",
    }), /COSIGN_BIN path is required/u);
    assert.deepEqual(fixture.calls, []);
  }
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
    await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher, async () => {}), new RegExp(`HTTP ${status}`, "u"));
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
  const install = readback[1].slice(readback[1].indexOf("- name: install pinned provenance verifier"),
    readback[1].indexOf("- name: install the exact public archive"));
  assert.match(install, /https:\/\/github\.com\/sigstore\/cosign\/releases\/download\/v3\.1\.4\/cosign-linux-amd64/u);
  assert.match(install, /424e2c97d0b45fd7a6b608a87f090f35633f4c23ece5f092d48bba11aaf4bbfa/u);
  assert.match(install, /sha256sum --check --strict/u);
  assert.match(install, /COSIGN_BIN=%s/u);
  assert.ok(install.indexOf("sha256sum --check --strict") < install.indexOf("chmod 700"));
  assert.ok(install.indexOf("chmod 700") < install.indexOf("COSIGN_BIN=%s"));
  assert.match(readback[1], /timeout-minutes: 30/u);
  assert.doesNotMatch(readback[1], /secrets\.|environment:|NODE_AUTH_TOKEN/u);
});

test("transient registry failures recover with a bounded retry budget", async () => {
  const delays = [];
  const healthy = registry();
  let attempts = 0;
  const fetcher = async (url, options) => {
    if (url === metadataUrl && attempts++ < 2) return new Response(null, { status: attempts === 1 ? 404 : 503 });
    return healthy.fetcher(url, options);
  };
  assert.deepEqual((await downloadPublishedPackage(expected, fetcher, async (ms) => { delays.push(ms); })).bytes, bytes);
  assert.equal(attempts, 3);
  assert.deepEqual(delays, [1000, 2000]);
  let failures = 0;
  await assert.rejects(downloadPublishedPackage(expected, async () => {
    failures += 1;
    throw new DOMException("request timed out", "TimeoutError");
  }, async () => {}), /timed out/u);
  assert.equal(failures, 3);
});

test("semantic refusal never becomes successful through retries", async () => {
  const record = metadata(); record.dist.integrity = "sha512-wrong";
  const fixture = registry(record);
  const delays = [];
  await assert.rejects(downloadPublishedPackage(expected, fixture.fetcher, async (ms) => { delays.push(ms); }), /integrity differs/u);
  assert.deepEqual(fixture.calls, [metadataUrl, archiveUrl]);
  assert.deepEqual(delays, []);
});

test("trusted npm publishing grants OIDC only to the verified publisher", () => {
  const caller = readFileSync(new URL("../../../.github/workflows/publish.yml", import.meta.url), "utf8");
  const worker = readFileSync(new URL("../../../.github/workflows/publish-worker.yml", import.meta.url), "utf8");
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.match(caller, /name: publish\n/u);
  assert.match(caller, /uses: \.\/\.github\/workflows\/publish-worker\.yml/u);
  assert.match(caller, /id-token: write/u);
  assert.doesNotMatch(caller, /secrets:|NPM_TOKEN|secrets: inherit/u);
  const [top, afterPrepare] = worker.split("\n  prepare:\n");
  assert.equal(top.split("workflow_call:").length, 2);
  assert.doesNotMatch(top, /NPM_TOKEN|id-token: write|secrets:/u);
  const [prepare, afterPublish] = afterPrepare.split("\n  publish:\n");
  const [publish, verify] = afterPublish.split("\n  verify-publication:\n");
  assert.ok(prepare && publish && verify);
  assert.doesNotMatch(prepare, /id-token: write|NPM_TOKEN|environment:/u);
  assert.match(publish, /environment: npm-publish/u);
  assert.match(publish, /permissions:\n      contents: read\n      actions: read\n      id-token: write/u);
  assert.match(publish, /ACTIONS_ID_TOKEN_REQUEST_URL/u);
  assert.match(publish, /ACTIONS_ID_TOKEN_REQUEST_TOKEN/u);
  assert.match(publish, /legacy npm token is not permitted/u);
  assert.doesNotMatch(publish, /registry-url: "https:\/\/registry\.npmjs\.org"|secrets\.NPM_TOKEN/u);
  assert.doesNotMatch(verify, /id-token: write|NPM_TOKEN|environment: npm-publish/u);
  assert.equal(pkg.repository.url, "https://github.com/Labpics-Team/lab-colors");
  assert.deepEqual(pkg.publishConfig, { access: "public" });
  assert.match(pkg.packageManager, /^npm@11\./u);
});

test("an ancestor release tag retains its own recipe after main advances", async () => {
  const caller = readFileSync(new URL("../../../.github/workflows/publish.yml", import.meta.url), "utf8");
  const worker = readFileSync(new URL("../../../.github/workflows/publish-worker.yml", import.meta.url), "utf8");
  const localRecipe = /^    uses: \.\/\.github\/workflows\/publish-worker\.yml$/mu;
  assert.match(caller, localRecipe);
  for (const foreign of [
    "Labpics-Team/lab-colors/.github/workflows/publish-worker.yml@main",
    "foreign/colors/.github/workflows/publish-worker.yml@main",
  ]) {
    assert.doesNotMatch(caller.replace("./.github/workflows/publish-worker.yml", foreign), localRecipe);
  }
  const step = worker.split("- name: guard — exact tag SHA is in origin/main\n")[1].split("\n      - ")[0];
  const script = step.split("node <<'NODE'\n")[1].split("\n          NODE")[0]
    .split("\n").map((line) => line.slice(10)).join("\n");
  const tag = "a".repeat(40), main = "b".repeat(40);
  for (const [status, mergeBase, allowed] of [["ahead", tag, true], ["diverged", main, false]]) {
    const process = { env: {
      GITHUB_REPOSITORY: "Labpics-Team/lab-colors", GITHUB_SHA: tag,
      GITHUB_API_URL: "https://github-api.example.invalid", GH_READ_TOKEN: "fixture-read-token",
    } };
    const errors = [];
    await runInNewContext(script, {
      process,
      fetch: async (url) => {
        assert.equal(url, process.env.GITHUB_API_URL + "/repos/Labpics-Team/lab-colors/compare/" + tag + "...main");
        return { ok: true, json: async () => ({
          status, base_commit: { sha: tag }, head_commit: { sha: main }, merge_base_commit: { sha: mergeBase },
        }) };
      },
      console: { log() {}, error(value) { errors.push(String(value)); } },
    }, { timeout: 1000, filename: "release-tag-ancestry.cjs" });
    assert.equal(process.exitCode ?? 0, allowed ? 0 : 1);
    assert.equal(errors.length, allowed ? 0 : 1);
    if (!allowed) assert.match(errors[0], /outside main history/u);
  }
});

test("npm scanning delay eventually admits only the verified public archive", async () => {
  const calls = [];
  const waits = [];
  let misses = 0;
  const healthy = registry();
  const fetcher = async (url, options) => {
    calls.push(url);
    if (url === metadataUrl && misses++ < 3) {
      return new Response(null, { status: 404 });
    }
    return healthy.fetcher(url, options);
  };
  const archive = await waitForPublishedPackage(expected, fetcher, async (ms) => { waits.push(ms); });
  assert.deepEqual(archive.bytes, bytes);
  assert.deepEqual(waits, [15_000, 15_000, 15_000]);
  assert.equal(calls.filter((url) => url === metadataUrl).length, 4);
  assert.equal(calls.filter((url) => url === archiveUrl).length, 1);
});

test("publication availability has a finite wall-clock and attempt budget", async () => {
  let ticks = 0;
  const waits = [];
  let attempts = 0;
  const notVisible = async () => { attempts += 1; return new Response(null, { status: 404 }); };
  await assert.rejects(
    waitForPublishedPackage(expected, notVisible, async (ms) => {
      waits.push(ms); ticks += ms;
    }, () => ticks, 30_000),
    /HTTP 404/u,
  );
  assert.deepEqual(waits, [15_000, 15_000]);
  assert.equal(attempts, 3);
  let foreverAttempts = 0;
  await assert.rejects(
    waitForPublishedPackage(expected, async () => {
      foreverAttempts += 1;
      return new Response(null, { status: 503 });
    }, async () => {}, () => 0),
    /HTTP 503/u,
  );
  assert.equal(foreverAttempts, 81);
  await assert.rejects(waitForPublishedPackage(expected, notVisible,
    async () => {}, () => 0, -1), /invalid publication visibility budget/u);
});

test("visibility waiting cannot hide compromised source, archive or authorization", async () => {
  const delays = [];
  for (const [metadataRow, archived, problem] of [
    [{ ...metadata(), name: "some-other-package" }, bytes, /package identity differs/u],
    [metadata(), Buffer.from("an unrelated archive"), /verified publication/u],
    [{ ...metadata(), dist: { ...metadata().dist, integrity: "sha512-broken" } }, bytes, /integrity differs/u],
  ]) {
    const fixture = registry(metadataRow, () => new Response(archived));
    await assert.rejects(waitForPublishedPackage(expected, fixture.fetcher,
      async (ms) => { delays.push(ms); }), problem);
    assert.deepEqual(fixture.calls, [metadataUrl].concat(metadataRow.name === expected.name ? [archiveUrl] : []));
  }
  let calls = 0;
  await assert.rejects(waitForPublishedPackage(expected, async () => {
    calls += 1;
    return new Response(null, { status: 401 });
  }, async (ms) => { delays.push(ms); }), /HTTP 401/u);
  assert.equal(calls, 1);
  assert.deepEqual(delays, []);
});
