import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  appendFile,
  mkdtemp,
  mkdir,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

import { workspaceVersion } from "./cargo-workspace.mjs";
import { PACKAGE_DIR, REPO_ROOT, prepareNpmPackage } from "./prepare-npm-package.mjs";

const RELEASE_DIR = resolve(PACKAGE_DIR, ".release");
const RELEASE_MANIFEST = resolve(RELEASE_DIR, "release-manifest.json");
const PACKAGE_JSON = resolve(PACKAGE_DIR, "package.json");
const PACKAGE_LOCK = resolve(PACKAGE_DIR, "package-lock.json");
const BUILD_METADATA = resolve(PACKAGE_DIR, "build-metadata.json");
const ROOT_CARGO = resolve(REPO_ROOT, "Cargo.toml");
const CONFORMANCE_DIR = resolve(REPO_ROOT, "conformance/vectors");
const CONFORMANCE_MANIFEST = resolve(CONFORMANCE_DIR, "manifest.json");
const WCAG22_CONTRACT_DIR = resolve(REPO_ROOT, "crates/labcolors-core/contracts");
const WCAG22_EVIDENCE_FILES = [
  "wcag22-srgb8-v1.json",
  "wcag22-srgb8-q55-v1.bin",
  "wcag22-srgb8-q55-proof-v1.json",
];
const CONFORMANCE_FAMILY_FILES = [
  "contrasts.json",
  "ladders.json",
  "alpha.json",
  "solve.json",
  "muddiness.json",
  "wcag22.json",
  "wcag22-feasibility.json",
];
const WASM_PATH = resolve(PACKAGE_DIR, "pkg/labcolors_bg.wasm");

const REQUIRED_PACK_FILES = ["package.json", "README.md", "LICENSE"];
const FORBIDDEN_PACK_SEGMENTS = new Set([
  ".git",
  ".github",
  ".release",
  "bench",
  "node_modules",
  "scripts",
  "test",
]);

function fail(message) {
  throw new Error(message);
}

export const RELEASE_COMMAND_TIMEOUT_MS = 5 * 60 * 1_000;

export function command(
  commandName,
  args,
  cwd = REPO_ROOT,
  { timeoutMs = RELEASE_COMMAND_TIMEOUT_MS } = {},
) {
  try {
    return execFileSync(commandName, args, {
      cwd,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
      timeout: timeoutMs,
      stdio: ["ignore", "pipe", "pipe"],
    });
  } catch (error) {
    const stderr = error?.stderr?.toString().trim();
    const stdout = error?.stdout?.toString().trim();
    const detail = [stderr, stdout].filter(Boolean).join("\n");
    const outcome =
      error?.code === "ETIMEDOUT" ? `timed out after ${timeoutMs} ms` : "failed";
    fail(`${commandName} ${args.join(" ")} ${outcome}${detail ? `:\n${detail}` : ""}`);
  }
}

function npm(args, cwd = REPO_ROOT) {
  // npm exposes the exact CLI entrypoint to lifecycle scripts. Falling back to
  // PATH keeps direct `node scripts/verify-package-release.mjs` usable after a
  // normal Node installation, while a missing npm remains a hard failure.
  if (process.env.npm_execpath) {
    return command(process.execPath, [process.env.npm_execpath, ...args], cwd);
  }
  return command(process.platform === "win32" ? "npm.cmd" : "npm", args, cwd);
}

async function readJson(path) {
  const source = await readFile(path, "utf8");
  try {
    return JSON.parse(source);
  } catch (error) {
    fail(`${relative(REPO_ROOT, path)} is not valid JSON: ${error.message}`);
  }
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function hashedArtifact(path, displayPath) {
  const bytes = await readFile(path);
  if (bytes.length === 0) fail(`${displayPath} is empty`);
  return { path: displayPath, bytes: bytes.length, sha256: sha256(bytes) };
}

export async function validateWcag22EvidenceArtifacts(
  root,
  expectedArtifacts,
  label,
) {
  const allowedPaths = WCAG22_EVIDENCE_FILES.map((file) => `evidence/${file}`);
  if (!Array.isArray(expectedArtifacts) || expectedArtifacts.length !== allowedPaths.length) {
    fail(`${label} WCAG22 evidence expectation must contain ${allowedPaths.length} artifacts`);
  }
  const expectedByPath = new Map();
  for (const artifact of expectedArtifacts) {
    if (
      !allowedPaths.includes(artifact?.path) ||
      !Number.isSafeInteger(artifact?.bytes) ||
      artifact.bytes <= 0 ||
      !/^[0-9a-f]{64}$/u.test(artifact?.sha256 ?? "") ||
      expectedByPath.has(artifact.path)
    ) {
      fail(`${label} has malformed or duplicate WCAG22 evidence metadata`);
    }
    expectedByPath.set(artifact.path, artifact);
  }

  const actualArtifacts = [];
  for (const file of WCAG22_EVIDENCE_FILES) {
    const displayPath = `evidence/${file}`;
    const expected = expectedByPath.get(displayPath);
    if (!expected) fail(`${label} lacks expected WCAG22 evidence metadata: ${displayPath}`);
    const [canonical, actual] = await Promise.all([
      readFile(resolve(WCAG22_CONTRACT_DIR, file)),
      readFile(resolve(root, "evidence", file)),
    ]);
    if (!actual.equals(canonical)) {
      fail(`${label} WCAG22 evidence bytes differ from canonical source: ${displayPath}`);
    }
    const metadata = {
      path: displayPath,
      bytes: actual.length,
      sha256: sha256(actual),
    };
    if (metadata.bytes !== expected.bytes || metadata.sha256 !== expected.sha256) {
      fail(
        `${label} WCAG22 evidence metadata differs for ${displayPath}: ` +
          `expected ${expected.bytes}B/${expected.sha256}, ` +
          `actual ${metadata.bytes}B/${metadata.sha256}`,
      );
    }
    actualArtifacts.push(metadata);
  }
  return actualArtifacts;
}

async function validateWcag22Evidence() {
  const artifacts = [];
  for (const file of WCAG22_EVIDENCE_FILES) {
    artifacts.push(
      await hashedArtifact(
        resolve(WCAG22_CONTRACT_DIR, file),
        `evidence/${file}`,
      ),
    );
  }
  await validateWcag22EvidenceArtifacts(PACKAGE_DIR, artifacts, "staged package");

  const profilePath = resolve(WCAG22_CONTRACT_DIR, WCAG22_EVIDENCE_FILES[0]);
  const profileBytes = await readFile(profilePath);
  const profile = await readJson(profilePath);
  const binary = await readFile(resolve(WCAG22_CONTRACT_DIR, WCAG22_EVIDENCE_FILES[1]));
  const proof = await readJson(resolve(WCAG22_CONTRACT_DIR, WCAG22_EVIDENCE_FILES[2]));
  if (profile.profileId !== "wcag22-srgb8-contrast-v1" || proof.profile_id !== profile.profileId) {
    fail("WCAG22 profile/proof identity drifted");
  }
  if (binary.length !== 768 * 2 * 8) {
    fail(`WCAG22 Q55 artifact has ${binary.length} bytes, expected 12288`);
  }
  if (proof.profile_source_sha256 !== sha256(profileBytes)) {
    fail("WCAG22 proof does not bind the canonical profile bytes");
  }
  if (proof.artifact_sha256 !== sha256(binary)) {
    fail("WCAG22 proof does not bind the canonical Q55 artifact bytes");
  }
  if (
    proof.artifact_id !== "wcag22-srgb8-luminance-q55-v1" ||
    proof.bound_id !== "wcag22-srgb8-outward-q55-v1" ||
    proof.proof_id !== "wcag22-srgb8-full-domain-q55-v1" ||
    proof.kernel_id !== "wcag22-srgb8-evaluation-kernel-v1" ||
    proof.terminal_evidence_id !== "wcag22-srgb8-terminal-evidence-v1" ||
    proof.parser_id !== "encoded-srgb8-hex-parser-v1" ||
    proof.facade_id !== "wcag22-srgb8-public-facade-v1" ||
    proof.declared_operation_law !==
      "final-srgb8-outward-q55-two-orientation-integer-threshold-v1"
  ) {
    fail("WCAG22 proof typed identity or operation law drifted");
  }
  if (!/^[0-9a-f]{8}$/u.test(proof.profile_checksum ?? "")) {
    fail("WCAG22 proof lacks a typed profile checksum");
  }
  if (
    proof.schema_version !== 2 ||
    proof.source_binding_schema_version !== 1 ||
    proof.source_binding_law !== "wcag22-rust-semantic-dependency-cone-v1" ||
    !/^[0-9a-f]{64}$/u.test(proof.source_route_sha256 ?? "")
  ) {
    fail("WCAG22 proof lacks the versioned semantic source binding");
  }
  if (Object.hasOwn(proof, "crate_lib_source_sha256")) {
    fail("WCAG22 proof still binds unrelated whole-crate source bytes");
  }
  if (proof.rows !== 768 || proof.artifact_words !== 1536 || proof.colors !== 16_777_216) {
    fail("WCAG22 proof has incomplete row or finite-domain coverage");
  }
  if (
    !Array.isArray(proof.thresholds) ||
    proof.thresholds.length !== 2 ||
    !proof.thresholds.every((threshold) => threshold.unresolved === 0)
  ) {
    fail("WCAG22 proof contains an unresolved supported threshold");
  }
  return {
    profileId: profile.profileId,
    profileChecksum: proof.profile_checksum,
    artifactId: proof.artifact_id,
    boundId: proof.bound_id,
    proofId: proof.proof_id,
    kernelId: proof.kernel_id,
    terminalEvidenceId: proof.terminal_evidence_id,
    parserId: proof.parser_id,
    facadeId: proof.facade_id,
    artifacts,
  };
}

export function validateBuildMetadata(
  metadata,
  { packageJson, source, coreVersion, conformanceEvidence, wasm },
) {
  const expected = {
    schemaVersion: 1,
    package: { name: packageJson.name, version: packageJson.version },
    sourceSha: source,
    coreVersion,
    conformance: {
      packVersion: conformanceEvidence.packVersion,
      packDigest: conformanceEvidence.packDigest,
      manifestSha256: conformanceEvidence.manifestSha256,
      familySetSha256: conformanceEvidence.familySetSha256,
    },
    wasm: { bytes: wasm.bytes, sha256: wasm.sha256 },
  };
  if (!isDeepStrictEqual(metadata, expected)) {
    fail(
      "generated build-metadata.json does not exactly bind the release inputs:\n" +
        `expected ${JSON.stringify(expected)}\n` +
        `actual   ${JSON.stringify(metadata)}`,
    );
  }
}

function normalisePackPath(path) {
  const normal = path.replaceAll("\\", "/").replace(/^\.\//u, "");
  if (!normal || normal.startsWith("../") || normal.includes("/../")) {
    fail(`unsafe path reported by npm pack: ${path}`);
  }
  return normal;
}

function exportTargets(value, into = []) {
  if (typeof value === "string") {
    if (value.startsWith("./")) into.push(value.slice(2));
    return into;
  }
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) exportTargets(child, into);
  }
  return into;
}

