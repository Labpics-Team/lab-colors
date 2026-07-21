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
import {
  NUMERICAL_EVIDENCE_FILES,
  PACKED_NUMERICAL_EVIDENCE_PATHS,
  POINT_SUPPORT_EVIDENCE_FILES,
  WCAG22_EVIDENCE_FILES,
  assertPackageEvidenceInventory,
} from "./release-evidence.mjs";

const RELEASE_DIR = resolve(PACKAGE_DIR, ".release");
const RELEASE_MANIFEST = resolve(RELEASE_DIR, "release-manifest.json");
const PACKAGE_JSON = resolve(PACKAGE_DIR, "package.json");
const PACKAGE_LOCK = resolve(PACKAGE_DIR, "package-lock.json");
const BUILD_METADATA = resolve(PACKAGE_DIR, "build-metadata.json");
const ROOT_CARGO = resolve(REPO_ROOT, "Cargo.toml");
const CONFORMANCE_DIR = resolve(REPO_ROOT, "conformance/vectors");
const CONFORMANCE_MANIFEST = resolve(CONFORMANCE_DIR, "manifest.json");
const NUMERICAL_CONTRACT_DIR = resolve(REPO_ROOT, "crates/labcolors-core/contracts");
// Полный состав пака 10.0.0. Верификатор читает байты из репозитория (не из
// тарболла) и пересчитывает packDigest над всеми пятью семействами.
const CONFORMANCE_FAMILY_FILES = [
  "contrasts.json",
  "ladders.json",
  "alpha.json",
  "solve.json",
  "wcag22.json",
];
const RUNTIME_WASM_PATH = resolve(PACKAGE_DIR, "pkg/labcolors_bg.wasm");

const POINT_SUPPORT_CERTIFIED_CLAIM =
  "for every successfully evaluated enabled stability cell, decision is Retained iff current_lower_surplus >= (10000-drop_bps)/10000 * max(baseline_lower_surplus,0); the declared anchor remains a separate hard floor";
const POINT_SUPPORT_EXCLUDED_CLAIM =
  "does not certify retention against the unknown exact baseline surplus, renderer equivalence outside encoded-sRGB8 source-over, or a successful result when evaluation fails";
const POINT_SUPPORT_SOURCE_BINDING_SCOPE =
  "exact bytes of the private point-support Rust semantic cone and its two WCAG include_str inputs; comments and cfg(test) text are intentionally significant";
const POINT_SUPPORT_SOURCE_BINDING_EXCLUSIONS = [
  "whole-crate compilation or compiler/toolchain attestation",
  "binary, package, FFI, renderer, or browser transport attestation",
  "unrelated Lab Colors modules outside the declared point-support semantic cone",
];
const POINT_SUPPORT_SOURCE_PATHS = [
  "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json",
  "crates/labcolors-core/contracts/wcag22-srgb8-v1.json",
  "crates/labcolors-core/src/appearance.rs",
  "crates/labcolors-core/src/composition.rs",
  "crates/labcolors-core/src/constraints/exact.rs",
  "crates/labcolors-core/src/constraints/mod.rs",
  "crates/labcolors-core/src/constraints/wcag22.rs",
  "crates/labcolors-core/src/hash.rs",
  "crates/labcolors-core/src/lib.rs",
  "crates/labcolors-core/src/numerics.rs",
  "crates/labcolors-core/src/observation.rs",
  "crates/labcolors-core/src/point_support.rs",
  "crates/labcolors-core/src/session.rs",
  "crates/labcolors-core/src/srgb8.rs",
  "crates/labcolors-core/src/wcag22/kernel.rs",
  "crates/labcolors-core/src/wcag22/q55_data.rs",
  "crates/labcolors-core/src/wcag22.rs",
  "crates/labcolors-core/src/wcag22_evidence.rs",
].sort();

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

