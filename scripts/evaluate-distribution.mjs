import { createHash } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import { chmod, copyFile, lstat, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";
import { distributionNotices, licenseInventory, verifyLicenseInventory } from "./distribution-licenses.mjs";

const REPO_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const REPOSITORY = "https://github.com/Labpics-Team/lab-colors";
const ATTESTATION_TYPE = "https://in-toto.io/Statement/v1";
const PREDICATE_TYPE = "https://lab.pics/attestations/evaluate-distribution/v1";
const BENCHMARK_SCHEMA = 2;
const EVIDENCE_SCHEMA = 1;
const UUID_DNS_NAMESPACE = Buffer.from("6ba7b8109dad11d180b400c04fd430c8", "hex");
const WARMUP_ROUNDS = 7;
const MEASURED_ROUNDS = 31;
const GOOD_TERMINAL_SRGB8 = [128, 128, 128];
const REJECTED_EXIT = 4;
const FIXTURE_GOOD = "crates/labcolors-evaluate-cli/examples/declared-point.json";
const FIXTURE_REJECTED = "crates/labcolors-evaluate-cli/examples/rejected-point.json";

function fail(message) {
  throw new Error(message);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function stable(value) {
  if (Array.isArray(value)) return value.map(stable);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, stable(value[key])]));
  }
  return value;
}

function stableJson(value) {
  return `${JSON.stringify(stable(value), null, 2)}\n`;
}

function command(name, args, options = {}) {
  try {
    return execFileSync(name, args, {
      cwd: options.cwd ?? REPO_ROOT,
      encoding: options.encoding ?? "utf8",
      input: options.input,
      env: options.env ?? process.env,
      maxBuffer: 64 * 1024 * 1024,
      stdio: options.stdio ?? ["pipe", "pipe", "pipe"],
    });
  } catch (error) {
    const stderr = error?.stderr?.toString().trim();
    const stdout = error?.stdout?.toString().trim();
    fail(`${name} ${args.join(" ")} failed${stderr || stdout ? `: ${[stderr, stdout].filter(Boolean).join("\n")}` : ""}`);
  }
}

function percentile(sorted, p) {
  if (sorted.length === 0) fail("cannot compute percentile of empty sample");
  const rank = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, Math.min(sorted.length - 1, rank))];
}

function summarize(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    n: sorted.length,
    minMs: Number(sorted[0].toFixed(6)),
    medianMs: Number(percentile(sorted, 50).toFixed(6)),
    p95Ms: Number(percentile(sorted, 95).toFixed(6)),
    maxMs: Number(sorted.at(-1).toFixed(6)),
  };
}

function timed(binary, args, input) {
  const start = performance.now();
  const result = spawnSync(binary, args, {
    input,
    maxBuffer: 16 * 1024 * 1024,
    stdio: ["pipe", "pipe", "pipe"],
  });
  if (result.error) fail(`${binary} ${args.join(" ")} failed to spawn`);
  if (result.status !== 0) {
    const stderr = result.stderr?.toString().trim();
    fail(`${binary} ${args.join(" ")} exited ${result.status}${stderr ? `: ${stderr.slice(0, 200)}` : ""}`);
  }
  return { milliseconds: performance.now() - start, output: result.stdout, stderr: result.stderr };
}

function readFixtures(sourceSha) {
  exactSha(sourceSha, "fixture source SHA");
  return {
    sourceSha,
    good: command("git", ["show", `${sourceSha}:${FIXTURE_GOOD}`], { encoding: "buffer" }),
    rejected: command("git", ["show", `${sourceSha}:${FIXTURE_REJECTED}`], { encoding: "buffer" }),
  };
}