function validatePackedFiles(packageJson, packResult) {
  if (!Array.isArray(packResult.files) || packResult.files.length === 0) {
    fail("npm pack did not report a non-empty files inventory");
  }

  const actual = packResult.files.map((entry) => normalisePackPath(entry.path));
  const duplicates = actual.filter((path, index) => actual.indexOf(path) !== index);
  if (duplicates.length > 0) fail(`npm pack reported duplicate paths: ${duplicates.join(", ")}`);

  const expected = new Set([
    ...REQUIRED_PACK_FILES,
    ...(packageJson.files ?? []).map(normalisePackPath),
  ]);
  for (const target of exportTargets(packageJson.exports)) expected.add(normalisePackPath(target));
  if (typeof packageJson.types === "string") expected.add(normalisePackPath(packageJson.types));

  const actualSet = new Set(actual);
  const missing = [...expected].filter((path) => !actualSet.has(path)).sort();
  if (missing.length > 0) fail(`npm tarball is missing required files: ${missing.join(", ")}`);

  const undeclared = actual.filter((path) => !expected.has(path)).sort();
  if (undeclared.length > 0) {
    fail(`npm tarball contains undeclared files: ${undeclared.join(", ")}`);
  }

  for (const path of actual) {
    const segments = path.split("/");
    const forbidden = segments.find((segment) => FORBIDDEN_PACK_SEGMENTS.has(segment));
    if (forbidden) fail(`npm tarball contains forbidden ${forbidden} path: ${path}`);
  }
}

async function packInto(destination, packageJson) {
  await mkdir(destination, { recursive: true });
  const packedJson = npm(
    ["pack", "--ignore-scripts", "--json", `--pack-destination=${destination}`, PACKAGE_DIR],
    REPO_ROOT,
  ).trim();
  let packed;
  try {
    packed = JSON.parse(packedJson);
  } catch (error) {
    fail(`npm pack --json returned invalid JSON: ${error.message}`);
  }
  if (!Array.isArray(packed) || packed.length !== 1) {
    fail(`npm pack returned ${Array.isArray(packed) ? packed.length : "non-array"} results`);
  }

  const packResult = packed[0];
  if (packResult.name !== packageJson.name || packResult.version !== packageJson.version) {
    fail(
      `npm pack produced ${packResult.name}@${packResult.version}, ` +
        `expected ${packageJson.name}@${packageJson.version}`,
    );
  }
  const tarballName = basename(packResult.filename ?? "");
  if (!tarballName.endsWith(".tgz") || tarballName !== packResult.filename) {
    fail(`npm pack returned an unsafe tarball filename: ${packResult.filename}`);
  }
  validatePackedFiles(packageJson, packResult);

  return { path: resolve(destination, tarballName), tarballName };
}

async function validatePackedWcag22Evidence(tarballPath, expectedArtifacts) {
  const extracted = await mkdtemp(join(tmpdir(), "labcolors-packed-evidence-"));
  try {
    command("tar", ["-xzf", tarballPath, "-C", extracted]);
    await validateWcag22EvidenceArtifacts(
      resolve(extracted, "package"),
      expectedArtifacts,
      "npm tarball",
    );
  } finally {
    await rm(extracted, { recursive: true, force: true });
  }
}

function fnv1a32(buffers) {
  let hash = 0x811c9dc5;
  for (const buffer of buffers) {
    for (const byte of buffer) {
      hash ^= byte;
      hash = Math.imul(hash, 0x01000193) >>> 0;
    }
  }
  return hash.toString(16).padStart(8, "0");
}

// Canonical checksum preimage capability-манифеста (ядро:
// labcolors-core/src/numerics.rs, canonical_checksum_preimage). Домен-сепаратор
// и length-prefixed кодирование повторены здесь НЕЗАВИСИМО: релизный гейт не
// доверяет закоммиченному checksum, а пересчитывает его из тех же typed rows.
const CAPABILITY_CHECKSUM_DOMAIN_V2 = "labcolors.numerical-capability.v2";
// Поля-списки одного site в каноническом порядке preimage (порядок фиксирован
// схемой v2 и не выводится из JSON, чтобы переименование ключа ломало гейт).
const CAPABILITY_SITE_LIST_FIELDS = [
  "stableOutcomes",
  "compatibilityReleases",
  "evidenceClasses",
  "artifactIds",
  "boundIds",
  "proofIds",
  "runtimeAttestations",
];

function u32le(value) {
  const buffer = Buffer.alloc(4);
  buffer.writeUInt32LE(value >>> 0, 0);
  return buffer;
}

function lenPrefixed(bytes) {
  return [u32le(bytes.length), bytes];
}

// Сортировка по СЫРЫМ UTF-8 байтам (как sort_unstable по &[u8] в ядре), а не по
// UTF-16 code units JS-строк — для не-ASCII ключей порядки расходятся.
function compareUtf8(a, b) {
  return Buffer.compare(Buffer.from(a, "utf8"), Buffer.from(b, "utf8"));
}

function capabilityChecksumPreimage(capabilities) {
  const chunks = [];
  chunks.push(...lenPrefixed(Buffer.from(CAPABILITY_CHECKSUM_DOMAIN_V2, "utf8")));
  chunks.push(u32le(capabilities.schemaVersion));
  chunks.push(...lenPrefixed(Buffer.from(capabilities.coverage, "utf8")));
  const sites = [...capabilities.sites].sort((a, b) => compareUtf8(a.siteId, b.siteId));
  chunks.push(u32le(sites.length));
  for (const site of sites) {
    chunks.push(...lenPrefixed(Buffer.from(site.siteId, "utf8")));
    for (const field of CAPABILITY_SITE_LIST_FIELDS) {
      // Пустой список кодируется явным count=0 — отсутствие evidence является
      // частью контракта, а не пропуском.
      const keys = [...site[field]].sort(compareUtf8);
      chunks.push(u32le(keys.length));
      for (const key of keys) chunks.push(...lenPrefixed(Buffer.from(key, "utf8")));
    }
  }
  return chunks;
}

// Структурная (generic) валидация numericalCapabilities: форма, coverage и
// независимый пересчёт drift-checksum. Никакого hardcode конкретного site —
// точный состав rows держит exact-проекция ядра (reference_runner conformance-
// крейта); релизный гейт проверяет, что закоммиченный manifest самосогласован.
function validateCapabilityManifest(capabilities) {
  if (typeof capabilities !== "object" || capabilities === null || Array.isArray(capabilities)) {
    fail("conformance manifest has no numericalCapabilities object");
  }
  if (capabilities.schemaVersion !== 2) {
    fail(
      `numericalCapabilities schemaVersion ${capabilities.schemaVersion} is not the supported 2`,
    );
  }
  if (capabilities.coverage !== "migrated-sites-only-v1") {
    fail(`numericalCapabilities coverage must be migrated-sites-only-v1, got ${capabilities.coverage}`);
  }
  if (!Array.isArray(capabilities.sites) || capabilities.sites.length === 0) {
    fail("numericalCapabilities must list at least one migrated site");
  }
  const isKeyList = (value) =>
    Array.isArray(value) && value.every((key) => typeof key === "string" && key.length > 0);
  const siteIds = new Set();
  for (const site of capabilities.sites) {
    if (typeof site.siteId !== "string" || site.siteId.length === 0) {
      fail("numericalCapabilities site lacks a non-empty siteId");
    }
    if (siteIds.has(site.siteId)) fail(`duplicate numericalCapabilities siteId ${site.siteId}`);
    siteIds.add(site.siteId);
    for (const field of CAPABILITY_SITE_LIST_FIELDS) {
      if (!isKeyList(site[field])) {
        fail(`numericalCapabilities site ${site.siteId} has malformed ${field}`);
      }
    }
    if (site.stableOutcomes.length === 0) {
      fail(`numericalCapabilities site ${site.siteId} declares no lawful stable outcome`);
    }
  }
  if (!/^[0-9a-f]{8}$/u.test(capabilities.checksum ?? "")) {
    fail(`invalid numericalCapabilities checksum: ${capabilities.checksum}`);
  }
  const recomputed = fnv1a32(capabilityChecksumPreimage(capabilities));
  if (recomputed !== capabilities.checksum) {
    fail(
      `numericalCapabilities checksum ${capabilities.checksum} does not bind the ` +
        `canonical preimage (independent recompute: ${recomputed})`,
    );
  }
}