// The proof deliberately carries exact Q55/u128 integer lexemes beyond
// Number.MAX_SAFE_INTEGER. Remove its self-digest from the canonical raw bytes:
// a JSON.parse/JSON.stringify round-trip would silently change those integers.
function exactJsonPayloadWithoutTopLevelField(bytes, field, label) {
  if (
    !Buffer.isBuffer(bytes) ||
    bytes.length < 3 ||
    bytes[bytes.length - 1] !== 0x0a ||
    bytes[bytes.length - 2] !== 0x7d
  ) {
    fail(`${label} must be one canonical JSON object followed by one LF`);
  }

  const body = bytes.subarray(0, -1);
  if (body[0] !== 0x7b) {
    fail(`${label} must have a top-level JSON object`);
  }

  const members = [];
  const containers = [0x7b];
  let memberStart = 1;
  let inString = false;
  let escaped = false;

  function addMember(end) {
    if (end === memberStart) {
      fail(`${label} has an empty or trailing top-level member`);
    }
    let keyEnd = memberStart + 1;
    if (body[memberStart] !== 0x22) {
      fail(`${label} has a non-string top-level key`);
    }
    let keyEscaped = false;
    for (; keyEnd < end; keyEnd += 1) {
      const byte = body[keyEnd];
      if (keyEscaped) {
        keyEscaped = false;
      } else if (byte === 0x5c) {
        keyEscaped = true;
      } else if (byte === 0x22) {
        break;
      }
    }
    if (keyEnd >= end || body[keyEnd + 1] !== 0x3a) {
      fail(`${label} has a malformed top-level member`);
    }
    const rawKey = body.subarray(memberStart, keyEnd + 1);
    const key = JSON.parse(rawKey.toString("utf8"));
    if (!rawKey.equals(Buffer.from(JSON.stringify(key), "utf8"))) {
      fail(`${label} has a non-canonical top-level key`);
    }
    members.push({ start: memberStart, end, key, rawKey });
  }

  for (let index = 1; index < body.length; index += 1) {
    const byte = body[index];
    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (byte === 0x5c) {
        escaped = true;
      } else if (byte === 0x22) {
        inString = false;
      }
      continue;
    }
    if (byte === 0x22) {
      inString = true;
      continue;
    }
    if (byte === 0x20 || byte === 0x09 || byte === 0x0a || byte === 0x0d) {
      fail(`${label} contains non-canonical insignificant whitespace`);
    }
    if (byte === 0x7b || byte === 0x5b) {
      containers.push(byte);
      continue;
    }
    if (byte === 0x7d || byte === 0x5d) {
      const expectedOpen = byte === 0x7d ? 0x7b : 0x5b;
      if (containers.at(-1) !== expectedOpen) {
        fail(`${label} has mismatched JSON containers`);
      }
      if (containers.length === 1) {
        if (byte !== 0x7d || index !== body.length - 1) {
          fail(`${label} has bytes after its top-level object`);
        }
        if (index !== memberStart || members.length > 0) addMember(index);
      }
      containers.pop();
      continue;
    }
    if (byte === 0x2c && containers.length === 1) {
      addMember(index);
      memberStart = index + 1;
    }
  }

  if (inString || containers.length !== 0) {
    fail(`${label} is not a complete JSON object`);
  }
  for (let index = 1; index < members.length; index += 1) {
    if (Buffer.compare(members[index - 1].rawKey, members[index].rawKey) >= 0) {
      fail(`${label} top-level keys are duplicate or unsorted`);
    }
  }

  const targetIndex = members.findIndex(({ key }) => key === field);
  if (targetIndex < 0 || members.some(({ key }, index) => key === field && index !== targetIndex)) {
    fail(`${label} must contain exactly one top-level ${field}`);
  }
  const target = members[targetIndex];
  if (members.length === 1) return Buffer.from("{}", "utf8");
  if (targetIndex === 0) {
    return Buffer.concat([body.subarray(0, target.start), body.subarray(target.end + 1)]);
  }
  return Buffer.concat([body.subarray(0, target.start - 1), body.subarray(target.end)]);
}

async function hashedArtifact(path, displayPath) {
  const bytes = await readFile(path);
  if (bytes.length === 0) fail(`${displayPath} is empty`);
  return { path: displayPath, bytes: bytes.length, sha256: sha256(bytes) };
}

export async function validateNumericalEvidenceArtifacts(
  root,
  expectedArtifacts,
  label,
) {
  const allowedPaths = PACKED_NUMERICAL_EVIDENCE_PATHS;
  if (!Array.isArray(expectedArtifacts) || expectedArtifacts.length !== allowedPaths.length) {
    fail(`${label} numerical evidence expectation must contain ${allowedPaths.length} artifacts`);
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
      fail(`${label} has malformed or duplicate numerical evidence metadata`);
    }
    expectedByPath.set(artifact.path, artifact);
  }

  const actualArtifacts = [];
  for (const file of NUMERICAL_EVIDENCE_FILES) {
    const displayPath = `evidence/${file}`;
    const expected = expectedByPath.get(displayPath);
    if (!expected) fail(`${label} lacks expected numerical evidence metadata: ${displayPath}`);
    const [canonical, actual] = await Promise.all([
      readFile(resolve(NUMERICAL_CONTRACT_DIR, file)),
      readFile(resolve(root, "evidence", file)),
    ]);
    if (!actual.equals(canonical)) {
      fail(`${label} numerical evidence bytes differ from canonical source: ${displayPath}`);
    }
    const metadata = {
      path: displayPath,
      bytes: actual.length,
      sha256: sha256(actual),
    };
    if (metadata.bytes !== expected.bytes || metadata.sha256 !== expected.sha256) {
      fail(
        `${label} numerical evidence metadata differs for ${displayPath}: ` +
          `expected ${expected.bytes}B/${expected.sha256}, ` +
          `actual ${metadata.bytes}B/${metadata.sha256}`,
      );
    }
    actualArtifacts.push(metadata);
  }
  return actualArtifacts;
}