function describeFixtures(fixtures) {
  exactSha(fixtures.sourceSha, "fixture source SHA");
  const result = { sourceSha: fixtures.sourceSha };
  for (const [kind, path] of [["good", FIXTURE_GOOD], ["rejected", FIXTURE_REJECTED]]) {
    const bytes = fixtures[kind];
    if (!Buffer.isBuffer(bytes) || bytes.length === 0 || bytes.length > 2 * 1024 * 1024) {
      fail(`${kind}: expected a bounded JSON fixture`);
    }
    const utf8 = bytes.toString("utf8");
    if (!Buffer.from(utf8).equals(bytes)) fail(`${kind}: fixture is not UTF-8`);
    const request = JSON.parse(utf8);
    const release = request?.profile?.conventionReleaseSha256;
    if (!/^[0-9a-f]{64}$/u.test(release ?? "") || request.formatVersion !== 1 ||
      request.kind !== "labcolors-declared-point-request-v1" ||
      request.profile.scope !== "modeled-srgb8-point" ||
      request.profile.admission !== "declared-package-policy-candidate" ||
      request.profile.human !== "not-requested") {
      fail(`${kind}: fixture does not declare the supported nominal point profile`);
    }
    if (kind === "good") result.conventionReleaseSha256 = release;
    else if (release !== result.conventionReleaseSha256) fail("GOOD and REJECTED release identities differ");
    // Parsing is timed too: preserve whitespace, key order and all bytes except
    // the single literal release selector, whose width is always 64 hex digits.
    const selector = /"conventionReleaseSha256"(\s*:\s*)"([a-f0-9]{64})"/gu;
    const matches = [...utf8.matchAll(selector)];
    if (matches.length !== 1 || matches[0][2] !== release) fail(`${kind}: expected one literal release selector`);
    const normalized = utf8.replace(selector, (_match, separator) => `"conventionReleaseSha256"${separator}"${"0".repeat(64)}"`);
    result[kind] = { path, bytes: bytes.length, sha256: sha256(bytes), utf8,
      normalizedSha256: sha256(normalized) };
  }
  return result;
}

export function describeFixturePair(candidate, baseline = null) {
  const result = { candidate: describeFixtures(candidate), baseline: baseline ? describeFixtures(baseline) : null,
    comparison: baseline ? "same-requests-except-convention-release" : "candidate-only" };
  if (result.baseline) {
    for (const kind of ["good", "rejected"]) {
      if (result.candidate[kind].normalizedSha256 !== result.baseline[kind].normalizedSha256) {
        fail(`${kind}: paired workload differs beyond the convention release`);
      }
    }
  }
  return result;
}

function assertGoodReport(stdout, stderr, release, label) {
  if (stderr.length !== 0) fail(`${label}: expected empty stderr`);
  let report;
  try {
    report = JSON.parse(stdout.toString("utf8"));
  } catch {
    fail(`${label}: stdout is not a JSON report`);
  }
  if (report?.formatVersion !== 1 || report.ok !== true || report.kind !== "labcolors-declared-point-report-v1") {
    fail(`${label}: stdout is not a successful declared-point report`);
  }
  if (report.conventionReleaseSha256 !== release || report.scope !== "modeled-srgb8-point" ||
    report.admission !== "declared-package-policy-candidate" || report.human !== "not-requested" ||
    report.rendererProvenance !== "unverified") {
    fail(`${label}: report has the wrong release or claim scope`);
  }
  const terminal = report.terminalSrgb8;
  if (!Array.isArray(terminal) || terminal.length !== GOOD_TERMINAL_SRGB8.length ||
    terminal.some((value, index) => value !== GOOD_TERMINAL_SRGB8[index])) {
    fail(`${label}: terminal sRGB8 is not the declared GOOD point`);
  }
  return report;
}

function exercise(binary, fixtures, fixturePath, release) {
  const viaStdin = timed(binary, [], fixtures.good);
  const stdinReport = assertGoodReport(viaStdin.output, viaStdin.stderr, release, `${binary}: stdin-json`);
  const viaFile = timed(binary, [fixturePath], Buffer.alloc(0));
  assertGoodReport(viaFile.output, viaFile.stderr, release, `${binary}: file-json`);
  if (!viaFile.output.equals(viaStdin.output)) fail(`${binary}: file and stdin reports diverged`);
  const compact = Buffer.from(JSON.stringify(JSON.parse(fixtures.good)));
  const viaJsonl = timed(binary, ["--format", "jsonl", "-"], compact);
  if (viaJsonl.output.filter((byte) => byte === 0x0a).length !== 1) {
    fail(`${binary}: jsonl output is not a single line`);
  }
  const jsonlReport = assertGoodReport(viaJsonl.output, viaJsonl.stderr, release, `${binary}: stdin-jsonl`);
  if (stableJson(jsonlReport) !== stableJson(stdinReport)) fail(`${binary}: jsonl and json reports diverged`);
  return { "stdin-json": viaStdin.milliseconds, "file-json": viaFile.milliseconds, "stdin-jsonl": viaJsonl.milliseconds };
}