const FEASIBILITY_VECTOR_KEYS_V1 = ["caseId", "outcomeJson", "requestJson"];
const FEASIBILITY_CASES_V1 = new Map([
  ["text-default-seven", { terminal: "feasible", count: 7 }],
  ["text-default-two", { terminal: "feasible", count: 2 }],
  ["text-default-zero", { terminal: "infeasible", count: 0 }],
  ["text-large-scale-ninety-two", { terminal: "feasible", count: 92 }],
  ["ui-component-ninety-two", { terminal: "feasible", count: 92 }],
  ["graphical-object-ninety-two", { terminal: "feasible", count: 92 }],
  ["ui-component-fifty-nine", { terminal: "feasible", count: 59 }],
  ["mixed-not-applicable", { terminal: "feasible" }],
  ["all-not-applicable", { terminal: "notEvaluated" }],
  ["conflicting-relation-id", { failure: "conflict" }],
  ["raw-adjacent-resource-rejection", { failure: "resource" }],
  ["opaque-identity-a", { terminal: "feasible" }],
  ["opaque-identity-b", { terminal: "feasible" }],
]);
const FEASIBILITY_CRITERIA_V1 = new Set([
  "sc-1.4.3-text-default",
  "sc-1.4.3-text-large-scale",
  "sc-1.4.11-ui-component-or-state",
  "sc-1.4.11-graphical-object",
]);
const FEASIBILITY_PROPORTIONAL_KEYS_V1 = new Set([
  "assessments",
  "cells",
  "feasibleCandidates",
  "infeasibleCandidates",
]);
const FEASIBILITY_PROOF_KEYS_V1 = [
  "applicableEdges",
  "applicableRelations",
  "artifactId",
  "boundId",
  "canonicalRelations",
  "domainCount",
  "domainDigest",
  "domainFirst",
  "domainId",
  "domainLast",
  "evaluationId",
  "logicalAssessments",
  "matrixDigest",
  "notApplicableRelations",
  "partition",
  "proofId",
  "proofSha256",
  "relationSetDigest",
  "resourceProfileId",
  "wcag22ProfileId",
];
const FEASIBILITY_DOMAIN_SEPARATOR_V1 = Buffer.from(
  "labcolors/wcag22-feasibility/domain/v1\0",
  "utf8",
);
const FEASIBILITY_RELATION_SEPARATOR_V1 = Buffer.from(
  "labcolors/wcag22-feasibility/relations/v1\0",
  "utf8",
);
const FEASIBILITY_EVALUATION_SEPARATOR_V1 = Buffer.from(
  "labcolors/wcag22-feasibility/evaluation/v1\0",
  "utf8",
);

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value, expected, label) {
  if (!isRecord(value)) fail(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  const canonical = [...expected].sort();
  if (!isDeepStrictEqual(actual, canonical)) {
    fail(`${label} fields ${JSON.stringify(actual)} differ from ${JSON.stringify(canonical)}`);
  }
}

function canonicalJson(source, label) {
  if (typeof source !== "string" || source.length === 0) fail(`${label} must be non-empty JSON text`);
  let value;
  try {
    value = JSON.parse(source);
  } catch (error) {
    fail(`${label} is not valid JSON: ${error.message}`);
  }
  if (JSON.stringify(value) !== source) fail(`${label} is not canonical compact JSON`);
  return value;
}

function decimalU64(value, label) {
  if (typeof value !== "string" || !/^(?:0|[1-9][0-9]*)$/u.test(value)) {
    fail(`${label} must be a canonical decimal u64 string`);
  }
  const parsed = BigInt(value);
  if (parsed > 18_446_744_073_709_551_615n) fail(`${label} exceeds u64`);
  return parsed;
}

function byteArray(value, expectedLength, label) {
  if (
    !Array.isArray(value) ||
    value.length !== expectedLength ||
    !value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)
  ) {
    fail(`${label} must contain exactly ${expectedLength} byte values`);
  }
}

function u64be(value, label) {
  const parsed = BigInt(value);
  if (parsed < 0n || parsed > 18_446_744_073_709_551_615n) {
    fail(`${label} is outside u64`);
  }
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64BE(parsed);
  return bytes;
}

function updateLengthPrefixed(hasher, value, label) {
  const bytes = Buffer.from(value, "utf8");
  hasher.update(u64be(bytes.length, `${label}.length`));
  hasher.update(bytes);
}

function requireDigest(actual, expected, label) {
  byteArray(actual, 32, label);
  if (!Buffer.from(actual).equals(expected)) {
    fail(`${label} does not bind its canonical preimage`);
  }
}

function neutralAxisDomainV1() {
  return Array.from({ length: 256 }, (_, value) => [value, value, value]);
}

function feasibilityDomainDigestV1(domainId, domain) {
  const hasher = createHash("sha256");
  hasher.update(FEASIBILITY_DOMAIN_SEPARATOR_V1);
  updateLengthPrefixed(hasher, domainId, "domainId");
  hasher.update(u64be(domain.length, "domainCount"));
  for (const candidate of domain) hasher.update(Buffer.from(candidate));
  return hasher.digest();
}

function feasibilityRelationSetDigestV1(relations) {
  const hasher = createHash("sha256");
  hasher.update(FEASIBILITY_RELATION_SEPARATOR_V1);
  hasher.update(u64be(relations.length, "canonicalRelations"));
  for (const relation of relations) {
    if (relation.kind === "applicable") {
      hasher.update(Buffer.from([1]));
      updateLengthPrefixed(hasher, relation.relationId, "relationId");
      updateLengthPrefixed(hasher, relation.occurrenceId, "occurrenceId");
      updateLengthPrefixed(hasher, relation.criterion, "criterion");
      hasher.update(u64be(relation.adjacent.length, "adjacentCount"));
      for (const adjacent of relation.adjacent) hasher.update(Buffer.from(adjacent));
    } else {
      hasher.update(Buffer.from([2]));
      updateLengthPrefixed(hasher, relation.relationId, "relationId");
      updateLengthPrefixed(hasher, relation.occurrenceId, "occurrenceId");
      updateLengthPrefixed(hasher, relation.reasonId, "reasonId");
    }
  }
  return hasher.digest();
}

function feasibilityEvaluationIdV1({ proof, counts, matrixDigest, atomicProofSha256 }) {
  const hasher = createHash("sha256");
  hasher.update(FEASIBILITY_EVALUATION_SEPARATOR_V1);
  hasher.update(Buffer.from(proof.domainDigest));
  hasher.update(Buffer.from(proof.relationSetDigest));
  for (const [field, value] of [
    ["wcag22ProfileId", proof.wcag22ProfileId],
    ["artifactId", proof.artifactId],
    ["boundId", proof.boundId],
    ["proofId", proof.proofId],
  ]) {
    updateLengthPrefixed(hasher, value, field);
  }
  hasher.update(atomicProofSha256);
  for (const [field, value] of [
    ["canonicalRelations", counts.canonicalRelations],
    ["applicableRelations", counts.applicableRelations],
    ["notApplicableRelations", counts.notApplicableRelations],
    ["applicableEdges", counts.applicableEdges],
    ["logicalAssessments", counts.logicalAssessments],
    ["packedResultBytes", counts.packedResultBytes],
  ]) {
    hasher.update(u64be(value, field));
  }
  hasher.update(matrixDigest);
  hasher.update(Buffer.from(proof.partition));
  return hasher.digest();
}

function rgb(value, label) {
  byteArray(value, 3, label);
}

function walkNoProportionalDto(value, label) {
  if (Array.isArray(value)) {
    for (const child of value) walkNoProportionalDto(child, label);
    return;
  }
  if (!isRecord(value)) return;
  for (const [key, child] of Object.entries(value)) {
    if (FEASIBILITY_PROPORTIONAL_KEYS_V1.has(key)) {
      fail(`${label} contains forbidden proportional field ${key}`);
    }
    walkNoProportionalDto(child, label);
  }
}

function validateFeasibilityRelation(relation, label) {
  if (relation?.kind === "applicable") {
    exactKeys(
      relation,
      ["adjacent", "criterion", "kind", "occurrenceId", "relationId"],
      label,
    );
    if (!FEASIBILITY_CRITERIA_V1.has(relation.criterion)) {
      fail(`${label} has unsupported criterion ${relation.criterion}`);
    }
    if (!Array.isArray(relation.adjacent) || relation.adjacent.length === 0) {
      fail(`${label} must declare non-empty adjacency`);
    }
    relation.adjacent.forEach((value, index) => rgb(value, `${label}.adjacent[${index}]`));
  } else if (relation?.kind === "notApplicable") {
    exactKeys(relation, ["kind", "occurrenceId", "reasonId", "relationId"], label);
    if (typeof relation.reasonId !== "string" || relation.reasonId.length === 0) {
      fail(`${label} must declare a non-empty reasonId`);
    }
  } else {
    fail(`${label} has unsupported kind ${relation?.kind}`);
  }
  for (const field of ["relationId", "occurrenceId"]) {
    if (typeof relation[field] !== "string" || relation[field].length === 0) {
      fail(`${label}.${field} must be a non-empty opaque string`);
    }
  }
}

function validateFeasibilityRequest(source, caseId) {
  const request = canonicalJson(source, `${caseId}.requestJson`);
  exactKeys(
    request,
    ["domainId", "relations", "resourceProfileId", "schemaVersion"],
    `${caseId}.request`,
  );
  if (
    request.schemaVersion !== 1 ||
    request.domainId !== "srgb8-neutral-axis-v1" ||
    request.resourceProfileId !== "compile-v1"
  ) {
    fail(`${caseId}.request has unsupported version/domain/profile`);
  }
  if (!Array.isArray(request.relations) || request.relations.length === 0) {
    fail(`${caseId}.request must contain relations`);
  }
  request.relations.forEach((relation, index) =>
    validateFeasibilityRelation(relation, `${caseId}.request.relations[${index}]`));
  return request;
}