async function validateNumericalEvidence() {
  const artifacts = [];
  for (const file of NUMERICAL_EVIDENCE_FILES) {
    artifacts.push(
      await hashedArtifact(
        resolve(NUMERICAL_CONTRACT_DIR, file),
        `evidence/${file}`,
      ),
    );
  }
  await validateNumericalEvidenceArtifacts(PACKAGE_DIR, artifacts, "staged package");
  return artifacts;
}

async function validateWcag22Evidence(artifacts) {
  const profilePath = resolve(NUMERICAL_CONTRACT_DIR, WCAG22_EVIDENCE_FILES[0]);
  const profileBytes = await readFile(profilePath);
  const profile = await readJson(profilePath);
  const binary = await readFile(resolve(NUMERICAL_CONTRACT_DIR, WCAG22_EVIDENCE_FILES[1]));
  const proof = await readJson(resolve(NUMERICAL_CONTRACT_DIR, WCAG22_EVIDENCE_FILES[2]));
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
    artifacts: artifacts.filter(({ path }) =>
      WCAG22_EVIDENCE_FILES.some((file) => path === `evidence/${file}`)
    ),
  };
}

async function validatePointSupportEvidence(artifacts, numericalCapabilities) {
  const proofFile = POINT_SUPPORT_EVIDENCE_FILES[0];
  const proofPath = resolve(NUMERICAL_CONTRACT_DIR, proofFile);
  const [proofBytes, proof, wcagProofBytes, wcagProof] = await Promise.all([
    readFile(proofPath),
    readJson(proofPath),
    readFile(resolve(NUMERICAL_CONTRACT_DIR, WCAG22_EVIDENCE_FILES[2])),
    readJson(resolve(NUMERICAL_CONTRACT_DIR, WCAG22_EVIDENCE_FILES[2])),
  ]);
  exactKeys(
    proof,
    [
      "schema_version",
      "profile_id",
      "site_id",
      "artifact_id",
      "bound_id",
      "proof_id",
      "declared_operation_law",
      "certified_claim",
      "excluded_claim",
      "q55_dependency",
      "source_binding_schema_version",
      "source_binding_law",
      "source_binding_scope",
      "source_binding_exclusions",
      "source_closure_sha256",
      "source_negative_controls",
      "source_files",
      "universal_algebraic_certificate",
      "reference_and_anchor_proof",
      "basis_point_proof",
      "comparator_proof",
      "integer_replay_envelope",
      "verifier_sha256",
      "proof_payload_sha256",
    ],
    "point-support proof",
  );
  const proofPayloadBytes = exactJsonPayloadWithoutTopLevelField(
    proofBytes,
    "proof_payload_sha256",
    "point-support proof",
  );
  if (sha256(proofPayloadBytes) !== proof.proof_payload_sha256) {
    fail("point-support proof payload digest is invalid");
  }
  if (
    proof.schema_version !== 2 ||
    proof.site_id !== "point-support-retained-reference-surplus-v1" ||
    proof.profile_id !== "srgb8-q55-retained-reference-surplus-bps-v1" ||
    proof.artifact_id !== "wcag22-srgb8-luminance-q55-v1" ||
    proof.bound_id !== "point-support-reference-surplus-q55-bps-v1" ||
    proof.proof_id !== "point-support-reference-surplus-integer-v1" ||
    proof.declared_operation_law !==
      "q55-lower-reference-distance-explicit-anchor-bps-retention-v1"
  ) {
    fail("point-support proof typed identity or operation law drifted");
  }
  if (
    proof.certified_claim !== POINT_SUPPORT_CERTIFIED_CLAIM ||
    proof.excluded_claim !== POINT_SUPPORT_EXCLUDED_CLAIM
  ) {
    fail("point-support proof claim boundary drifted");
  }
  if (
    proof.source_binding_schema_version !== 2 ||
    proof.source_binding_law !== "point-support-rust-whole-file-semantic-cone-v2" ||
    proof.source_binding_scope !== POINT_SUPPORT_SOURCE_BINDING_SCOPE ||
    !isDeepStrictEqual(
      proof.source_binding_exclusions,
      POINT_SUPPORT_SOURCE_BINDING_EXCLUSIONS,
    ) ||
    !/^[0-9a-f]{64}$/u.test(proof.source_closure_sha256 ?? "") ||
    proof.source_negative_controls !== 33 ||
    !/^[0-9a-f]{64}$/u.test(proof.proof_payload_sha256 ?? "") ||
    !/^[0-9a-f]{64}$/u.test(proof.verifier_sha256 ?? "") ||
    !Array.isArray(proof.source_files) ||
    proof.source_files.length !== POINT_SUPPORT_SOURCE_PATHS.length
  ) {
    fail("point-support proof lacks its versioned semantic source binding");
  }
  for (const [index, sourceFile] of proof.source_files.entries()) {
    const expectedPath = POINT_SUPPORT_SOURCE_PATHS[index];
    const expectedKind = expectedPath.endsWith(".rs")
      ? "rust-source"
      : "compile-time-input";
    exactKeys(sourceFile, ["path", "kind", "sha256"], `point source file ${index}`);
    if (
      sourceFile?.path !== expectedPath ||
      sourceFile?.kind !== expectedKind ||
      !/^[0-9a-f]{64}$/u.test(sourceFile?.sha256 ?? "")
    ) {
      fail(`point-support proof has malformed or non-canonical source file ${index}`);
    }
    const sourceBytes = await readFile(resolve(REPO_ROOT, expectedPath));
    if (sourceFile.sha256 !== sha256(sourceBytes)) {
      fail(`point-support proof source file ${expectedPath} drifted`);
    }
  }
  const algebra = proof.universal_algebraic_certificate;
  if (
    algebra?.method !==
      "exact-sparse-integer-polynomial-identities-plus-positive-denominator-order-lemma-v1" ||
    algebra?.wolfram_language_cross_check?.query_sha256 !==
      "8cdbb9964583030c8b92498961896cb2a98613f1cb31eb7c54acdf8e16beff10" ||
    algebra?.wolfram_language_cross_check?.result !==
      "{True, True, True, True, True}" ||
    algebra?.wolfram_language_cross_check?.result_sha256 !==
      "13a8f2ee8d0fde335a638e46d7cc8a8427b9a1437c77d22cfcf925bb87fa6303"
  ) {
    fail("point-support proof lacks the universal algebraic certificate");
  }
  const dependency = proof.q55_dependency;
  if (
    dependency?.artifact_id !== wcagProof.artifact_id ||
    dependency?.artifact_sha256 !== wcagProof.artifact_sha256 ||
    dependency?.proof_id !== wcagProof.proof_id ||
    dependency?.proof_sha256 !== sha256(wcagProofBytes) ||
    dependency?.proof_payload_sha256 !== wcagProof.proof_payload_sha256
  ) {
    fail("point-support proof does not bind the exact WCAG22 Q55 dependency");
  }
  const capabilitySite = numericalCapabilities?.sites?.find(
    ({ siteId }) => siteId === proof.site_id,
  );
  const expectedCapabilitySite = {
    siteId: proof.site_id,
    stableOutcomes: ["canonical-finite-bounded"],
    compatibilityReleases: [],
    evidenceClasses: ["canonical-finite-bounded"],
    artifactIds: [proof.artifact_id],
    boundIds: [proof.bound_id],
    proofIds: [proof.proof_id],
    runtimeAttestations: [],
  };
  if (!isDeepStrictEqual(capabilitySite, expectedCapabilitySite)) {
    fail("numerical capability manifest does not exactly project the point-support proof");
  }
  const evidencePath = `evidence/${proofFile}`;
  const proofArtifact = artifacts.find(({ path }) => path === evidencePath);
  if (proofArtifact?.sha256 !== sha256(proofBytes)) {
    fail("point-support proof artifact metadata does not bind its canonical bytes");
  }
  return {
    siteId: proof.site_id,
    profileId: proof.profile_id,
    artifactId: proof.artifact_id,
    boundId: proof.bound_id,
    proofId: proof.proof_id,
    proofSha256: sha256(proofBytes),
    proofPayloadSha256: proof.proof_payload_sha256,
    declaredOperationLaw: proof.declared_operation_law,
    certifiedClaim: proof.certified_claim,
    excludedClaim: proof.excluded_claim,
    sourceBinding: {
      schemaVersion: proof.source_binding_schema_version,
      law: proof.source_binding_law,
      scope: proof.source_binding_scope,
      exclusions: proof.source_binding_exclusions,
      closureSha256: proof.source_closure_sha256,
    },
    q55Dependency: {
      artifactId: dependency.artifact_id,
      artifactSha256: dependency.artifact_sha256,
      proofId: dependency.proof_id,
      proofSha256: dependency.proof_sha256,
      proofPayloadSha256: dependency.proof_payload_sha256,
    },
    artifacts: [proofArtifact],
  };
}

