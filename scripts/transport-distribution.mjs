import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmod, copyFile, lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const REPOSITORY = "https://github.com/Labpics-Team/lab-colors";
const ATTESTATION_TYPE = "https://in-toto.io/Statement/v1";
const PREDICATE_TYPE = "https://lab.pics/attestations/transport-distribution/v1";
const BENCHMARK_SCHEMA = 1;
const EVIDENCE_SCHEMA = 1;
const WARMUP_ROUNDS = 7;
const MEASURED_ROUNDS = 31;

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
  const output = execFileSync(binary, args, {
    input,
    maxBuffer: 16 * 1024 * 1024,
    stdio: ["pipe", "pipe", "pipe"],
  });
  return { milliseconds: performance.now() - start, output };
}

function parseReferenceWire() {
  const path = resolve(
    REPO_ROOT,
    "crates/labcolors-core/contracts/certificate-envelope-v1/reference-vectors.tsv",
  );
  const text = command("git", ["show", `HEAD:${path.slice(REPO_ROOT.length + 1)}`]);
  const row = text.split("\n").find((line) => line.startsWith("canonical-binary-body\t"));
  if (!row) fail("canonical certificate transport vector is missing");
  const wireHex = row.split("\t")[2];
  if (!wireHex || !/^[0-9a-f]+$/u.test(wireHex) || wireHex.length % 2 !== 0) {
    fail("canonical certificate transport vector is malformed");
  }
  return Buffer.from(wireHex, "hex");
}

function exercise(binary, wire) {
  const parsed = timed(binary, ["parse", "--format", "jsonl", "-"], wire);
  const inspected = timed(binary, ["inspect", "--format", "jsonl", "-"], wire);
  const serialized = timed(binary, ["serialize", "--format", "jsonl", "-"], parsed.output);
  if (!serialized.output.equals(wire)) fail(`${binary}: parse/serialize round-trip drifted`);
  return {
    parse: parsed.milliseconds,
    inspect: inspected.milliseconds,
    serialize: serialized.milliseconds,
  };
}

export function benchmarkPair(candidateBinary, baselineBinary = null) {
  const wire = parseReferenceWire();
  const candidate = { parse: [], inspect: [], serialize: [] };
  const baseline = baselineBinary ? { parse: [], inspect: [], serialize: [] } : null;
  for (let i = 0; i < WARMUP_ROUNDS; i += 1) {
    if (baselineBinary && i % 2 === 0) exercise(baselineBinary, wire);
    exercise(candidateBinary, wire);
    if (baselineBinary && i % 2 !== 0) exercise(baselineBinary, wire);
  }
  for (let i = 0; i < MEASURED_ROUNDS; i += 1) {
    const firstBaseline = baselineBinary && i % 2 === 0;
    const runs = [];
    if (firstBaseline) runs.push(["baseline", baselineBinary]);
    runs.push(["candidate", candidateBinary]);
    if (baselineBinary && !firstBaseline) runs.push(["baseline", baselineBinary]);
    for (const [kind, binary] of runs) {
      const values = exercise(binary, wire);
      const target = kind === "candidate" ? candidate : baseline;
      for (const operation of Object.keys(values)) target[operation].push(values[operation]);
    }
  }
  const result = {
    schemaVersion: BENCHMARK_SCHEMA,
    workload: {
      id: "certificate-envelope-reference-vector-cli-process-v1",
      fixtureBytes: wire.length,
      warmupRounds: WARMUP_ROUNDS,
      measuredRounds: MEASURED_ROUNDS,
      operations: ["parse", "inspect", "serialize"],
      method: "alternating-process-wall-clock-hrtime",
    },
    candidate: Object.fromEntries(Object.entries(candidate).map(([key, values]) => [key, summarize(values)])),
  };
  if (baseline) {
    result.baseline = Object.fromEntries(Object.entries(baseline).map(([key, values]) => [key, summarize(values)]));
    result.pairedMedianRatioCandidateOverBaseline = Object.fromEntries(
      Object.keys(candidate).map((operation) => {
        const ratios = candidate[operation].map((value, index) => value / baseline[operation][index]);
        return [operation, Number(percentile(ratios.sort((a, b) => a - b), 50).toFixed(6))];
      }),
    );
  }
  return result;
}