function compareRgb(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

function canonicalRelations(relations) {
  const values = relations.map((relation) => {
    if (relation.kind !== "applicable") return structuredClone(relation);
    const adjacent = [...relation.adjacent].sort(compareRgb);
    const unique = adjacent.filter(
      (value, index) => index === 0 || compareRgb(value, adjacent[index - 1]) !== 0,
    );
    return { ...structuredClone(relation), adjacent: unique };
  });
  values.sort((left, right) =>
    Buffer.compare(Buffer.from(left.relationId, "utf8"), Buffer.from(right.relationId, "utf8")));
  return values.filter(
    (value, index) => index === 0 || !isDeepStrictEqual(value, values[index - 1]),
  );
}

function packedBit(bytes, logicalIndex) {
  return (bytes[Math.floor(logicalIndex / 8)] & (1 << (logicalIndex % 8))) !== 0;
}

function validateEvaluatedFeasibility(
  result,
  request,
  status,
  caseId,
  atomicProofSha256,
) {
  exactKeys(result, ["domain", "failureMatrix", "proof", "relations"], `${caseId}.result`);
  if (!Array.isArray(result.domain) || result.domain.length !== 256) {
    fail(`${caseId}.domain must contain exactly 256 candidates`);
  }
  result.domain.forEach((candidate, index) => {
    rgb(candidate, `${caseId}.domain[${index}]`);
    if (!isDeepStrictEqual(candidate, [index, index, index])) {
      fail(`${caseId}.domain candidate ${index} is not the registered neutral-axis value`);
    }
  });
  if (!Array.isArray(result.relations) || result.relations.length === 0) {
    fail(`${caseId}.result must retain canonical relations once`);
  }
  result.relations.forEach((relation, index) =>
    validateFeasibilityRelation(relation, `${caseId}.result.relations[${index}]`));
  if (!isDeepStrictEqual(result.relations, canonicalRelations(request.relations))) {
    fail(`${caseId}.result relations differ from independent canonical request projection`);
  }

  const proof = result.proof;
  exactKeys(proof, FEASIBILITY_PROOF_KEYS_V1, `${caseId}.proof`);
  for (const [field, value] of [
    ["evaluationId", proof.evaluationId],
    ["domainDigest", proof.domainDigest],
    ["relationSetDigest", proof.relationSetDigest],
    ["matrixDigest", proof.matrixDigest],
    ["partition", proof.partition],
    ["proofSha256", proof.proofSha256],
  ]) {
    byteArray(value, 32, `${caseId}.proof.${field}`);
  }
  rgb(proof.domainFirst, `${caseId}.proof.domainFirst`);
  rgb(proof.domainLast, `${caseId}.proof.domainLast`);
  if (
    proof.resourceProfileId !== "compile-v1" ||
    proof.domainId !== "srgb8-neutral-axis-v1" ||
    proof.wcag22ProfileId !== "wcag22-srgb8-contrast-v1" ||
    proof.artifactId !== "wcag22-srgb8-luminance-q55-v1" ||
    proof.boundId !== "wcag22-srgb8-outward-q55-v1" ||
    proof.proofId !== "wcag22-srgb8-full-domain-q55-v1"
  ) {
    fail(`${caseId}.proof typed identities drifted`);
  }
  if (
    !isDeepStrictEqual(proof.domainFirst, [0, 0, 0]) ||
    !isDeepStrictEqual(proof.domainLast, [255, 255, 255])
  ) {
    fail(`${caseId}.proof domain endpoints drifted`);
  }
  requireDigest(
    proof.domainDigest,
    feasibilityDomainDigestV1(proof.domainId, result.domain),
    `${caseId}.proof.domainDigest`,
  );
  requireDigest(
    proof.relationSetDigest,
    feasibilityRelationSetDigestV1(result.relations),
    `${caseId}.proof.relationSetDigest`,
  );
  requireDigest(proof.proofSha256, atomicProofSha256, `${caseId}.proof.proofSha256`);

  const counts = Object.fromEntries(
    [
      "domainCount",
      "canonicalRelations",
      "applicableRelations",
      "notApplicableRelations",
      "applicableEdges",
      "logicalAssessments",
    ].map((field) => [field, decimalU64(proof[field], `${caseId}.proof.${field}`)]),
  );
  const applicable = result.relations.filter((relation) => relation.kind === "applicable");
  const notApplicable = result.relations.length - applicable.length;
  const edges = applicable.reduce((sum, relation) => sum + relation.adjacent.length, 0);
  if (
    counts.domainCount !== 256n ||
    counts.canonicalRelations !== BigInt(result.relations.length) ||
    counts.applicableRelations !== BigInt(applicable.length) ||
    counts.notApplicableRelations !== BigInt(notApplicable) ||
    counts.applicableEdges !== BigInt(edges) ||
    counts.logicalAssessments !== 256n * BigInt(edges)
  ) {
    fail(`${caseId}.proof decimal counts disagree with transported content`);
  }
  byteArray(result.failureMatrix, 32 * edges, `${caseId}.failureMatrix`);
  const matrixDigest = createHash("sha256").update(Buffer.from(result.failureMatrix)).digest();
  requireDigest(proof.matrixDigest, matrixDigest, `${caseId}.proof.matrixDigest`);

  let feasibleCount = 0;
  for (let candidate = 0; candidate < 256; candidate += 1) {
    let hasFailure = false;
    for (let edge = 0; edge < edges; edge += 1) {
      hasFailure ||= packedBit(result.failureMatrix, candidate * edges + edge);
    }
    const feasible = !hasFailure;
    if (packedBit(proof.partition, candidate) !== feasible) {
      fail(`${caseId}.partition is not the candidate-major LSB0 all-edge reduction`);
    }
    feasibleCount += Number(feasible);
  }
  if ((status === "feasible") !== (feasibleCount > 0)) {
    fail(`${caseId}.${status} contradicts its complete partition`);
  }
  requireDigest(
    proof.evaluationId,
    feasibilityEvaluationIdV1({
      proof,
      matrixDigest,
      atomicProofSha256,
      counts: {
        ...counts,
        packedResultBytes: BigInt(result.failureMatrix.length + proof.partition.length),
      },
    }),
    `${caseId}.proof.evaluationId`,
  );
  return feasibleCount;
}

function validateNotEvaluatedFeasibility(result, request, caseId) {
  exactKeys(
    result,
    ["domainDigest", "domainId", "relationSetDigest", "relations", "resourceProfileId"],
    `${caseId}.result`,
  );
  if (
    result.domainId !== "srgb8-neutral-axis-v1" ||
    result.resourceProfileId !== "compile-v1"
  ) {
    fail(`${caseId}.NotEvaluated typed identities drifted`);
  }
  byteArray(result.domainDigest, 32, `${caseId}.domainDigest`);
  byteArray(result.relationSetDigest, 32, `${caseId}.relationSetDigest`);
  if (
    !Array.isArray(result.relations) ||
    result.relations.some((relation) => relation?.kind !== "notApplicable")
  ) {
    fail(`${caseId}.NotEvaluated must retain only declared NotApplicable relations`);
  }
  result.relations.forEach((relation, index) =>
    validateFeasibilityRelation(relation, `${caseId}.result.relations[${index}]`));
  if (!isDeepStrictEqual(result.relations, canonicalRelations(request.relations))) {
    fail(`${caseId}.NotEvaluated relations differ from its canonical request`);
  }
  requireDigest(
    result.domainDigest,
    feasibilityDomainDigestV1(result.domainId, neutralAxisDomainV1()),
    `${caseId}.domainDigest`,
  );
  requireDigest(
    result.relationSetDigest,
    feasibilityRelationSetDigestV1(result.relations),
    `${caseId}.relationSetDigest`,
  );
}

function validateFeasibilityFailure(outcome, expectation, request, caseId) {
  exactKeys(outcome, ["error", "outcome", "schemaVersion"], `${caseId}.outcome`);
  exactKeys(outcome.error, ["error", "source"], `${caseId}.error`);
  if (outcome.error.source !== "core") fail(`${caseId} must preserve a Core failure`);
  const error = outcome.error.error;
  if (expectation.failure === "conflict") {
    exactKeys(error, ["code", "details"], `${caseId}.coreError`);
    exactKeys(error.details, ["code", "relationId"], `${caseId}.coreError.details`);
    const conflictingIds = new Set();
    for (const relation of request.relations) {
      const peers = request.relations.filter(
        (candidate) => candidate.relationId === relation.relationId,
      );
      if (peers.some((candidate) => !isDeepStrictEqual(candidate, relation))) {
        conflictingIds.add(relation.relationId);
      }
    }
    if (
      error.code !== "invalidRequest" ||
      error.details.code !== "conflictingRelationId" ||
      !conflictingIds.has(error.details.relationId)
    ) {
      fail(`${caseId} does not preserve the conflicting relation failure`);
    }
    return;
  }
  exactKeys(error, ["code", "details"], `${caseId}.coreError`);
  exactKeys(
    error.details,
    ["dimension", "limit", "profileId", "requested"],
    `${caseId}.coreError.details`,
  );
  const rawAdjacent = request.relations.reduce(
    (sum, relation) => sum + (relation.kind === "applicable" ? relation.adjacent.length : 0),
    0,
  );
  if (
    error.code !== "resourceLimitExceeded" ||
    error.details.profileId !== "compile-v1" ||
    error.details.dimension !== "rawAdjacentEntries" ||
    decimalU64(error.details.requested, `${caseId}.requested`) !== BigInt(rawAdjacent) ||
    decimalU64(error.details.limit, `${caseId}.limit`) !== 2_047n ||
    rawAdjacent !== 2_048
  ) {
    fail(`${caseId} does not preserve the exact raw-adjacent resource failure`);
  }
}

/** Independently validate the complete pack-5 feasibility family. */
export function validateWcag22FeasibilityFamily(family, atomicProofSha256Hex) {
  if (!/^[0-9a-f]{64}$/u.test(atomicProofSha256Hex ?? "")) {
    fail("WCAG22 feasibility validation requires the canonical atomic proof SHA-256");
  }
  const atomicProofSha256 = Buffer.from(atomicProofSha256Hex, "hex");
  if (!Array.isArray(family) || family.length !== FEASIBILITY_CASES_V1.size) {
    fail(`wcag22-feasibility family must contain exactly ${FEASIBILITY_CASES_V1.size} vectors`);
  }
  const byCase = new Map();
  for (const [index, vector] of family.entries()) {
    exactKeys(vector, FEASIBILITY_VECTOR_KEYS_V1, `wcag22-feasibility[${index}]`);
    if (typeof vector.caseId !== "string" || !FEASIBILITY_CASES_V1.has(vector.caseId)) {
      fail(`wcag22-feasibility[${index}] has unknown caseId ${vector.caseId}`);
    }
    if (byCase.has(vector.caseId)) fail(`duplicate wcag22-feasibility caseId ${vector.caseId}`);
    const expectation = FEASIBILITY_CASES_V1.get(vector.caseId);
    const request = validateFeasibilityRequest(vector.requestJson, vector.caseId);
    const outcome = canonicalJson(vector.outcomeJson, `${vector.caseId}.outcomeJson`);
    walkNoProportionalDto(outcome, `${vector.caseId}.outcome`);
    if (outcome.schemaVersion !== 1) fail(`${vector.caseId}.outcome schemaVersion must be 1`);

    let evaluated;
    let feasibleCount;
    if (expectation.failure) {
      if (outcome.outcome !== "failure") fail(`${vector.caseId} must be a failure outcome`);
      validateFeasibilityFailure(outcome, expectation, request, vector.caseId);
    } else {
      exactKeys(
        outcome,
        ["feasibility", "outcome", "schemaVersion"],
        `${vector.caseId}.outcome`,
      );
      if (outcome.outcome !== "success") fail(`${vector.caseId} must be a success outcome`);
      exactKeys(outcome.feasibility, ["result", "status"], `${vector.caseId}.feasibility`);
      if (outcome.feasibility.status !== expectation.terminal) {
        fail(`${vector.caseId} terminal ${outcome.feasibility.status} != ${expectation.terminal}`);
      }
      if (expectation.terminal === "notEvaluated") {
        validateNotEvaluatedFeasibility(outcome.feasibility.result, request, vector.caseId);
      } else {
        evaluated = outcome.feasibility.result;
        feasibleCount = validateEvaluatedFeasibility(
          evaluated,
          request,
          expectation.terminal,
          vector.caseId,
          atomicProofSha256,
        );
        if (expectation.count !== undefined && feasibleCount !== expectation.count) {
          fail(`${vector.caseId} feasible count ${feasibleCount} != ${expectation.count}`);
        }
      }
    }
    byCase.set(vector.caseId, { vector, request, outcome, evaluated, feasibleCount });
  }

  const opaqueARecord = byCase.get("opaque-identity-a");
  const opaqueBRecord = byCase.get("opaque-identity-b");
  const opaqueA = opaqueARecord?.evaluated;
  const opaqueB = opaqueBRecord?.evaluated;
  if (!opaqueA || !opaqueB || !opaqueARecord || !opaqueBRecord) {
    fail("opaque identity fixtures must both be evaluated");
  }
  const physicalRequest = (request) => ({
    ...request,
    relations: request.relations.map(({ relationId, occurrenceId, ...physical }) => physical),
  });
  if (
    !isDeepStrictEqual(
      physicalRequest(opaqueARecord.request),
      physicalRequest(opaqueBRecord.request),
    ) ||
    opaqueARecord.request.relations.some(
      (relation, index) =>
        relation.relationId === opaqueBRecord.request.relations[index]?.relationId ||
        relation.occurrenceId === opaqueBRecord.request.relations[index]?.occurrenceId,
    )
  ) {
    fail("opaque identity fixtures must differ only in client-owned identities");
  }
  for (const field of ["domain", "failureMatrix"]) {
    if (!isDeepStrictEqual(opaqueA[field], opaqueB[field])) {
      fail(`opaque identities changed physical ${field}`);
    }
  }
  for (const field of ["partition", "matrixDigest", "domainDigest", "proofSha256"]) {
    if (!isDeepStrictEqual(opaqueA.proof[field], opaqueB.proof[field])) {
      fail(`opaque identities changed physical proof ${field}`);
    }
  }
  for (const field of ["relationSetDigest", "evaluationId"]) {
    if (isDeepStrictEqual(opaqueA.proof[field], opaqueB.proof[field])) {
      fail(`opaque identities did not change declared ${field}`);
    }
  }
  return byCase;
}

async function validateConformance(conformance) {
  if (conformance.packVersion !== "5.0.0") {
    fail(`release requires conformance pack 5.0.0, got ${conformance.packVersion}`);
  }
  if (!/^[0-9a-f]{8}$/u.test(conformance.packDigest ?? "")) {
    fail(`invalid conformance packDigest: ${conformance.packDigest}`);
  }

  const familyBuffers = await Promise.all(
    CONFORMANCE_FAMILY_FILES.map((name) => readFile(resolve(CONFORMANCE_DIR, name))),
  );
  const proofPath = resolve(WCAG22_CONTRACT_DIR, "wcag22-srgb8-q55-proof-v1.json");
  const proofBytes = await readFile(proofPath);
  const proof = await readJson(proofPath);
  const actualDigest = fnv1a32(familyBuffers);
  if (actualDigest !== conformance.packDigest) {
    fail(
      `conformance packDigest ${conformance.packDigest} does not bind family bytes ${actualDigest}`,
    );
  }

  const families = familyBuffers.map((bytes, index) => {
    try {
      const value = JSON.parse(bytes.toString("utf8"));
      if (!Array.isArray(value) || value.length === 0) {
        fail(`${CONFORMANCE_FAMILY_FILES[index]} must contain a non-empty vector array`);
      }
      return value;
    } catch (error) {
      fail(`${CONFORMANCE_FAMILY_FILES[index]} is not valid JSON: ${error.message}`);
    }
  });
  const countKeys = [
    "contrasts",
    "ladders",
    "alpha",
    "solve",
    "muddiness",
    "wcag22",
    "wcag22Feasibility",
  ];
  let total = 0;
  for (const [index, key] of countKeys.entries()) {
    const actual = families[index].length;
    if (conformance.counts?.[key] !== actual) {
      fail(`conformance count ${key}=${conformance.counts?.[key]} differs from ${actual}`);
    }
    total += actual;
  }
  if (conformance.counts?.total !== total) {
    fail(`conformance total=${conformance.counts?.total} differs from ${total}`);
  }
  validateWcag22FeasibilityFamily(families[6], sha256(proofBytes));
  const halfTie = families[2].find(
    (entry) => entry.tint === "#C0B2FA" && entry.bg === "#000000" && entry.alpha === 0.122,
  );
  if (halfTie?.composite !== "#17161F") {
    fail("conformance pack lacks the exact source-over half-tie #C0B2FA@0.122 -> #17161F");
  }
  const antiEpsilon = families[5].find(
    (entry) =>
      entry.foreground === "#89BB09" &&
      entry.background === "#8212DB" &&
      entry.criterion === "sc-1.4.11-ui-component-or-state",
  );
  if (
    antiEpsilon?.decision !== "fail" ||
    antiEpsilon?.evidenceKind !== "canonical-finite-bounded" ||
    antiEpsilon?.artifactId !== proof.artifact_id ||
    antiEpsilon?.artifactSha256 !== proof.artifact_sha256 ||
    antiEpsilon?.boundId !== proof.bound_id ||
    antiEpsilon?.proofId !== proof.proof_id ||
    antiEpsilon?.proofSha256 !== sha256(proofBytes) ||
    antiEpsilon?.proofPayloadSha256 !== proof.proof_payload_sha256 ||
    antiEpsilon?.generatorSha256 !== proof.generator_sha256 ||
    antiEpsilon?.verifierSha256 !== proof.verifier_sha256 ||
    antiEpsilon?.profileChecksum !== proof.profile_checksum ||
    antiEpsilon?.profileSha256 !== proof.profile_source_sha256
  ) {
    fail("conformance pack lacks the exact proof-bound WCAG22 anti-epsilon witness");
  }
  validateCapabilityManifest(conformance.numericalCapabilities);

  const manifestBytes = await readFile(CONFORMANCE_MANIFEST);
  return {
    packVersion: conformance.packVersion,
    packDigest: conformance.packDigest,
    counts: conformance.counts,
    manifestSha256: sha256(manifestBytes),
    familySetSha256: sha256(Buffer.concat(familyBuffers)),
    families: CONFORMANCE_FAMILY_FILES.map((path, index) => ({
      path: `conformance/vectors/${path}`,
      bytes: familyBuffers[index].length,
      sha256: sha256(familyBuffers[index]),
    })),
  };
}

function lockedTypescriptVersion(packageLock) {
  const version = packageLock.packages?.["node_modules/typescript"]?.version;
  if (!/^\d+\.\d+\.\d+(?:[-+].+)?$/u.test(version ?? "")) {
    fail("package-lock.json does not pin an exact TypeScript compiler version");
  }
  return version;
}

function lockedNpmVersion(packageJson) {
  const declared = packageJson.packageManager?.match(/^npm@(\d+\.\d+\.\d+)$/u)?.[1];
  if (!declared) {
    fail("package.json packageManager must pin an exact npm version");
  }
  const actual = npm(["--version"]).trim();
  if (actual !== declared) {
    fail(`release packer is npm ${actual}, expected packageManager npm@${declared}`);
  }
  return declared;
}

async function wcag22FeasibilitySmokeFixture() {
  const family = await readJson(resolve(CONFORMANCE_DIR, "wcag22-feasibility.json"));
  const proofBytes = await readFile(
    resolve(WCAG22_CONTRACT_DIR, "wcag22-srgb8-q55-proof-v1.json"),
  );
  const canonical = validateWcag22FeasibilityFamily(family, sha256(proofBytes))
    .get("text-default-seven")?.vector;
  if (!canonical) fail("wcag22-feasibility smoke fixture is missing text-default-seven");
  return {
    requestJson: canonical.requestJson,
    outcomeJson: canonical.outcomeJson,
  };
}

function runtimeSmokeSource(feasibilityFixture) {
  return String.raw`
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

import init, {
  LabColors,
  evaluateWcag22,
  evaluateWcag22Feasibility,
  numericalCapabilityManifest,
  wcag22FeasibilityMaxBytes,
} from "@labpics/colors";

const require = createRequire(import.meta.url);
const wasmPath = require.resolve("@labpics/colors/pkg/labcolors_bg.wasm");
const metadataPath = require.resolve("@labpics/colors/build-metadata.json");
const packagePath = require.resolve("@labpics/colors/package.json");
const metadata = JSON.parse(await readFile(metadataPath, "utf8"));
const installedPackage = JSON.parse(await readFile(packagePath, "utf8"));
assert.deepEqual(metadata.package, {
  name: installedPackage.name,
  version: installedPackage.version,
});
assert.match(metadata.sourceSha, /^[0-9a-f]{40}(?:[0-9a-f]{24})?$/u);
assert.match(metadata.coreVersion, /^\d+\.\d+\.\d+$/u);
assert.equal(metadata.wasm.bytes, (await readFile(wasmPath)).length);
await init({ module_or_path: await readFile(wasmPath) });

const feasibilityFixture = ${JSON.stringify(feasibilityFixture)};
const feasibilityRequest = new TextEncoder().encode(feasibilityFixture.requestJson);
assert.ok(feasibilityRequest.byteLength <= wcag22FeasibilityMaxBytes());
assert.equal(
  JSON.stringify(evaluateWcag22Feasibility(feasibilityRequest)),
  feasibilityFixture.outcomeJson,
);

const capability = numericalCapabilityManifest();
assert.equal(capability.schemaVersion, 2);
assert.ok(capability.sites.some((site) =>
  site.siteId === "wcag22-srgb8-contrast-v1" &&
  site.proofIds.includes("wcag22-srgb8-full-domain-q55-v1")
));

const exactWcag22 = evaluateWcag22(
  "#898CB8",
  "#3E2217",
  "sc-1.4.3-text-default",
);
assert.equal(exactWcag22.decision, "fail");
assert.equal(exactWcag22.evidence.profileChecksum, "152813fe");
assert.match(exactWcag22.evidence.proofSha256, /^[0-9a-f]{64}$/u);

const config = {
  brand: {
    light: "#17161F",
    dark: "#17161F",
    light_ic: "#17161F",
    dark_ic: "#17161F",
  },
  neutral: {
    anchors: { light: "#FFFFFF", mid: "#7A7A82", dark: "#17171A" },
    tint: { ratio: 0.1, target_mp: 6.1, hue_stiffness: 9.0 },
  },
  palette: [{
    key: "family-4d",
    anchors: {
      light: "#7C3AED",
      dark: "#8B5CF6",
      light_ic: "#5B21B6",
      dark_ic: "#A78BFA",
    },
  }],
  sentiments: { categories: [], hardness: 5.0, chroma_fraction: 0.88 },
  themes: [{ name: "light", preset: "srgb" }],
  roles: [
    {
      name: "token-7f3a",
      recipe: { kind: "alpha-analog", of: { kind: "brand" }, alpha: 0.122 },
    },
    {
      name: "token-92be",
      recipe: {
        kind: "glow",
        source: { kind: "family", key: "family-4d" },
        step: "base",
        decision_profile: "legacy-platform-dependent-v1",
      },
    },
    {
      name: "token-a11c",
      recipe: {
        kind: "material",
        source: { kind: "brand" },
        tone_light: 0.72,
        tone_dark: 0.28,
        floor: "aa-ui",
      },
    },
  ],
};

const bytes = (hex) => [1, 3, 5].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16));
const hex = (channels) => "#" + channels.map((channel) => channel.toString(16).padStart(2, "0")).join("").toUpperCase();
const sourceOver = (tintHex, alpha, bgHex) => {
  const tint = bytes(tintHex);
  const bg = bytes(bgHex);
  return hex(bg.map((channel, index) => Math.round(channel + alpha * (tint[index] - channel))));
};
const screen = (glowHex, alpha, bgHex) => {
  const glow = bytes(glowHex);
  const bg = bytes(bgHex);
  return hex(bg.map((channel, index) =>
    Math.round(channel + alpha * glow[index] * (255 - channel) / 255)
  ));
};

const engine = new LabColors();
const fingerprint = engine.loadConfig(JSON.stringify(config));
assert.match(fingerprint, /^[0-9a-f]{16}$/u);

const background = "#000000";
const resolved = engine.resolveTheme(background, "light");
assert.deepEqual(Object.keys(resolved.roles).sort(), ["token-7f3a", "token-92be", "token-a11c"]);

const alpha = resolved.roles["token-7f3a"];
assert.equal(alpha.kind, "translucent");
assert.match(alpha.tintHex, /^#[0-9A-F]{6}$/u);
assert.equal(alpha.alpha, 0.122);
assert.equal(alpha.compositeHex, "#17161F");
assert.equal(alpha.alphaCoerced, false);
assert.equal(sourceOver(alpha.tintHex, alpha.alpha, background), alpha.compositeHex);
assert.match(alpha.css, /^oklch\(.+ \/ 0\.122\)$/u);

const material = resolved.roles["token-a11c"];
assert.equal(material.kind, "material");
assert.equal(
  material.alphaGuarantee.numericalProfile,
  "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1",
);
if (material.alphaGuarantee.kind === "transparent-endpoint-characterized-v1") {
  assert.equal(material.alpha, 0);
  assert.equal(material.alphaStatus, "satisfied");
  assert.equal(material.guaranteed, true);
} else if (material.alphaGuarantee.kind === "bisection-bracket-characterized-v1") {
  assert.equal(material.alpha, material.alphaGuarantee.upperAlpha);
  assert.equal(material.alphaStatus, "satisfied");
  assert.equal(material.guaranteed, true);
} else {
  assert.equal(material.alphaGuarantee.kind, "opaque-endpoint-characterized-v1");
  assert.equal(material.alpha, 1);
  assert.equal(material.alphaStatus, "degraded");
  assert.equal(material.guaranteed, false);
}
assert.ok(Number.isFinite(material.floor));

const glow = resolved.roles["token-92be"];
assert.equal(glow.kind, "glow");
assert.equal(glow.compositeProfile, "encoded-srgb8-screen-v1");
assert.equal(glow.compositeGuarantee, "bit-exact");
assert.equal(glow.layerRecipeProfile, "cam16-jprime-oklab-cusp-v1");
assert.equal(glow.appearanceDiagnosticProfile, "cam16-ucs-jprime-li2017-v1");
assert.equal(glow.selectionDiagnosticProfile, "cam16-ucs-jprime-li2017-v1");
assert.equal(glow.decisionProfile, "legacy-platform-dependent-v1");
assert.deepEqual(glow.decisionGuarantee, { kind: "legacy-platform-dependent-v1" });
assert.equal(glow.constraintLayer, "halo");
assert.ok(glow.targetStatus === "legacy-reached" || glow.targetStatus === "legacy-unreachable");
assert.ok(Number.isFinite(glow.alpha) && glow.alpha > 0 && glow.alpha <= 1);
assert.equal(Number(glow.alphaCss), glow.alpha);
assert.equal(glow.achievedDj, glow.haloAchievedDj);
assert.equal(glow.degraded, glow.targetStatus === "legacy-unreachable");
assert.equal(screen(glow.haloHex, glow.alpha, background), glow.haloCompositeHex);
assert.equal(screen(glow.coreHex, glow.alpha, background), glow.coreCompositeHex);
for (const value of [glow.targetDj, glow.haloAchievedDj, glow.coreAchievedDj]) {
  assert.ok(Number.isFinite(value));
}

assert.equal(engine.isStableGlowPointNoop("#010000", "#FE0000"), true);
assert.equal(engine.isStableGlowPointNoop("#800000", "#FE0000"), false);
assert.equal(engine.isStableGlowPointNoop("#001", "fff"), true);

config.roles[1].recipe.decision_profile = "stable-v1";
engine.loadConfig(JSON.stringify(config));
const stableDark = engine.resolveTheme("#101012", "light");
const stableIndeterminate = stableDark.roles["token-92be"];
assert.equal(stableIndeterminate.kind, "glow-indeterminate");
assert.equal(stableIndeterminate.decisionProfile, "stable-v1");
assert.equal(stableIndeterminate.reason, "sound-bound-unavailable");
assert.deepEqual(stableIndeterminate.bounds, { kind: "unavailable" });
for (const key of [
  stableIndeterminate.cssVar,
  stableIndeterminate.cssVar + "-core",
  stableIndeterminate.cssVar + "-alpha",
]) {
  assert.equal(stableDark.vars[key], undefined);
}

const stableWhite = engine.resolveTheme("#FFFFFF", "light");
const stableNoop = stableWhite.roles["token-92be"];
assert.equal(stableNoop.kind, "glow");
assert.equal(stableNoop.decisionProfile, "stable-v1");
assert.deepEqual(stableNoop.decisionGuarantee, { kind: "bit-exact" });
assert.equal(stableNoop.layerRecipeProfile, "cam16-jprime-oklab-cusp-v1");
assert.equal(stableNoop.appearanceDiagnosticProfile, "cam16-ucs-jprime-li2017-v1");
assert.equal(stableNoop.selectionDiagnosticProfile, null);
assert.equal(stableNoop.targetStatus, "exact-noop-unreachable");
assert.equal(stableNoop.degraded, true);
assert.equal(stableNoop.haloCompositeHex, "#FFFFFF");
for (const key of [
  stableNoop.cssVar,
  stableNoop.cssVar + "-core",
  stableNoop.cssVar + "-alpha",
]) {
  assert.equal(typeof stableWhite.vars[key], "string");
}
`;
}

function typeSmokeSource() {
  return String.raw`
import init, {
  LabColors,
  evaluateWcag22,
  evaluateWcag22Feasibility,
  numericalCapabilityManifest,
  wcag22FeasibilityMaxBytes,
  type GlowDecisionGuaranteeV1,
  type GlowDeterminateRole,
  type GlowDeterminateRoleBase,
  type GlowRole,
  type GlowTargetStatusV1,
  type LadderPositionV1,
  type MaterialRole,
  type MaterialRoleBase,
  type NumericalCapabilityManifestV2,
  type NumericalIndeterminacyV1,
  type ResolvedTheme,
  type ThemeConfig,
  type TranslucentRole,
  type Wcag22AssessmentV1,
  type Wcag22CriterionV1,
  type Wcag22FeasibilityOutcomeV1,
  type Wcag22FeasibilityRequestV1,
} from "@labpics/colors";
import { applyTheme } from "@labpics/colors/apply-theme";
import {
  watchTheme,
  type WatchController,
  type WatchThemeOptions,
} from "@labpics/colors/watch-theme";
import {
  adaptTheme,
  type AdaptController,
  type AdaptThemeOptions,
} from "@labpics/colors/adapt-theme";
import {
  effectiveBackground,
  type EffectiveBackgroundOptions,
  type Rgba,
} from "@labpics/colors/effective-bg";

const initialise: typeof init = init;
const apply: typeof applyTheme = applyTheme;
const watch: typeof watchTheme = watchTheme;
const adapt: typeof adaptTheme = adaptTheme;
const effective: typeof effectiveBackground = effectiveBackground;
type PublicSubpathTypes =
  | WatchController
  | WatchThemeOptions
  | AdaptController
  | AdaptThemeOptions
  | EffectiveBackgroundOptions
  | Rgba;
declare const publicSubpathType: PublicSubpathTypes;
void [apply, watch, adapt, effective, publicSubpathType];
const engine = new LabColors();
const fingerprint: string = engine.loadConfig("{}");
const resolved: ResolvedTheme = engine.resolveTheme("#000000", "light");
const capability: NumericalCapabilityManifestV2 = numericalCapabilityManifest();
const wcagCriterion: Wcag22CriterionV1 = "sc-1.4.3-text-default";
const wcagAssessment: Wcag22AssessmentV1 = evaluateWcag22(
  "#000000",
  "#FFFFFF",
  wcagCriterion,
);
// @ts-expect-error criterion is an explicit closed menu, not an opaque string.
evaluateWcag22("#000000", "#FFFFFF", "danger");

const feasibilityRequest: Wcag22FeasibilityRequestV1 = {
  schemaVersion: 1,
  domainId: "srgb8-neutral-axis-v1",
  resourceProfileId: "compile-v1",
  relations: [{
    relationId: "opaque-client-relation",
    occurrenceId: "opaque-client-occurrence",
    kind: "applicable",
    criterion: "sc-1.4.3-text-default",
    adjacent: [[118, 118, 118]],
  }],
};
const feasibilityBytes = new TextEncoder().encode(JSON.stringify(feasibilityRequest));
const feasibilityCeiling: number = wcag22FeasibilityMaxBytes();
const feasibilityOutcome: Wcag22FeasibilityOutcomeV1 =
  evaluateWcag22Feasibility(feasibilityBytes);
// @ts-expect-error byte API rejects strings.
evaluateWcag22Feasibility(JSON.stringify(feasibilityRequest));

function assertNever(value: never): never {
  throw new Error("unreachable: " + String(value));
}

type FeasibilityFailure = Extract<
  Wcag22FeasibilityOutcomeV1,
  { readonly outcome: "failure" }
>["error"];
type FeasibilityTransportFailure = Extract<
  FeasibilityFailure,
  { readonly source: "transport" }
>["error"];
type FeasibilityCoreFailure = Extract<
  FeasibilityFailure,
  { readonly source: "core" }
>["error"];

function describeTransportFailure(error: FeasibilityTransportFailure): string {
  switch (error.code) {
    case "envelopeTooLarge":
    case "invalidUtf8":
    case "malformedEnvelope":
    case "unsupportedSchemaVersion":
    case "unsupportedDomainId":
    case "unsupportedResourceProfileId":
    case "unsupportedCriterion":
    case "emptyNotApplicableReason":
      return error.code;
    default:
      return assertNever(error);
  }
}

function describeCoreFailure(error: FeasibilityCoreFailure): string {
  switch (error.code) {
    case "invalidRequest":
    case "resourceLimitExceeded":
    case "allocationFailed":
    case "evaluatorInvariantViolation":
    case "compilerInvariantViolation":
      return error.code;
    default:
      return assertNever(error);
  }
}

function describeFeasibilityOutcome(outcome: Wcag22FeasibilityOutcomeV1): string {
  switch (outcome.outcome) {
    case "success": {
      const feasibility = outcome.feasibility;
      switch (feasibility.status) {
        case "feasible":
        case "infeasible":
        case "notEvaluated":
          return feasibility.status;
        default:
          return assertNever(feasibility);
      }
    }
    case "failure": {
      const failure = outcome.error;
      switch (failure.source) {
        case "transport":
          return describeTransportFailure(failure.error);
        case "core":
          return describeCoreFailure(failure.error);
        case "incompatibleCoreContract":
          return failure.source;
        default:
          return assertNever(failure);
      }
    }
    default:
      return assertNever(outcome);
  }
}

const borderPosition: LadderPositionV1 = "border-strong";
const config: ThemeConfig = {
  brand: {
    light: "#17161F",
    dark: "#17161F",
    light_ic: "#17161F",
    dark_ic: "#17161F",
  },
  neutral: {
    anchors: { light: "#FFFFFF", mid: "#7A7A82", dark: "#17171A" },
    tint: { ratio: 0.1, target_mp: 6.1, hue_stiffness: 9.0 },
  },
  palette: [],
  sentiments: { categories: [], hardness: 5.0, chroma_fraction: 0.88 },
  themes: [{ name: "light", preset: "srgb" }],
  roles: [
    {
      name: "label-a1",
      recipe: {
        kind: "text-anchor",
        fraction: 0.62,
        floor: "aa-text",
        hue: { kind: "brand" },
      },
    },
    {
      name: "border-b2",
      recipe: {
        kind: "ladder",
        source: { kind: "brand" },
        position: borderPosition,
        floor: "aa-ui",
      },
    },
    {
      name: "glow-stable",
      recipe: {
        kind: "glow",
        source: { kind: "brand" },
        step: "base",
        decision_profile: "stable-v1",
      },
    },
    {
      name: "glow-legacy",
      recipe: {
        kind: "glow",
        source: { kind: "brand" },
        step: "base",
        decision_profile: "legacy-platform-dependent-v1",
      },
    },
  ],
};

function alphaContract(role: TranslucentRole): readonly [string, number, string, boolean] {
  return [role.tintHex, role.alpha, role.compositeHex, role.alphaCoerced] as const;
}

function glowContract(
  role: GlowRole,
): readonly ["indeterminate", "glow-target-or-maximum-v1"] | readonly [
  "encoded-srgb8-screen-v1",
  "halo",
  GlowTargetStatusV1,
  string,
  number,
] {
  if (role.kind === "glow-indeterminate") {
    return ["indeterminate", role.numericalSiteId] as const;
  }
  return [
    role.compositeProfile,
    role.constraintLayer,
    role.targetStatus,
    role.haloCompositeHex,
    role.haloAchievedDj,
  ] as const;
}

function decisionEvidence(guarantee: GlowDecisionGuaranteeV1): string {
  return guarantee.kind;
}

function indeterminacyEvidence(evidence: NumericalIndeterminacyV1): number | string {
  return evidence.reason === "interval-overlap"
    ? evidence.bounds.upper - evidence.bounds.lower
    : evidence.bounds.kind;
}

function determinateGlowEvidence(role: GlowDeterminateRole): string {
  if (role.decisionProfile === "stable-v1") {
    const status: "exact-noop-unreachable" = role.targetStatus;
    const selection: null = role.selectionDiagnosticProfile;
    const degraded: true = role.degraded;
    void selection;
    void degraded;
    return status;
  }
  if (role.targetStatus === "legacy-reached") {
    const degraded: false = role.degraded;
    void degraded;
    return role.targetStatus;
  }
  const status: "legacy-unreachable" = role.targetStatus;
  const degraded: true = role.degraded;
  void degraded;
  return status;
}

function materialEvidence(role: MaterialRole): number | boolean {
  if (role.alphaStatus === "degraded") {
    const alpha: 1 = role.alpha;
    const guaranteed: false = role.guaranteed;
    void alpha;
    return guaranteed;
  }
  const guaranteed: true = role.guaranteed;
  if (role.alphaGuarantee.kind === "bisection-bracket-characterized-v1") {
    return role.alphaGuarantee.upperAlpha;
  }
  return guaranteed;
}

declare const glowBase: GlowDeterminateRoleBase;
// @ts-expect-error stable profile не может нести legacy status.
const impossibleGlow: GlowDeterminateRole = {
  ...glowBase,
  decisionProfile: "stable-v1",
  decisionGuarantee: { kind: "bit-exact" },
  selectionDiagnosticProfile: null,
  targetStatus: "legacy-reached",
  degraded: false,
};

declare const materialBase: MaterialRoleBase;
// @ts-expect-error compatibility boolean выводится из material status/guarantee.
const impossibleMaterial: MaterialRole = {
  ...materialBase,
  alpha: 1,
  alphaGuarantee: {
    kind: "opaque-endpoint-characterized-v1",
    numericalProfile: "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1",
  },
  alphaStatus: "degraded",
  guaranteed: true,
};

void [
  initialise,
  fingerprint,
  resolved,
  wcagAssessment,
  feasibilityRequest,
  feasibilityBytes,
  feasibilityCeiling,
  feasibilityOutcome,
  describeFeasibilityOutcome,
  capability,
  config,
  alphaContract,
  glowContract,
  decisionEvidence,
  indeterminacyEvidence,
  determinateGlowEvidence,
  materialEvidence,
  impossibleGlow,
  impossibleMaterial,
];
`;
}

async function verifyCleanConsumer(
  tarballPath,
  packageJson,
  packageLock,
  expectedBuildMetadata,
  expectedWcag22Artifacts,
) {
  const consumer = await mkdtemp(join(tmpdir(), "labcolors-release-consumer-"));
  try {
    await writeFile(
      join(consumer, "package.json"),
      `${JSON.stringify({ private: true, type: "module" }, null, 2)}\n`,
    );

    const typescriptVersion = lockedTypescriptVersion(packageLock);
    const localTypescript = await readJson(
      resolve(PACKAGE_DIR, "node_modules/typescript/package.json"),
    );
    if (localTypescript.version !== typescriptVersion) {
      fail(
        `installed TypeScript ${localTypescript.version} differs from lockfile ${typescriptVersion}`,
      );
    }
    const typescriptCompiler = resolve(PACKAGE_DIR, "node_modules/typescript/bin/tsc");

    npm(
      [
        "install",
        "--offline",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--no-package-lock",
        "--save=false",
        tarballPath,
      ],
      consumer,
    );

    const installed = resolve(consumer, "node_modules", ...packageJson.name.split("/"));
    const installedPackage = await readJson(resolve(installed, "package.json"));
    if (installedPackage.name !== packageJson.name || installedPackage.version !== packageJson.version) {
      fail(
        `clean install resolved ${installedPackage.name}@${installedPackage.version}, ` +
          `expected ${packageJson.name}@${packageJson.version}`,
      );
    }

    await validateWcag22EvidenceArtifacts(
      installed,
      expectedWcag22Artifacts,
      "clean-installed package",
    );

    const installedWasm = await readFile(resolve(installed, "pkg/labcolors_bg.wasm"));
    if (sha256(installedWasm) !== expectedBuildMetadata.wasm.sha256) {
      fail("clean-installed WASM bytes differ from the packed release input");
    }
    const installedBuildMetadata = await readJson(resolve(installed, "build-metadata.json"));
    if (!isDeepStrictEqual(installedBuildMetadata, expectedBuildMetadata)) {
      fail("clean-installed build metadata differs from the verified release inputs");
    }

    const feasibilityFixture = await wcag22FeasibilitySmokeFixture();
    const runtimePath = resolve(consumer, "smoke.mjs");
    const typesPath = resolve(consumer, "smoke.ts");
    await writeFile(runtimePath, runtimeSmokeSource(feasibilityFixture));
    await writeFile(typesPath, typeSmokeSource());

    command(process.execPath, [runtimePath], consumer);
    command(
      process.execPath,
      [
        typescriptCompiler,
        "--noEmit",
        "--strict",
        "--skipLibCheck",
        "false",
        "--target",
        "ES2022",
        "--lib",
        "ES2022,DOM",
        "--module",
        "NodeNext",
        "--moduleResolution",
        "NodeNext",
        typesPath,
      ],
      consumer,
    );
  } finally {
    await rm(consumer, { recursive: true, force: true });
  }
}

// Execute the same packed-package runtime smoke under the caller's Node binary.
// CI uses this to prove the public consumer floor independently from the pinned
// release packer.
export async function smokePackedRuntime(tarballPath) {
  const tarball = resolve(tarballPath);
  const consumer = await mkdtemp(join(tmpdir(), "labcolors-runtime-smoke-"));
  try {
    await writeFile(
      join(consumer, "package.json"),
      `${JSON.stringify({ private: true, type: "module" }, null, 2)}\n`,
    );
    npm(
      [
        "install",
        "--offline",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--no-package-lock",
        "--save=false",
        tarball,
      ],
      consumer,
    );
    const feasibilityFixture = await wcag22FeasibilitySmokeFixture();
    const runtimePath = resolve(consumer, "smoke.mjs");
    await writeFile(runtimePath, runtimeSmokeSource(feasibilityFixture));
    command(process.execPath, [runtimePath], consumer);
  } finally {
    await rm(consumer, { recursive: true, force: true });
  }
}

export async function verifyPackageRelease() {
  const { sourceSha: source } = await prepareNpmPackage();
  command("python3", ["scripts/verify_wcag22_q55.py"], REPO_ROOT);
  const wcag22Evidence = await validateWcag22Evidence();

  const [packageJson, packageLock, cargoSource, conformance] = await Promise.all([
    readJson(PACKAGE_JSON),
    readJson(PACKAGE_LOCK),
    readFile(ROOT_CARGO, "utf8"),
    readJson(CONFORMANCE_MANIFEST),
  ]);

  const coreVersion = workspaceVersion(cargoSource);
  const npmVersion = lockedNpmVersion(packageJson);
  const lockRoot = packageLock.packages?.[""];
  if (packageJson.name !== "@labpics/colors") fail(`unexpected npm package name: ${packageJson.name}`);
  const consumerNodeFloor = packageJson.engines?.node?.match(/^>=(\d+\.\d+\.\d+)$/u)?.[1];
  if (!consumerNodeFloor) {
    fail("package.json engines.node must declare one exact >=X.Y.Z consumer floor");
  }
  if (lockRoot?.name !== packageJson.name || lockRoot?.version !== packageJson.version) {
    fail("package.json and package-lock.json root identity differ");
  }
  if (conformance.coreVersion !== coreVersion) {
    fail(
      `conformance coreVersion ${conformance.coreVersion} differs from Cargo version ${coreVersion}`,
    );
  }
  if (!/^\d+\.\d+\.\d+$/u.test(conformance.packVersion ?? "")) {
    fail(`invalid conformance packVersion: ${conformance.packVersion}`);
  }
  const conformanceEvidence = await validateConformance(conformance);

  const wasmBytes = await readFile(WASM_PATH);
  if (wasmBytes.length < 8 || !wasmBytes.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109]))) {
    fail("pkg/labcolors_bg.wasm is absent or has no WebAssembly magic header");
  }
  const wasm = await hashedArtifact(WASM_PATH, "pkg/labcolors_bg.wasm");
  const buildMetadataValue = await readJson(BUILD_METADATA);
  validateBuildMetadata(buildMetadataValue, {
    packageJson,
    source,
    coreVersion,
    conformanceEvidence,
    wasm,
  });
  const buildMetadata = await hashedArtifact(BUILD_METADATA, "build-metadata.json");

  await rm(RELEASE_DIR, { recursive: true, force: true });
  await mkdir(RELEASE_DIR, { recursive: true });

  const canonicalPack = await packInto(RELEASE_DIR, packageJson);
  const reproductionDir = await mkdtemp(join(tmpdir(), "labcolors-release-reproduction-"));
  try {
    const reproducedPack = await packInto(reproductionDir, packageJson);
    if (canonicalPack.tarballName !== reproducedPack.tarballName) {
      fail(
        `independent npm pack passes produced different filenames: ` +
          `${canonicalPack.tarballName} != ${reproducedPack.tarballName}`,
      );
    }
    const [canonicalBytes, reproducedBytes] = await Promise.all([
      readFile(canonicalPack.path),
      readFile(reproducedPack.path),
    ]);
    if (!canonicalBytes.equals(reproducedBytes)) {
      fail("independent npm pack passes produced different tarball bytes");
    }
  } finally {
    await rm(reproductionDir, { recursive: true, force: true });
  }

  await validatePackedWcag22Evidence(canonicalPack.path, wcag22Evidence.artifacts);

  const tarball = await hashedArtifact(
    canonicalPack.path,
    `.release/${canonicalPack.tarballName}`,
  );
  await verifyCleanConsumer(
    canonicalPack.path,
    packageJson,
    packageLock,
    buildMetadataValue,
    wcag22Evidence.artifacts,
  );

  const manifest = {
    // Схема release-manifest v2: numericalSites (pack 2.x, прозаические
    // research-поля) заменён на numericalCapabilities — typed capability
    // projection ядра с независимо пересчитанным checksum. Read-back в
    // publish-workflow пиняет ровно эту версию.
    schemaVersion: 2,
    npm: packageJson.version,
    core: coreVersion,
    wire: {
      identity: `resolved-theme@${packageJson.version}`,
      embeddedInPayload: false,
      trackingIssue: 258,
    },
    conformance: conformanceEvidence,
    normativeEvidence: { wcag22: wcag22Evidence },
    sourceSha: source,
    reproducibility: {
      method: "two-independent-npm-pack-passes",
      passes: 2,
      byteIdentical: true,
    },
    requirements: {
      consumerRuntime: {
        node: packageJson.engines.node,
        verifiedFloor: consumerNodeFloor,
        canonicalGate: "Node 22 consumer floor",
      },
      buildToolchain: {
        node: process.versions.node,
        npm: npmVersion,
      },
      typescript: {
        compiler: lockedTypescriptVersion(packageLock),
        target: "ES2022",
        libraries: ["ES2022", "DOM"],
        skipLibCheck: false,
      },
    },
    supported: [
      "exact-alpha-srgb8-v1",
      "exact-screen-composite-srgb8-v1",
      "typed-glow-indeterminate-v1",
      "wcag22-srgb8-contrast-v1",
      "wcag22-feasibility-v1",
    ],
    numericalCapabilities: conformance.numericalCapabilities,
    unsupported: [
      "embedded-wire-schema-version",
      "stable-cam16-glow-target-or-maximum-selection",
      "renderer-or-output-pipeline-equivalence",
      "spatial-glow-field",
      "display-p3",
    ],
    artifacts: { tarball, wasm, buildMetadata },
  };
  await writeFile(RELEASE_MANIFEST, `${JSON.stringify(manifest, null, 2)}\n`);

  return { manifest: RELEASE_MANIFEST, tarball: canonicalPack.path };
}