export function validateBuildMetadata(
  metadata,
  { packageJson, source, coreVersion, conformanceEvidence, wasm },
) {
  const expected = {
    schemaVersion: 2,
    package: { name: packageJson.name, version: packageJson.version },
    sourceSha: source,
    coreVersion,
    conformance: {
      packVersion: conformanceEvidence.packVersion,
      packDigest: conformanceEvidence.packDigest,
      manifestSha256: conformanceEvidence.manifestSha256,
      familySetSha256: conformanceEvidence.familySetSha256,
    },
    wasm: [{ role: "runtime", ...wasm.runtime }],
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

async function validatePackedNumericalEvidence(tarballPath, expectedArtifacts) {
  const extracted = await mkdtemp(join(tmpdir(), "labcolors-packed-evidence-"));
  try {
    command("tar", ["-xzf", tarballPath, "-C", extracted]);
    await validateNumericalEvidenceArtifacts(
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
  exactKeys(
    capabilities,
    ["schemaVersion", "coverage", "sites", "checksum"],
    "numericalCapabilities",
  );
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
    Array.isArray(value) &&
    value.every((key) => typeof key === "string" && key.length > 0) &&
    new Set(value).size === value.length;
  const siteIds = new Set();
  for (const site of capabilities.sites) {
    exactKeys(
      site,
      ["siteId", ...CAPABILITY_SITE_LIST_FIELDS],
      "numericalCapabilities site",
    );
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

const SOLVE_FAILURE_CATEGORY_BY_CODE = new Map([
  ["below_contrast_floor", "unreachable"],
  ["exceeds_range", "unreachable"],
  ["bounded_search_exhausted", "unresolved"],
  ["floor_unreachable", "unreachable"],
  ["gamut_unsupported", "unsupported"],
  ["invalid_input", "rejected"],
]);

export function validateSolveFailurePair(category, code, label = "solve failure") {
  const expectedCategory = SOLVE_FAILURE_CATEGORY_BY_CODE.get(code);
  if (expectedCategory === undefined) {
    fail(`${label} has unknown failure code ${code}`);
  }
  if (category !== expectedCategory) {
    fail(`${label} category ${category} differs from ${expectedCategory} for ${code}`);
  }
}

// Validate the versioned solve outcome algebra independently of Rust serde.
// The category/code pair is atomic: neither an old tag nor a plausible local
// reclassification may enter release evidence.
export function validateSolveFamily(family) {
  if (!Array.isArray(family) || family.length === 0) {
    fail("solve family must be a non-empty vector array");
  }
  let solved = 0;
  let failures = 0;
  for (const [index, vector] of family.entries()) {
    exactKeys(vector, ["bg", "contract", "theme", "outcome"], `solve[${index}]`);
    const outcome = vector.outcome;
    if (!isRecord(outcome)) fail(`solve[${index}].outcome must be an object`);
    if (outcome.kind === "solved") {
      exactKeys(
        outcome,
        ["kind", "hex", "lc", "wcagRatio", "floorOverride"],
        `solve[${index}].outcome`,
      );
      if (typeof outcome.hex !== "string" || !/^#[0-9A-F]{6}$/u.test(outcome.hex)) {
        fail(`solve[${index}].outcome.hex must be canonical #RRGGBB`);
      }
      if (!Number.isFinite(outcome.lc)) {
        fail(`solve[${index}].outcome.lc must be finite`);
      }
      if (
        !Number.isFinite(outcome.wcagRatio) ||
        outcome.wcagRatio < 1 ||
        outcome.wcagRatio > 21
      ) {
        fail(`solve[${index}].outcome.wcagRatio must be finite and within [1, 21]`);
      }
      if (typeof outcome.floorOverride !== "boolean") {
        fail(`solve[${index}].outcome.floorOverride must be boolean`);
      }
      solved += 1;
      continue;
    }
    if (outcome.kind !== "failure") {
      fail(`solve[${index}].outcome has unsupported kind ${outcome.kind}`);
    }
    exactKeys(outcome, ["kind", "category", "code"], `solve[${index}].outcome`);
    validateSolveFailurePair(outcome.category, outcome.code, `solve[${index}].outcome`);
    failures += 1;
  }
  if (solved === 0 || failures === 0) {
    fail(`solve family must exercise both outcomes, got solved=${solved} failure=${failures}`);
  }
}

async function validateConformance(conformance) {
  if (conformance.packVersion !== "10.0.0") {
    fail(`release requires conformance pack 10.0.0, got ${conformance.packVersion}`);
  }
  if (!/^[0-9a-f]{8}$/u.test(conformance.packDigest ?? "")) {
    fail(`invalid conformance packDigest: ${conformance.packDigest}`);
  }

  const familyBuffers = await Promise.all(
    CONFORMANCE_FAMILY_FILES.map((name) => readFile(resolve(CONFORMANCE_DIR, name))),
  );
  const proofPath = resolve(NUMERICAL_CONTRACT_DIR, "wcag22-srgb8-q55-proof-v1.json");
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
  const countKeys = ["contrasts", "ladders", "alpha", "solve", "wcag22"];
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
  validateSolveFamily(families[3]);
  const halfTie = families[2].find(
    (entry) => entry.tint === "#C0B2FA" && entry.bg === "#000000" && entry.alpha === 0.122,
  );
  if (halfTie?.composite !== "#17161F") {
    fail("conformance pack lacks the exact source-over half-tie #C0B2FA@0.122 -> #17161F");
  }
  const antiEpsilon = families[4].find(
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

function lockedTypescriptVersion(packageLock, packagePath, label) {
  const version = packageLock.packages?.[packagePath]?.version;
  if (!/^\d+\.\d+\.\d+(?:[-+].+)?$/u.test(version ?? "")) {
    fail(`package-lock.json does not pin an exact ${label} version`);
  }
  return version;
}

function lockedTypescriptCompilers(packageJson, packageLock) {
  const minimumSpec = packageJson.devDependencies?.["typescript-floor"];
  const minimumDeclared = minimumSpec?.match(
    /^npm:typescript@(\d+\.\d+\.\d+)$/u,
  )?.[1];
  if (!minimumDeclared) {
    fail("package.json must declare an exact npm:typescript@X.Y.Z consumer floor");
  }
  const compilers = [
    {
      role: "minimum-consumer",
      packageDirectory: "typescript-floor",
      version: lockedTypescriptVersion(
        packageLock,
        "node_modules/typescript-floor",
        "minimum consumer TypeScript",
      ),
    },
    {
      role: "workspace-locked",
      packageDirectory: "typescript",
      version: lockedTypescriptVersion(
        packageLock,
        "node_modules/typescript",
        "workspace TypeScript compiler",
      ),
    },
  ];
  if (compilers[0].version !== minimumDeclared) {
    fail(
      `minimum TypeScript lock ${compilers[0].version} differs from declared ${minimumDeclared}`,
    );
  }
  return compilers;
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

function runtimeSmokeSource() {
  return String.raw`
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

import init, {
  LabColors,
  adaptTheme,
  evaluateWcag22,
  numericalCapabilityManifest,
  watchTheme,
} from "@labpics/colors";
import * as colorsApi from "@labpics/colors";

const require = createRequire(import.meta.url);
for (const name of [
  "effectiveBackground",
  "parseCssColor",
  "compositeOver",
  "compositeStackToHex",
  "toHex",
  "oklabLerp",
]) {
  assert.equal(name in colorsApi, false, name + " must not be a root export");
}
await assert.rejects(
  import("@labpics/colors/effective-bg"),
  (error) => error?.code === "ERR_PACKAGE_PATH_NOT_EXPORTED",
);
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
assert.deepEqual(metadata.wasm.map(({ role }) => role), ["runtime"]);
const runtimeWasm = metadata.wasm.find(({ role }) => role === "runtime");
assert.deepEqual(runtimeWasm.path, "pkg/labcolors_bg.wasm");
assert.equal(runtimeWasm.bytes, (await readFile(wasmPath)).length);
await init({ module_or_path: await readFile(wasmPath) });

const capability = numericalCapabilityManifest();
assert.equal(capability.schemaVersion, 2);
assert.ok(capability.sites.some((site) =>
  site.siteId === "wcag22-srgb8-contrast-v1" &&
  site.proofIds.includes("wcag22-srgb8-full-domain-q55-v1")
));
assert.deepEqual(
  capability.sites.find((site) =>
    site.siteId === "point-support-retained-reference-surplus-v1"
  ),
  {
    siteId: "point-support-retained-reference-surplus-v1",
    stableOutcomes: ["canonical-finite-bounded"],
    compatibilityReleases: [],
    evidenceClasses: ["canonical-finite-bounded"],
    artifactIds: ["wcag22-srgb8-luminance-q55-v1"],
    boundIds: ["point-support-reference-surplus-q55-bps-v1"],
    proofIds: ["point-support-reference-surplus-integer-v1"],
    runtimeAttestations: [],
  },
);

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
    tint: { target_mp: 6.1, hue_stiffness: 9.0 },
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

const runtimeTarget = () => {
  const names = [];
  const values = new Map();
  return {
    values,
    style: {
      setProperty(name, value) {
        if (!values.has(name)) names.push(name);
        values.set(name, value);
      },
      removeProperty(name) {
        values.delete(name);
        const index = names.indexOf(name);
        if (index >= 0) names.splice(index, 1);
      },
      item(index) { return names[index] ?? ""; },
      get length() { return names.length; },
    },
  };
};
const watchedTarget = runtimeTarget();
const watcher = watchTheme(watchedTarget, {
  colors: engine,
  theme: "light",
  background,
  observe: false,
  win: {},
});
assert.equal(watcher.background(), background);
assert.equal(typeof watchedTarget.values.get("--lab-token-7f3a"), "string");
watcher.refresh();
watcher.stop();

const adaptedTarget = runtimeTarget();
const adaptive = adaptTheme(adaptedTarget, {
  colors: engine,
  theme: "light",
  background,
  target: adaptedTarget,
  now: () => 0,
  win: {},
});
adaptive.tick(0);
assert.equal(typeof adaptive.current()["--lab-token-7f3a"], "string");
adaptive.stop();

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
} else if (material.alphaGuarantee.kind === "bisection-bracket-characterized-v1") {
  assert.equal(material.alpha, material.alphaGuarantee.upperAlpha);
  assert.equal(material.alphaStatus, "satisfied");
} else {
  assert.equal(material.alphaGuarantee.kind, "opaque-endpoint-characterized-v1");
  assert.equal(material.alpha, 1);
  assert.equal(material.alphaStatus, "degraded");
}
assert.equal(Object.hasOwn(material, "guaranteed"), false);
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
assert.equal(Object.hasOwn(glow, "achievedDj"), false);
assert.equal(Object.hasOwn(glow, "degraded"), false);
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
assert.equal(Object.hasOwn(stableNoop, "degraded"), false);
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
  numericalCapabilityManifest,
  type GlowDecisionGuaranteeV1,
  type GlowDeterminateRole,
  type GlowDeterminateRoleBase,
  type GlowRole,
  type GlowTargetStatusV1,
  type LadderPositionV1,
  type MaterialRole,
  type NumericalCapabilityManifestV2,
  type NumericalIndeterminacyV1,
  type ResolvedTheme,
  type ThemeConfig,
  type TranslucentRole,
  type Wcag22AssessmentV1,
  type Wcag22CriterionV1,
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

const initialise: typeof init = init;
const apply: typeof applyTheme = applyTheme;
const watch: typeof watchTheme = watchTheme;
const adapt: typeof adaptTheme = adaptTheme;
type PublicSubpathTypes =
  | WatchController
  | WatchThemeOptions
  | AdaptController
  | AdaptThemeOptions;
declare const publicSubpathType: PublicSubpathTypes;
void [apply, watch, adapt, publicSubpathType];
declare const rootApi: typeof import("@labpics/colors");
// @ts-expect-error low-level browser-shell colour math is not public API.
rootApi.parseCssColor;
// @ts-expect-error the compatibility background estimate is package-internal.
rootApi.effectiveBackground;
const engine = new LabColors();
const removedStrict: AdaptThemeOptions = {
  colors: engine,
  theme: "light",
  // @ts-expect-error the unverified legacy transition clamp was removed.
  strict: true,
};
void removedStrict;
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
    tint: { target_mp: 6.1, hue_stiffness: 9.0 },
  },
  palette: [],
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
    void selection;
    return status;
  }
  if (role.targetStatus === "legacy-reached") {
    return role.targetStatus;
  }
  const status: "legacy-unreachable" = role.targetStatus;
  return status;
}

function materialEvidence(role: MaterialRole): number | string {
  if (role.alphaStatus === "degraded") {
    const alpha: 1 = role.alpha;
    void alpha;
    return role.alphaStatus;
  }
  if (role.alphaGuarantee.kind === "bisection-bracket-characterized-v1") {
    return role.alphaGuarantee.upperAlpha;
  }
  return role.alphaStatus;
}

declare const glowBase: GlowDeterminateRoleBase;
// @ts-expect-error stable profile не может нести legacy status.
const impossibleGlow: GlowDeterminateRole = {
  ...glowBase,
  decisionProfile: "stable-v1",
  decisionGuarantee: { kind: "bit-exact" },
  selectionDiagnosticProfile: null,
  targetStatus: "legacy-reached",
};

declare const noAliasGlow: GlowDeterminateRole;
// @ts-expect-error ambiguous measurement alias was removed.
noAliasGlow.achievedDj;
// @ts-expect-error boolean duplicate of targetStatus was removed.
noAliasGlow.degraded;
declare const noAliasMaterial: MaterialRole;
// @ts-expect-error boolean duplicate of alphaStatus was removed.
noAliasMaterial.guaranteed;

void [
  initialise,
  fingerprint,
  resolved,
  wcagAssessment,
  capability,
  config,
  alphaContract,
  glowContract,
  decisionEvidence,
  indeterminacyEvidence,
  determinateGlowEvidence,
  materialEvidence,
  impossibleGlow,
  noAliasGlow,
  noAliasMaterial,
];
`;
}

async function verifyCleanConsumer(
  tarballPath,
  packageJson,
  typescriptCompilers,
  expectedBuildMetadata,
  expectedNumericalArtifacts,
) {
  const consumer = await mkdtemp(join(tmpdir(), "labcolors-release-consumer-"));
  try {
    await writeFile(
      join(consumer, "package.json"),
      `${JSON.stringify({ private: true, type: "module" }, null, 2)}\n`,
    );

    for (const compiler of typescriptCompilers) {
      const localTypescript = await readJson(
        resolve(
          PACKAGE_DIR,
          "node_modules",
          compiler.packageDirectory,
          "package.json",
        ),
      );
      if (localTypescript.version !== compiler.version) {
        fail(
          `installed ${compiler.role} TypeScript ${localTypescript.version} ` +
            `differs from lockfile ${compiler.version}`,
        );
      }
    }

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

    await validateNumericalEvidenceArtifacts(
      installed,
      expectedNumericalArtifacts,
      "clean-installed package",
    );

    const expectedWasm = new Map(
      expectedBuildMetadata.wasm.map((artifact) => [artifact.role, artifact]),
    );
    for (const [role, path] of [["runtime", "pkg/labcolors_bg.wasm"]]) {
      const expected = expectedWasm.get(role);
      const installedWasm = await readFile(resolve(installed, path));
      if (
        expected?.path !== path ||
        expected.bytes !== installedWasm.length ||
        expected.sha256 !== sha256(installedWasm)
      ) {
        fail(`clean-installed ${role} WASM differs from the packed release input`);
      }
    }
    const installedBuildMetadata = await readJson(resolve(installed, "build-metadata.json"));
    if (!isDeepStrictEqual(installedBuildMetadata, expectedBuildMetadata)) {
      fail("clean-installed build metadata differs from the verified release inputs");
    }

    const runtimePath = resolve(consumer, "runtime-smoke.mjs");
    const typesPath = resolve(consumer, "smoke.ts");
    await writeFile(runtimePath, runtimeSmokeSource());
    await writeFile(typesPath, typeSmokeSource());

    command(process.execPath, [runtimePath], consumer);
    for (const compiler of typescriptCompilers) {
      command(
        process.execPath,
        [
          resolve(
            PACKAGE_DIR,
            "node_modules",
            compiler.packageDirectory,
            "lib",
            "tsc.js",
          ),
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
    }

  } finally {
    await rm(consumer, { recursive: true, force: true });
  }
}

// Execute the same packed-package runtime smoke under the caller's Node binary.
// CI uses this to prove the public consumer floor independently from the pinned
// release packer.
export async function smokePackedPackage(tarballPath) {
  const tarball = resolve(tarballPath);
  const consumer = await mkdtemp(join(tmpdir(), "labcolors-package-smoke-"));
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
    const runtimePath = resolve(consumer, "smoke.mjs");
    await writeFile(runtimePath, runtimeSmokeSource());
    command(process.execPath, [runtimePath], consumer);
  } finally {
    await rm(consumer, { recursive: true, force: true });
  }
}

export async function verifyPackageRelease() {
  const { sourceSha: source } = await prepareNpmPackage();
  command("python3", ["scripts/verify_wcag22_q55.py"], REPO_ROOT);
  command("python3", ["scripts/verify_point_support_surplus.py"], REPO_ROOT);
  const numericalEvidenceArtifacts = await validateNumericalEvidence();
  const wcag22Evidence = await validateWcag22Evidence(numericalEvidenceArtifacts);

  const [packageJson, packageLock, cargoSource, conformance] = await Promise.all([
    readJson(PACKAGE_JSON),
    readJson(PACKAGE_LOCK),
    readFile(ROOT_CARGO, "utf8"),
    readJson(CONFORMANCE_MANIFEST),
  ]);
  assertPackageEvidenceInventory(packageJson.files);

  const coreVersion = workspaceVersion(cargoSource);
  const npmVersion = lockedNpmVersion(packageJson);
  const typescriptCompilers = lockedTypescriptCompilers(packageJson, packageLock);
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
  const pointSupportEvidence = await validatePointSupportEvidence(
    numericalEvidenceArtifacts,
    conformance.numericalCapabilities,
  );

  const wasmPaths = {
    runtime: [RUNTIME_WASM_PATH, "pkg/labcolors_bg.wasm"],
  };
  const wasm = {};
  for (const [role, [path, displayPath]] of Object.entries(wasmPaths)) {
    const bytes = await readFile(path);
    if (bytes.length < 8 || !bytes.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109]))) {
      fail(`${displayPath} is absent or has no WebAssembly magic header`);
    }
    wasm[role] = await hashedArtifact(path, displayPath);
  }
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

  await validatePackedNumericalEvidence(canonicalPack.path, numericalEvidenceArtifacts);

  const tarball = await hashedArtifact(
    canonicalPack.path,
    `.release/${canonicalPack.tarballName}`,
  );
  await verifyCleanConsumer(
    canonicalPack.path,
    packageJson,
    typescriptCompilers,
    buildMetadataValue,
    numericalEvidenceArtifacts,
  );

  const manifest = {
    // V4 replaces the old point-support capsule projection with the exact
    // whole-file semantic-cone projection and its explicit claim boundary.
    schemaVersion: 4,
    npm: packageJson.version,
    core: coreVersion,
    wire: {
      identity: `resolved-theme@${packageJson.version}`,
      embeddedInPayload: false,
      trackingIssue: 258,
    },
    conformance: conformanceEvidence,
    normativeEvidence: { wcag22: wcag22Evidence },
    numericalEvidence: {
      pointSupportReferenceSurplus: pointSupportEvidence,
    },
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
        compiler: typescriptCompilers[1].version,
        minimumConsumerCompiler: typescriptCompilers[0].version,
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
    ],
    numericalCapabilities: conformance.numericalCapabilities,
    unsupported: [
      "embedded-wire-schema-version",
      "stable-cam16-glow-target-or-maximum-selection",
      "renderer-or-output-pipeline-equivalence",
      "spatial-glow-field",
      "display-p3",
    ],
    artifacts: {
      tarball,
      wasm: [{ role: "runtime", ...wasm.runtime }],
      buildMetadata,
    },
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
  const packageSmokeIndex = process.argv.indexOf("--package-smoke");
  const action = packageSmokeIndex >= 0
    ? (() => {
        const tarball = process.argv[packageSmokeIndex + 1];
        if (!tarball) fail("--package-smoke requires a tarball path");
        return smokePackedPackage(tarball).then(() => ({ packageSmoke: tarball }));
      })()
    : verifyPackageRelease();
  action
    .then(async ({ manifest, tarball }) => {
      if (packageSmokeIndex >= 0) {
        console.log(`package smoke passed: ${resolve(process.argv[packageSmokeIndex + 1])}`);
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