function assertRefusal(binary, input, exit, code) {
  const result = spawnSync(binary, [], { input, maxBuffer: 16 * 1024 * 1024, stdio: ["pipe", "pipe", "pipe"] });
  if (result.error) fail(`${binary}: ${code} fixture run failed to spawn`);
  if (result.status !== exit || result.stdout.length !== 0) fail(`${binary}: expected ${code} exit ${exit} and empty stdout`);
  let report;
  try { report = JSON.parse(result.stderr.toString("utf8")); }
  catch { fail(`${binary}: ${code} stderr is not a JSON refusal`); }
  if (stableJson(report) !== stableJson({ formatVersion: 1, ok: false, error: { domain: "clean-convention", code } })) {
    fail(`${binary}: expected typed ${code} refusal`);
  }
}

export async function benchmarkPair(candidateBinary, baselineBinary, sourceSha, baselineSha = null) {
  return benchmarkPairWithFixtures(candidateBinary, baselineBinary, readFixtures(sourceSha),
    baselineSha ? readFixtures(baselineSha) : null);
}

export async function benchmarkPairWithFixtures(candidateBinary, baselineBinary, candidateFixtures, baselineFixtures = null) {
  if (Boolean(baselineBinary) !== Boolean(baselineFixtures)) fail("baseline binary and fixtures must be provided together");
  const fixtures = describeFixturePair(candidateFixtures, baselineFixtures);
  const crossRelease = fixtures.baseline && fixtures.candidate.conventionReleaseSha256 !== fixtures.baseline.conventionReleaseSha256;
  const scratch = await mkdtemp(join(tmpdir(), "labcolors-evaluate-dist-"));
  try {
    const inputs = { candidate: candidateFixtures, baseline: baselineFixtures };
    const paths = { candidate: join(scratch, "candidate.json"), baseline: join(scratch, "baseline.json") };
    const samples = { candidate: { "stdin-json": [], "file-json": [], "stdin-jsonl": [] } };
    const binaries = { candidate: candidateBinary };
    if (baselineBinary) { binaries.baseline = baselineBinary; samples.baseline = { "stdin-json": [], "file-json": [], "stdin-jsonl": [] }; }
    for (const [kind, binary] of Object.entries(binaries)) {
      await writeFile(paths[kind], inputs[kind].good);
      assertRefusal(binary, inputs[kind].rejected, REJECTED_EXIT, "rejected_by_convention");
    }
    if (crossRelease) {
      assertRefusal(candidateBinary, baselineFixtures.good, 3, "unsupported_convention_release");
      assertRefusal(baselineBinary, candidateFixtures.good, 3, "unsupported_convention_release");
    }
    for (let i = 0; i < WARMUP_ROUNDS + MEASURED_ROUNDS; i += 1) {
      const order = baselineBinary ? (i % 2 === 0 ? ["baseline", "candidate"] : ["candidate", "baseline"]) : ["candidate"];
      for (const kind of order) {
        const values = exercise(binaries[kind], inputs[kind], paths[kind], fixtures[kind].conventionReleaseSha256);
        if (i >= WARMUP_ROUNDS) {
          for (const operation of Object.keys(values)) samples[kind][operation].push(values[operation]);
        }
      }
    }
    const result = {
      schemaVersion: BENCHMARK_SCHEMA,
      workload: { id: "evaluate-declared-point-cli-process-v2", warmupRounds: WARMUP_ROUNDS,
        measuredRounds: MEASURED_ROUNDS, operations: ["stdin-json", "file-json", "stdin-jsonl"],
        method: "alternating-process-wall-clock-hrtime", fixtures },
      correctness: { goodTerminalSrgb8: [...GOOD_TERMINAL_SRGB8], rejectedExit: REJECTED_EXIT,
        rejectedCode: "rejected_by_convention", crossReleaseRefusals: crossRelease ? "unsupported_convention_release" : "not-applicable" },
      candidate: Object.fromEntries(Object.entries(samples.candidate).map(([key, values]) => [key, summarize(values)])),
    };
    if (samples.baseline) {
      result.baseline = Object.fromEntries(Object.entries(samples.baseline).map(([key, values]) => [key, summarize(values)]));
      result.pairedRatios = Object.fromEntries(Object.keys(samples.candidate).map((operation) => {
        const ratios = samples.candidate[operation].map((value, index) => value / samples.baseline[operation][index]);
        return [operation, Number(percentile(ratios.sort((a, b) => a - b), 50).toFixed(6))];
      }));
    }
    return result;
  } finally { await rm(scratch, { recursive: true, force: true }); }
}