function packageRef(pkg) {
  const source = pkg.source ?? "workspace";
  return `cargo:${pkg.name}@${pkg.version}#${source}`;
}

function cyclonedxComponent(pkg, sourceSha) {
  const component = {
    type: pkg.name === "labcolors-transport-cli" ? "application" : "library",
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
  const root = metadata.packages.find((pkg) => pkg.name === "labcolors-transport-cli");
  if (!root) fail("cargo metadata has no labcolors-transport-cli package");
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
    version: 1,
    metadata: {
      component: cyclonedxComponent(root, sourceSha),
      properties: [
        { name: "labpics:evidence-kind", value: "transport-distribution" },
        { name: "labpics:source-commit", value: sourceSha },
      ],
    },
    components: packages.map((pkg) => cyclonedxComponent(pkg, sourceSha)),
    dependencies,
  };
}

function licenseInventory(sbom) {
  return {
    schemaVersion: 1,
    components: sbom.components.map((component) => ({
      ref: component["bom-ref"],
      name: component.name,
      version: component.version,
      licenseExpressions: (component.licenses ?? []).map((entry) => entry.expression).sort(),
    })),
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
    fail("transport attestation build target is malformed");
  }
  return target.split("-").includes("windows") ? "labcolors-transport.exe" : "labcolors-transport";
}