async function writeGithubOutputs({ manifest, tarball }) {
  const output = process.env.GITHUB_OUTPUT?.trim();
  if (!output) return;
  for (const [key, value] of [["manifest", manifest], ["tarball", tarball]]) {
    if (value.includes("\n") || value.includes("\r")) {
      fail(`unsafe newline in ${key} output path`);
    }
    await appendFile(output, `${key}=${value}\n`, "utf8");
  }
}

const invokedDirectly =
  process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  const runtimeSmokeIndex = process.argv.indexOf("--runtime-smoke");
  const action = runtimeSmokeIndex >= 0
    ? (() => {
        const tarball = process.argv[runtimeSmokeIndex + 1];
        if (!tarball) fail("--runtime-smoke requires a tarball path");
        return smokePackedRuntime(tarball).then(() => ({ runtimeSmoke: tarball }));
      })()
    : verifyPackageRelease();
  action
    .then(async ({ manifest, tarball }) => {
      if (runtimeSmokeIndex >= 0) {
        console.log(`runtime smoke passed: ${resolve(process.argv[runtimeSmokeIndex + 1])}`);
        return;
      }
      await writeGithubOutputs({ manifest, tarball });
      console.log(`verified ${relative(REPO_ROOT, tarball).split(sep).join("/")}`);
      console.log(`wrote ${relative(REPO_ROOT, manifest).split(sep).join("/")}`);
    })
    .catch((error) => {
      console.error(error instanceof Error ? error.stack : String(error));
      process.exitCode = 1;
    });
}