function packageRef(pkg) {
  const source = pkg.source ?? "workspace";
  return `cargo:${pkg.name}@${pkg.version}#${source}`;
}

function bomSerialNumber(sourceSha) {
  exactSha(sourceSha, "SBOM source SHA");
  const digest = createHash("sha1")
    .update(UUID_DNS_NAMESPACE)
    .update(`${REPOSITORY}\0${sourceSha}\0labcolors-evaluate-cli`, "utf8")
    .digest();
  const bytes = Buffer.from(digest.subarray(0, 16));
  bytes[6] = (bytes[6] & 0x0f) | 0x50;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytes.toString("hex");
  return `urn:uuid:${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function cyclonedxComponent(pkg, sourceSha) {
  const component = {
    type: pkg.name === "labcolors-evaluate-cli" ? "application" : "library",
    "bom-ref": packageRef(pkg),
    name: pkg.name,
    version: pkg.version,
  };
  if (pkg.source?.startsWith("registry+")) {
    component.purl = `pkg:cargo/${encodeURIComponent(pkg.name)}@${encodeURIComponent(pkg.version)}`;
  } else if (pkg.source == null) {
    component.properties = [
      { name: "labpics:source-repository", value: REPOSITORY },
      { name: "labpics:source-commit", value: sourceSha },
    ];
  }
  if (pkg.license) component.licenses = [{ expression: pkg.license }];
  return component;
}

export function buildSbom(metadata, sourceSha) {
  const root = metadata.packages.find((pkg) => pkg.name === "labcolors-evaluate-cli");
  if (!root) fail("cargo metadata has no labcolors-evaluate-cli package");
  const byId = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const nodes = new Map((metadata.resolve?.nodes ?? []).map((node) => [node.id, node]));
  const selected = new Set();
  const queue = [root.id];
  while (queue.length > 0) {
    const id = queue.shift();
    if (selected.has(id)) continue;
    selected.add(id);
    const node = nodes.get(id);
    if (!node) continue;
    for (const dep of node.deps ?? []) {
      const admittedKinds = (dep.dep_kinds ?? []).filter((kind) => kind.kind !== "dev");
      if (admittedKinds.length > 0) queue.push(dep.pkg);
    }
  }
  const packages = [...selected].map((id) => byId.get(id)).filter(Boolean);
  packages.sort((a, b) => packageRef(a).localeCompare(packageRef(b), "en"));
  const refs = new Set(packages.map(packageRef));
  const dependencies = packages.map((pkg) => {
    const node = nodes.get(pkg.id);
    const dependsOn = (node?.deps ?? [])
      .filter((dep) => refs.has(packageRef(byId.get(dep.pkg))))
      .filter((dep) => (dep.dep_kinds ?? []).some((kind) => kind.kind !== "dev"))
      .map((dep) => packageRef(byId.get(dep.pkg)))
      .sort();
    return { ref: packageRef(pkg), dependsOn: [...new Set(dependsOn)] };
  });
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.6",
    serialNumber: bomSerialNumber(sourceSha),
    version: 1,
    metadata: {
      component: cyclonedxComponent(root, sourceSha),
      properties: [
        { name: "labpics:evidence-kind", value: "evaluate-distribution" },
        { name: "labpics:source-commit", value: sourceSha },
      ],
    },
    components: packages.map((pkg) => cyclonedxComponent(pkg, sourceSha)),
    dependencies,
  };
}

async function fileEvidence(path, displayPath = basename(path)) {
  const status = await lstat(path);
  if (!status.isFile()) fail(`${displayPath} must be a regular file`);
  const bytes = await readFile(path);
  if (bytes.length === 0) fail(`${displayPath} is empty`);
  return { path: displayPath, bytes: bytes.length, sha256: sha256(bytes) };
}

function exactSha(value, label) {
  if (!/^[0-9a-f]{40}$/u.test(value ?? "")) fail(`${label} must be a full lowercase Git SHA`);
}

function rustHostTriple(verbose) {
  const match = /^host:\s*(\S+)$/mu.exec(verbose);
  if (!match) fail("rustc -vV did not report a host triple");
  const host = match[1];
  if (!/^[A-Za-z0-9_.+]+(?:-[A-Za-z0-9_.+]+){2,}$/u.test(host)) {
    fail("rustc -vV reported a malformed host triple");
  }
  return host;
}

function canonicalBinaryName(target) {
  if (typeof target !== "string" || !/^[A-Za-z0-9_.+]+(?:-[A-Za-z0-9_.+]+){2,}$/u.test(target)) {
    fail("evaluate attestation build target is malformed");
  }
  return target.split("-").includes("windows") ? "labcolors-evaluate.exe" : "labcolors-evaluate";
}

export async function verifyBundle(directory, expectedSourceSha) {
  exactSha(expectedSourceSha, "expected source SHA");
  const attestationPath = resolve(directory, "evaluate.intoto.json");
  const attestationStatus = await lstat(attestationPath);
  if (!attestationStatus.isFile()) fail("evaluate.intoto.json must be a regular file");
  const attestationBytes = await readFile(attestationPath);
  const attestation = JSON.parse(attestationBytes);
  if (attestation._type !== ATTESTATION_TYPE || attestation.predicateType !== PREDICATE_TYPE) {
    fail("evaluate attestation has an unsupported type");
  }
  if (!Array.isArray(attestation.subject) || attestation.subject.length !== 1) {
    fail("evaluate attestation must bind exactly one binary subject");
  }
  const predicate = attestation.predicate;
  if (predicate?.schemaVersion !== EVIDENCE_SCHEMA || predicate?.source?.commit !== expectedSourceSha) {
    fail("evaluate attestation source identity mismatch");
  }
  if (predicate.source.repository !== REPOSITORY || predicate.build?.noRebuild !== true) {
    fail("evaluate attestation repository/build contract mismatch");
  }
  const files = predicate.evidence;
  const canonicalPaths = {
    binary: canonicalBinaryName(predicate.build?.target),
    sbom: "evaluate.sbom.cdx.json",
    licenses: "evaluate.licenses.json",
    notices: "NOTICES.txt",
    benchmark: "evaluate.benchmark.json",
  };
  const expectedEntries = ["evaluate.intoto.json", ...Object.values(canonicalPaths)].sort();
  const actualEntries = (await readdir(directory, { withFileTypes: true }))
    .map((entry) => entry.name)
    .sort();
  if (actualEntries.length !== expectedEntries.length || actualEntries.some((name, index) => name !== expectedEntries[index])) {
    fail("evaluate distribution directory is not the canonical closed file set");
  }
  const seenPaths = new Set();
  for (const key of ["binary", "sbom", "licenses", "notices", "benchmark"]) {
    const record = files?.[key];
    if (!record || !/^[0-9a-f]{64}$/u.test(record.sha256 ?? "") || !Number.isSafeInteger(record.bytes) || record.bytes <= 0) {
      fail(`evaluate attestation has malformed ${key} evidence`);
    }
    if (record.path !== canonicalPaths[key] || record.path.includes("/") || record.path.includes("\\")) {
      fail(`evaluate attestation has non-canonical ${key} evidence path`);
    }
    if (seenPaths.has(record.path)) fail("evaluate attestation reuses an evidence path");
    seenPaths.add(record.path);
    const actual = await fileEvidence(resolve(directory, record.path), record.path);
    if (actual.bytes !== record.bytes || actual.sha256 !== record.sha256) {
      fail(`evaluate ${key} evidence bytes changed`);
    }
  }
  const binaryDigest = files.binary.sha256;
  const subject = attestation.subject[0];
  if (subject.name !== files.binary.path || subject.digest?.sha256 !== binaryDigest) {
    fail("evaluate attestation subject does not bind the binary evidence");
  }
  const sbom = JSON.parse(await readFile(resolve(directory, files.sbom.path), "utf8"));
  if (
    sbom.bomFormat !== "CycloneDX" ||
    sbom.specVersion !== "1.6" ||
    sbom.serialNumber !== bomSerialNumber(expectedSourceSha)
  ) {
    fail("evaluate SBOM is not attestable CycloneDX 1.6");
  }
  const root = sbom.components?.filter((component) => component.name === "labcolors-evaluate-cli");
  if (!root || root.length !== 1 || root[0].type !== "application") fail("evaluate SBOM root is not exact");
  if (sbom.components.some((component) => !(component.licenses ?? []).some((entry) => typeof entry.expression === "string" && entry.expression.length > 0))) {
    fail("evaluate SBOM contains a component without declared license expression");
  }
  const licenses = JSON.parse(await readFile(resolve(directory, files.licenses.path), "utf8"));
  verifyLicenseInventory(licenses, sbom);
  const benchmark = JSON.parse(await readFile(resolve(directory, files.benchmark.path), "utf8"));
  if (benchmark.schemaVersion !== BENCHMARK_SCHEMA || benchmark.workload?.measuredRounds !== MEASURED_ROUNDS) {
    fail("evaluate benchmark receipt is malformed");
  }
  if (benchmark.workload?.id !== "evaluate-declared-point-cli-process-v2") {
    fail("evaluate benchmark workload is not the declared-point process");
  }
  const recordedFixtures = benchmark.workload.fixtures;
  const decodeFixtures = (record) => {
    if (typeof record?.good?.utf8 !== "string" || typeof record?.rejected?.utf8 !== "string") {
      fail("evaluate benchmark fixture bytes are missing");
    }
    return { sourceSha: record.sourceSha, good: Buffer.from(record.good.utf8), rejected: Buffer.from(record.rejected.utf8) };
  };
  const fixtures = describeFixturePair(decodeFixtures(recordedFixtures?.candidate),
    recordedFixtures?.baseline ? decodeFixtures(recordedFixtures.baseline) : null);
  if (stableJson(recordedFixtures) !== stableJson(fixtures)) fail("evaluate benchmark fixture metadata differs from its bytes");
  if (fixtures.candidate.sourceSha !== expectedSourceSha || benchmark.source?.candidate !== expectedSourceSha ||
    (fixtures.baseline?.sourceSha ?? null) !== benchmark.source?.baseline) {
    fail("evaluate benchmark fixture source identity mismatch");
  }
  const crossRelease = fixtures.baseline && fixtures.candidate.conventionReleaseSha256 !== fixtures.baseline.conventionReleaseSha256;
  const correctness = benchmark.correctness;
  if (!Array.isArray(correctness?.goodTerminalSrgb8) ||
    correctness.goodTerminalSrgb8.length !== GOOD_TERMINAL_SRGB8.length ||
    correctness.goodTerminalSrgb8.some((value, index) => value !== GOOD_TERMINAL_SRGB8[index]) ||
    correctness.rejectedExit !== REJECTED_EXIT || correctness.rejectedCode !== "rejected_by_convention" ||
    correctness.crossReleaseRefusals !== (crossRelease ? "unsupported_convention_release" : "not-applicable")) {
    fail("evaluate benchmark correctness is not the declared GOOD/REJECTED pair");
  }
  if (!fixtures.baseline && (benchmark.baseline !== undefined || benchmark.pairedRatios !== undefined)) {
    fail("evaluate benchmark has unbound baseline measurements");
  }
  for (const kind of fixtures.baseline ? ["candidate", "baseline"] : ["candidate"]) {
    for (const operation of ["stdin-json", "file-json", "stdin-jsonl"]) {
      const sample = benchmark[kind]?.[operation];
      if (sample?.n !== MEASURED_ROUNDS || ![sample.minMs, sample.medianMs, sample.p95Ms, sample.maxMs].every(Number.isFinite) ||
        !(sample.minMs > 0) || !(sample.medianMs >= sample.minMs) || !(sample.p95Ms >= sample.medianMs) || !(sample.maxMs >= sample.p95Ms)) {
        fail(`evaluate benchmark ${kind} ${operation} summary is invalid`);
      }
      if (kind === "baseline" && (!Number.isFinite(benchmark.pairedRatios?.[operation]) || benchmark.pairedRatios[operation] <= 0)) {
        fail(`evaluate benchmark ${operation} paired ratio is invalid`);
      }
    }
  }
  return { attestationSha256: sha256(attestationBytes), binarySha256: binaryDigest };
}

async function generate(options) {
  const sourceSha = options.sourceSha;
  exactSha(sourceSha, "source SHA");
  const head = command("git", ["rev-parse", "HEAD"]).trim();
  if (head !== sourceSha) fail(`checked-out HEAD ${head} != source SHA ${sourceSha}`);
  const treeSha = command("git", ["rev-parse", "HEAD^{tree}"]).trim();
  const changes = command("git", ["status", "--porcelain=v1", "--untracked-files=all"]).trim();
  if (changes) fail(`tracked source is dirty and cannot attest ${sourceSha}`);

  const out = resolve(options.out);
  await mkdir(out, { recursive: true });
  const binaryName = process.platform === "win32" ? "labcolors-evaluate.exe" : "labcolors-evaluate";
  const binaryPath = resolve(out, binaryName);
  await copyFile(resolve(options.binary), binaryPath);
  await chmod(binaryPath, 0o755);

  const rustcVerbose = command("rustc", ["-vV"]);
  const rustHost = rustHostTriple(rustcVerbose);
  const metadata = JSON.parse(command("cargo", ["metadata", "--locked", "--format-version", "1", "--filter-platform", rustHost]));
  const sbom = buildSbom(metadata, sourceSha);
  const sbomPath = resolve(out, "evaluate.sbom.cdx.json");
  await writeFile(sbomPath, stableJson(sbom));
  const licensesPath = resolve(out, "evaluate.licenses.json");
  await writeFile(licensesPath, stableJson(licenseInventory(sbom)));
  const noticesPath = resolve(out, "NOTICES.txt");
  await writeFile(noticesPath, await distributionNotices(metadata, sbom));

  const benchmark = await benchmarkPair(
    binaryPath,
    options.baselineBinary ?? null,
    sourceSha,
    options.baselineSha ?? null,
  );
  benchmark.environment = {
    platform: process.platform,
    arch: process.arch,
    node: process.version,
    rustc: rustcVerbose.trim(),
    cargo: command("cargo", ["-V"]).trim(),
  };
  benchmark.source = {
    candidate: sourceSha,
    baseline: options.baselineSha ?? null,
  };
  const benchmarkPath = resolve(out, "evaluate.benchmark.json");
  await writeFile(benchmarkPath, stableJson(benchmark));

  const evidence = {
    binary: await fileEvidence(binaryPath, binaryName),
    sbom: await fileEvidence(sbomPath, "evaluate.sbom.cdx.json"),
    licenses: await fileEvidence(licensesPath, "evaluate.licenses.json"),
    notices: await fileEvidence(noticesPath, "NOTICES.txt"),
    benchmark: await fileEvidence(benchmarkPath, "evaluate.benchmark.json"),
  };
  const cargoLock = await fileEvidence(resolve(REPO_ROOT, "Cargo.lock"), "Cargo.lock");
  const attestation = {
    _type: ATTESTATION_TYPE,
    subject: [{ name: evidence.binary.path, digest: { sha256: evidence.binary.sha256 } }],
    predicateType: PREDICATE_TYPE,
    predicate: {
      schemaVersion: EVIDENCE_SCHEMA,
      source: { repository: REPOSITORY, commit: sourceSha, tree: treeSha },
      build: {
        profile: "release",
        target: rustHost,
        cargoLockSha256: cargoLock.sha256,
        noRebuild: true,
      },
      evidence,
      boundary: {
        semanticAuthority: false,
        registryPublication: false,
        deployedAdoption: false,
      },
    },
  };
  await writeFile(resolve(out, "evaluate.intoto.json"), stableJson(attestation));
  const verified = await verifyBundle(out, sourceSha);
  return { ...verified, evidence };
}

function parseArgs(argv) {
  const [commandName, ...rest] = argv;
  if (!commandName || !["generate", "verify"].includes(commandName)) {
    fail("usage: evaluate-distribution.mjs <generate|verify> ...");
  }
  const options = {};
  for (let i = 0; i < rest.length; i += 2) {
    const flag = rest[i];
    const value = rest[i + 1];
    if (!flag?.startsWith("--") || value == null) fail(`invalid argument near ${flag ?? "<end>"}`);
    options[flag.slice(2)] = value;
  }
  return { commandName, options };
}

async function main() {
  const { commandName, options } = parseArgs(process.argv.slice(2));
  if (commandName === "generate") {
    for (const required of ["binary", "source-sha", "out"]) if (!options[required]) fail(`--${required} is required`);
    if (Boolean(options["baseline-binary"]) !== Boolean(options["baseline-sha"])) {
      fail("--baseline-binary and --baseline-sha must be provided together");
    }
    const result = await generate({
      binary: options.binary,
      sourceSha: options["source-sha"],
      out: options.out,
      baselineBinary: options["baseline-binary"],
      baselineSha: options["baseline-sha"],
    });
    console.log(stableJson(result).trim());
    return;
  }
  if (!options.dir || !options["source-sha"]) fail("verify requires --dir and --source-sha");
  console.log(stableJson(await verifyBundle(resolve(options.dir), options["source-sha"])).trim());
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