export async function verifyBundle(directory, expectedSourceSha) {
  exactSha(expectedSourceSha, "expected source SHA");
  const attestationPath = resolve(directory, "transport.intoto.json");
  const attestationStatus = await lstat(attestationPath);
  if (!attestationStatus.isFile()) fail("transport.intoto.json must be a regular file");
  const attestationBytes = await readFile(attestationPath);
  const attestation = JSON.parse(attestationBytes);
  if (attestation._type !== ATTESTATION_TYPE || attestation.predicateType !== PREDICATE_TYPE) {
    fail("transport attestation has an unsupported type");
  }
  if (!Array.isArray(attestation.subject) || attestation.subject.length !== 1) {
    fail("transport attestation must bind exactly one binary subject");
  }
  const predicate = attestation.predicate;
  if (predicate?.schemaVersion !== EVIDENCE_SCHEMA || predicate?.source?.commit !== expectedSourceSha) {
    fail("transport attestation source identity mismatch");
  }
  if (predicate.source.repository !== REPOSITORY || predicate.build?.noRebuild !== true) {
    fail("transport attestation repository/build contract mismatch");
  }
  const files = predicate.evidence;
  const canonicalPaths = {
    binary: canonicalBinaryName(predicate.build?.target),
    sbom: "transport.sbom.cdx.json",
    licenses: "transport.licenses.json",
    benchmark: "transport.benchmark.json",
  };
  const expectedEntries = ["transport.intoto.json", ...Object.values(canonicalPaths)].sort();
  const actualEntries = (await readdir(directory, { withFileTypes: true }))
    .map((entry) => entry.name)
    .sort();
  if (actualEntries.length !== expectedEntries.length || actualEntries.some((name, index) => name !== expectedEntries[index])) {
    fail("transport distribution directory is not the canonical closed file set");
  }
  const seenPaths = new Set();
  for (const key of ["binary", "sbom", "licenses", "benchmark"]) {
    const record = files?.[key];
    if (!record || !/^[0-9a-f]{64}$/u.test(record.sha256 ?? "") || !Number.isSafeInteger(record.bytes) || record.bytes <= 0) {
      fail(`transport attestation has malformed ${key} evidence`);
    }
    if (record.path !== canonicalPaths[key] || record.path.includes("/") || record.path.includes("\\")) {
      fail(`transport attestation has non-canonical ${key} evidence path`);
    }
    if (seenPaths.has(record.path)) fail("transport attestation reuses an evidence path");
    seenPaths.add(record.path);
    const actual = await fileEvidence(resolve(directory, record.path), record.path);
    if (actual.bytes !== record.bytes || actual.sha256 !== record.sha256) {
      fail(`transport ${key} evidence bytes changed`);
    }
  }
  const binaryDigest = files.binary.sha256;
  const subject = attestation.subject[0];
  if (subject.name !== files.binary.path || subject.digest?.sha256 !== binaryDigest) {
    fail("transport attestation subject does not bind the binary evidence");
  }
  const sbom = JSON.parse(await readFile(resolve(directory, files.sbom.path), "utf8"));
  if (sbom.bomFormat !== "CycloneDX" || sbom.specVersion !== "1.6") fail("transport SBOM is not CycloneDX 1.6");
  const root = sbom.components?.filter((component) => component.name === "labcolors-transport-cli");
  if (!root || root.length !== 1 || root[0].type !== "application") fail("transport SBOM root is not exact");
  if (sbom.components.some((component) => !(component.licenses ?? []).some((entry) => typeof entry.expression === "string" && entry.expression.length > 0))) {
    fail("transport SBOM contains a component without declared license expression");
  }
  const licenses = JSON.parse(await readFile(resolve(directory, files.licenses.path), "utf8"));
  if (licenses.schemaVersion !== 1 || licenses.components?.length !== sbom.components.length) {
    fail("transport license inventory is incomplete");
  }
  const benchmark = JSON.parse(await readFile(resolve(directory, files.benchmark.path), "utf8"));
  if (benchmark.schemaVersion !== BENCHMARK_SCHEMA || benchmark.workload?.measuredRounds !== MEASURED_ROUNDS) {
    fail("transport benchmark receipt is malformed");
  }
  for (const operation of ["parse", "inspect", "serialize"]) {
    const sample = benchmark.candidate?.[operation];
    if (sample?.n !== MEASURED_ROUNDS || !(sample.minMs > 0) || !(sample.p95Ms >= sample.medianMs)) {
      fail(`transport benchmark ${operation} summary is invalid`);
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
  const changes = command("git", ["status", "--porcelain=v1", "--untracked-files=no"]).trim();
  if (changes) fail(`tracked source is dirty and cannot attest ${sourceSha}`);

  const out = resolve(options.out);
  await mkdir(out, { recursive: true });
  const binaryName = process.platform === "win32" ? "labcolors-transport.exe" : "labcolors-transport";
  const binaryPath = resolve(out, binaryName);
  await copyFile(resolve(options.binary), binaryPath);
  await chmod(binaryPath, 0o755);

  const rustcVerbose = command("rustc", ["-vV"]);
  const rustHost = rustHostTriple(rustcVerbose);
  const metadata = JSON.parse(command("cargo", ["metadata", "--locked", "--format-version", "1", "--filter-platform", rustHost]));
  const sbom = buildSbom(metadata, sourceSha);
  const sbomPath = resolve(out, "transport.sbom.cdx.json");
  await writeFile(sbomPath, stableJson(sbom));
  const licensesPath = resolve(out, "transport.licenses.json");
  await writeFile(licensesPath, stableJson(licenseInventory(sbom)));

  const benchmark = benchmarkPair(binaryPath, options.baselineBinary ?? null);
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
  const benchmarkPath = resolve(out, "transport.benchmark.json");
  await writeFile(benchmarkPath, stableJson(benchmark));

  const evidence = {
    binary: await fileEvidence(binaryPath, binaryName),
    sbom: await fileEvidence(sbomPath, "transport.sbom.cdx.json"),
    licenses: await fileEvidence(licensesPath, "transport.licenses.json"),
    benchmark: await fileEvidence(benchmarkPath, "transport.benchmark.json"),
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
  await writeFile(resolve(out, "transport.intoto.json"), stableJson(attestation));
  const verified = await verifyBundle(out, sourceSha);
  return { ...verified, evidence };
}

function parseArgs(argv) {
  const [commandName, ...rest] = argv;
  if (!commandName || !["generate", "verify"].includes(commandName)) {
    fail("usage: transport-distribution.mjs <generate|verify> ...");
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
