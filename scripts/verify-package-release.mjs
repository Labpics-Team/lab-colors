import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import {
  appendFile,
  chmod,
  lstat,
  mkdtemp,
  mkdir,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
  win32 as windowsPath,
} from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

import { atomicWriteGeneratedFile } from "./atomic-write.mjs";
import { workspaceVersion } from "./cargo-workspace.mjs";
import {
  PRIVATE_PROGRAM_CONSUMER_PATH,
  PRIVATE_PROGRAM_METADATA_PATH,
  PRIVATE_PROGRAM_ROLE,
  PRIVATE_PROGRAM_SYMBOL_PREFIX,
  PRIVATE_PROGRAM_WASM_PATH,
} from "./build-private-program.mjs";
import { runCanonicalPrivateProgramBuild } from "./run-canonical-private-program-build.mjs";
import {
  PACKAGE_DIR,
  REPO_ROOT,
  prepareNpmPackage,
  verifiedSourceSha,
} from "./prepare-npm-package.mjs";
import {
  NUMERICAL_EVIDENCE_FILES,
  PACKED_NUMERICAL_EVIDENCE_PATHS,
  POINT_SUPPORT_EVIDENCE_FILES,
  WCAG22_EVIDENCE_FILES,
  assertPackageEvidenceInventory,
} from "./release-evidence.mjs";
import pointSupportReleaseContract from "./point-support-release-contract.cjs";

const {
  POINT_SUPPORT_CERTIFIED_CLAIM,
  POINT_SUPPORT_EXCLUDED_CLAIM,
  POINT_SUPPORT_SOURCE_BINDING_SCOPE,
  POINT_SUPPORT_SOURCE_BINDING_EXCLUSIONS,
  POINT_SUPPORT_SOURCE_PATHS,
  exactJsonPayloadWithoutTopLevelField,
} = pointSupportReleaseContract;

const RELEASE_DIR = resolve(PACKAGE_DIR, ".release");
const RELEASE_MANIFEST = resolve(RELEASE_DIR, "release-manifest.json");
const PACKAGE_JSON = resolve(PACKAGE_DIR, "package.json");
const PACKAGE_LOCK = resolve(PACKAGE_DIR, "package-lock.json");
const BUILD_METADATA = resolve(PACKAGE_DIR, "build-metadata.json");
const PRIVATE_PROGRAM_METADATA = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_METADATA_PATH);
const PRIVATE_PROGRAM_CONSUMER = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_CONSUMER_PATH);
const PRIVATE_PROGRAM_WASM = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_WASM_PATH);
const NPM_TARBALL_INSPECTOR = resolve(REPO_ROOT, "scripts/inspect-npm-tarball.py");
const VERIFIED_TARBALL_DIRECTORY_PREFIX = "labcolors-release-verified-";
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

export function npmInvocation(options = {}) {
  const {
    platform = process.platform,
    node = process.execPath,
    pathExists = existsSync,
  } = options;
  const lifecycleEntrypoint = Object.hasOwn(options, "lifecycleEntrypoint")
    ? options.lifecycleEntrypoint
    : process.env.npm_execpath;
  const entrypoint = lifecycleEntrypoint?.trim();
  if (
    entrypoint &&
    (platform === "win32" ? windowsPath.isAbsolute(entrypoint) : isAbsolute(entrypoint)) &&
    /(?:^|[\\/])npm-cli\.(?:c?js|mjs)$/u.test(entrypoint) &&
    pathExists(entrypoint)
  ) {
    return { commandName: node, argsPrefix: [entrypoint] };
  }
  if (platform === "win32") {
    const siblingEntrypoint = windowsPath.resolve(
      windowsPath.dirname(node),
      "node_modules",
      "npm",
      "bin",
      "npm-cli.js",
    );
    if (!pathExists(siblingEntrypoint)) {
      throw new Error(`npm CLI entrypoint is unavailable: ${siblingEntrypoint}`);
    }
    return {
      commandName: node,
      argsPrefix: [siblingEntrypoint],
    };
  }
  return { commandName: "npm", argsPrefix: [] };
}

function npm(args, cwd = REPO_ROOT) {
  // Windows cannot execute a .cmd shim through execFileSync without a shell.
  // Invoke npm's sibling JavaScript entrypoint instead, preserving an argv-only
  // boundary and avoiding shell quoting or injection.
  const invocation = npmInvocation();
  return command(invocation.commandName, [...invocation.argsPrefix, ...args], cwd);
}

export function pythonInvocation({ platform = process.platform } = {}) {
  return platform === "win32"
    ? { commandName: "py", argsPrefix: ["-3"] }
    : { commandName: "python3", argsPrefix: [] };
}

function python(args, cwd = REPO_ROOT) {
  const invocation = pythonInvocation();
  return command(invocation.commandName, [...invocation.argsPrefix, ...args], cwd);
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
    fail,
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
    proof.source_negative_controls !== 43 ||
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

export function validatePrivateProgramMetadata(
  metadata,
  { packageJson, source, coreVersion, privateProgramBuild, artifacts },
) {
  const expected = {
    schemaVersion: 1,
    role: PRIVATE_PROGRAM_ROLE,
    package: { name: packageJson.name, version: packageJson.version },
    source: {
      gitSha: source,
      core: {
        crate: privateProgramBuild.build.crate,
        version: coreVersion,
        digest: privateProgramBuild.source,
      },
    },
    build: privateProgramBuild.build,
    artifacts,
  };
  if (!isDeepStrictEqual(metadata, expected)) {
    fail(
      "private Program metadata does not exactly bind its release inputs:\n" +
        `expected ${JSON.stringify(expected)}\n` +
        `actual   ${JSON.stringify(metadata)}`,
    );
  }
}

export function validateRuntimeWasmIsolation(bytes) {
  if (bytes.includes(Buffer.from(PRIVATE_PROGRAM_SYMBOL_PREFIX, "utf8"))) {
    fail(
      `public runtime WASM contains private Program symbol prefix ` +
        PRIVATE_PROGRAM_SYMBOL_PREFIX,
    );
  }
}

function normalisePackPath(path) {
  if (
    typeof path !== "string" ||
    !path ||
    path !== path.normalize("NFC") ||
    path.includes("\\") ||
    path.startsWith("/") ||
    /^[A-Za-z]:/u.test(path) ||
    /[\u0000-\u001f\u007f]/u.test(path)
  ) {
    fail(`unsafe or ambiguous package path: ${path}`);
  }
  const segments = path.split("/");
  if (segments.some((segment) => !segment || segment === "." || segment === "..")) {
    fail(`non-canonical package path: ${path}`);
  }
  return path;
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

function expectedPackedFiles(packageJson) {
  const expected = new Set([
    ...REQUIRED_PACK_FILES,
    ...(packageJson.files ?? []).map(normalisePackPath),
  ]);
  for (const target of exportTargets(packageJson.exports)) expected.add(normalisePackPath(target));
  if (typeof packageJson.types === "string") {
    // npm declares the top-level types field with an explicit "./" spelling;
    // normalise it exactly like an export target before the canonical check.
    expected.add(normalisePackPath(packageJson.types.replace(/^\.\//u, "")));
  }
  return [...expected].sort((left, right) =>
    Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8")),
  );
}

function validatePackedFiles(packageJson, packResult) {
  if (!Array.isArray(packResult.files) || packResult.files.length === 0) {
    fail("npm pack did not report a non-empty files inventory");
  }

  const actual = packResult.files.map((entry, index) => {
    if (!Number.isSafeInteger(entry?.size) || entry.size < 0) {
      fail(`npm pack reported an invalid byte size for files[${index}]`);
    }
    return normalisePackPath(entry.path);
  });
  const duplicates = actual.filter((path, index) => actual.indexOf(path) !== index);
  if (duplicates.length > 0) fail(`npm pack reported duplicate paths: ${duplicates.join(", ")}`);

  const expected = expectedPackedFiles(packageJson);
  const expectedSet = new Set(expected);

  const actualSet = new Set(actual);
  const missing = expected.filter((path) => !actualSet.has(path));
  if (missing.length > 0) fail(`npm tarball is missing required files: ${missing.join(", ")}`);

  const undeclared = actual.filter((path) => !expectedSet.has(path)).sort();
  if (undeclared.length > 0) {
    fail(`npm tarball contains undeclared files: ${undeclared.join(", ")}`);
  }

  for (const path of actual) {
    const segments = path.split("/");
    const forbidden = segments.find((segment) => FORBIDDEN_PACK_SEGMENTS.has(segment));
    if (forbidden) fail(`npm tarball contains forbidden ${forbidden} path: ${path}`);
  }

  const totalFileBytes = packResult.files.reduce((total, entry) => total + entry.size, 0);
  if (packResult.entryCount !== actual.length || packResult.unpackedSize !== totalFileBytes) {
    fail("npm pack summary does not exactly match its reported files inventory");
  }
  return { actual, expected, totalFileBytes };
}

async function inspectNpmTarball(tarballPath, expected, packResult) {
  const temporary = await mkdtemp(join(tmpdir(), "labcolors-npm-inventory-"));
  const declaration = resolve(temporary, "declared-inventory.json");
  let inspection;
  try {
    await writeFile(
      declaration,
      `${JSON.stringify({ schemaVersion: 1, files: expected })}\n`,
      { encoding: "utf8", flag: "wx", mode: 0o600 },
    );
    const output = python([
      NPM_TARBALL_INSPECTOR,
      "--tarball",
      tarballPath,
      "--declared-inventory-json",
      declaration,
    ]);
    try {
      inspection = JSON.parse(output);
    } catch (error) {
      fail(`npm tarball inspector returned invalid JSON: ${error.message}`);
    }
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }

  exactKeys(
    inspection,
    ["schemaVersion", "verdict", "tarball", "limits", "members", "inventory"],
    "npm tarball inspection",
  );
  if (inspection.schemaVersion !== 1 || inspection.verdict !== "canonical") {
    fail("npm tarball inspector did not return its canonical schema-1 verdict");
  }
  exactKeys(inspection.tarball, ["bytes", "sha256"], "npm tarball inspection artifact");
  exactKeys(
    inspection.limits,
    ["maxMembers", "maxTarballBytes", "maxTotalFileBytes"],
    "npm tarball inspection limits",
  );
  exactKeys(
    inspection.inventory,
    ["files", "totalFileBytes"],
    "npm tarball inspection inventory",
  );
  if (
    !Array.isArray(inspection.members) ||
    inspection.members.length !== expected.length ||
    !isDeepStrictEqual(inspection.inventory.files, expected) ||
    inspection.inventory.totalFileBytes !== packResult.unpackedSize ||
    inspection.limits.maxMembers !== expected.length ||
    !Number.isSafeInteger(inspection.tarball.bytes) ||
    inspection.tarball.bytes <= 0 ||
    !/^[0-9a-f]{64}$/u.test(inspection.tarball.sha256 ?? "")
  ) {
    fail("npm tarball inspection receipt differs from the declared package inventory");
  }

  const reportedByPath = new Map(
    packResult.files.map((entry) => [normalisePackPath(entry.path), entry]),
  );
  for (const [index, member] of inspection.members.entries()) {
    exactKeys(
      member,
      ["index", "rawPath", "normalizedPath", "type", "size", "sha256"],
      `npm tarball member ${index}`,
    );
    const reported = reportedByPath.get(member.normalizedPath);
    if (
      member.index !== index ||
      member.rawPath !== `package/${member.normalizedPath}` ||
      member.type !== "file" ||
      !Number.isSafeInteger(member.size) ||
      member.size < 0 ||
      !/^[0-9a-f]{64}$/u.test(member.sha256 ?? "") ||
      reported?.size !== member.size
    ) {
      fail(`npm tarball member ${index} differs from npm's reported regular file`);
    }
  }

  const bytes = await readFile(tarballPath);
  const digest = sha256(bytes);
  if (
    inspection.tarball.bytes !== bytes.length ||
    inspection.tarball.sha256 !== digest ||
    packResult.size !== bytes.length ||
    packResult.shasum !== createHash("sha1").update(bytes).digest("hex") ||
    packResult.integrity !== `sha512-${createHash("sha512").update(bytes).digest("base64")}`
  ) {
    fail("npm tarball bytes changed or differ from npm's cryptographic pack receipt");
  }
  return { bytes, sha256: digest, inspection };
}

export async function packInto(destination, packageJson) {
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
  const { expected } = validatePackedFiles(packageJson, packResult);
  const path = resolve(destination, tarballName);
  const inspected = await inspectNpmTarball(path, expected, packResult);

  return { path, tarballName, expected, packResult, ...inspected };
}

export async function materializeVerifiedTarballSnapshot(pack) {
  const directory = await mkdtemp(join(tmpdir(), VERIFIED_TARBALL_DIRECTORY_PREFIX));
  let retain = false;
  try {
    await chmod(directory, 0o700);
    if (!/^[0-9a-f]{64}$/u.test(pack.sha256 ?? "")) {
      fail("verified tarball snapshot requires a lowercase SHA-256 identity");
    }
    const path = resolve(directory, `${pack.sha256}.tgz`);
    await atomicWriteGeneratedFile(path, pack.bytes);
    await chmod(path, 0o600);
    const metadata = await lstat(path);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.nlink !== 1) {
      fail("verified tarball snapshot is not one single-link regular file");
    }
    const verified = await inspectNpmTarball(path, pack.expected, pack.packResult);
    if (!verified.bytes.equals(pack.bytes) || verified.sha256 !== pack.sha256) {
      fail("verified tarball snapshot differs from the inspected npm pack bytes");
    }
    retain = true;
    return { path, bytes: verified.bytes, sha256: verified.sha256 };
  } finally {
    if (!retain) await rm(directory, { recursive: true, force: true });
  }
}

async function validatePackedNumericalEvidence(tarballBytes, expectedArtifacts) {
  const extracted = await mkdtemp(join(tmpdir(), "labcolors-packed-evidence-"));
  try {
    const tarballPath = resolve(extracted, "verified-package.tgz");
    const contents = resolve(extracted, "contents");
    await writeFile(tarballPath, tarballBytes, { flag: "wx", mode: 0o600 });
    await mkdir(contents);
    command("tar", ["-xzf", tarballPath, "-C", contents]);
    await validateNumericalEvidenceArtifacts(
      resolve(contents, "package"),
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
  ["unsatisfiable_criterion", "unreachable"],
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

export function runtimeSmokeSource() {
  return String.raw`
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

const runtimeElements = new WeakSet();
const runtimeDocuments = new WeakSet();
const runtimeShadowRoots = new WeakSet();
const runtimeStates = new WeakMap();

const runtimeDescriptor = (receiver, property) => {
  let owner = receiver;
  while (owner !== null) {
    const descriptor = Object.getOwnPropertyDescriptor(owner, property);
    if (descriptor !== undefined) return descriptor;
    owner = Object.getPrototypeOf(owner);
  }
  return undefined;
};
const runtimeRead = (receiver, property) => {
  const descriptor = runtimeDescriptor(receiver, property);
  if (descriptor === undefined) throw new TypeError("missing runtime " + property);
  if ("value" in descriptor) return descriptor.value;
  if (typeof descriptor.get !== "function") throw new TypeError("unreadable runtime " + property);
  return Reflect.apply(descriptor.get, receiver, []);
};
const runtimeWrite = (receiver, property, value) => {
  const descriptor = runtimeDescriptor(receiver, property);
  if (descriptor === undefined) throw new TypeError("missing runtime " + property);
  if (typeof descriptor.set === "function") Reflect.apply(descriptor.set, receiver, [value]);
  else if ("value" in descriptor && descriptor.writable) receiver[property] = value;
  else throw new TypeError("readonly runtime " + property);
};

class RuntimeNode {
  get nodeType() {
    if (runtimeDocuments.has(this)) return 9;
    if (runtimeShadowRoots.has(this)) return 11;
    if (runtimeElements.has(this)) return 1;
    throw new TypeError("invalid RuntimeNode receiver");
  }
  get ownerDocument() {
    const state = runtimeStates.get(this);
    if (state) return state.document ?? null;
    if (runtimeDocuments.has(this)) return null;
    return runtimeRead(this, "ownerDocument");
  }
  get isConnected() {
    const state = runtimeStates.get(this);
    return state ? state.connected === true : runtimeRead(this, "isConnected");
  }
  getRootNode() {
    const state = runtimeStates.get(this);
    if (state) return state.root ?? this;
    const callable = runtimeRead(this, "getRootNode");
    return Reflect.apply(callable, this, []);
  }
}
class RuntimeStyleDeclaration {
  get length() {
    const state = runtimeStates.get(this);
    return state ? state.names.length : runtimeRead(this, "length");
  }
  item(index) {
    const state = runtimeStates.get(this);
    if (state) return state.names[index] ?? "";
    return Reflect.apply(runtimeRead(this, "item"), this, [index]);
  }
}
class RuntimeElement extends RuntimeNode {
  get shadowRoot() {
    const state = runtimeStates.get(this);
    if (state) return state.shadowRoot ?? null;
    return runtimeDescriptor(this, "shadowRoot") === undefined
      ? null
      : runtimeRead(this, "shadowRoot");
  }
  get style() {
    const state = runtimeStates.get(this);
    return state ? state.style : runtimeRead(this, "style");
  }
  attachShadow(init) {
    const state = runtimeStates.get(this);
    if (!state || init?.mode !== "open" || state.shadowRoot) throw new TypeError("invalid attachShadow");
    const root = new RuntimeShadowRoot();
    runtimeShadowRoots.add(root);
    runtimeStates.set(root, { adopted: [], document: state.document, host: this, mode: "open" });
    state.shadowRoot = root;
    return root;
  }
}
class RuntimeDocument extends RuntimeNode {
  get documentElement() {
    const state = runtimeStates.get(this);
    return state ? state.documentElement : runtimeRead(this, "documentElement");
  }
  get defaultView() {
    const state = runtimeStates.get(this);
    return state ? state.realm : runtimeRead(this, "defaultView");
  }
  createElement() {
    const state = runtimeStates.get(this);
    if (!state) throw new TypeError("invalid RuntimeDocument receiver");
    const element = new RuntimeElement();
    const style = new RuntimeStyleDeclaration();
    runtimeElements.add(element);
    runtimeStates.set(style, { names: [] });
    runtimeStates.set(element, {
      connected: false,
      document: this,
      root: this,
      shadowRoot: null,
      style,
    });
    return element;
  }
  get adoptedStyleSheets() {
    const state = runtimeStates.get(this);
    return state ? state.adopted : runtimeRead(this, "adoptedStyleSheets");
  }
  set adoptedStyleSheets(value) {
    const state = runtimeStates.get(this);
    if (state) state.adopted = Array.from(value);
    else runtimeWrite(this, "adoptedStyleSheets", value);
  }
}
class RuntimeShadowRoot extends RuntimeNode {
  get host() {
    const state = runtimeStates.get(this);
    return state ? state.host : runtimeRead(this, "host");
  }
  get mode() {
    const state = runtimeStates.get(this);
    return state ? state.mode : runtimeRead(this, "mode");
  }
  get adoptedStyleSheets() {
    const state = runtimeStates.get(this);
    return state ? state.adopted : runtimeRead(this, "adoptedStyleSheets");
  }
  set adoptedStyleSheets(value) {
    const state = runtimeStates.get(this);
    if (state) state.adopted = Array.from(value);
    else runtimeWrite(this, "adoptedStyleSheets", value);
  }
}

const ambientDocument = new RuntimeDocument();
const ambientElement = new RuntimeElement();
const ambientStyle = new RuntimeStyleDeclaration();
const ambientRealm = { CSSStyleSheet: class RuntimeCSSStyleSheet {} };
runtimeDocuments.add(ambientDocument);
runtimeElements.add(ambientElement);
runtimeStates.set(ambientStyle, { names: [] });
runtimeStates.set(ambientElement, {
  connected: true,
  document: ambientDocument,
  root: ambientDocument,
  shadowRoot: null,
  style: ambientStyle,
});
runtimeStates.set(ambientDocument, {
  adopted: [],
  documentElement: ambientElement,
  realm: ambientRealm,
});
Object.defineProperty(globalThis, "document", {
  configurable: true,
  value: ambientDocument,
  writable: false,
});

const colorsApi = await import("@labpics/colors");
const {
  default: init,
  LabColors,
  adaptTheme,
  applyTheme,
  evaluateWcag22,
  numericalCapabilityManifest,
  watchTheme,
} = colorsApi;
const applyThemeApi = await import("@labpics/colors/apply-theme");
const watchThemeApi = await import("@labpics/colors/watch-theme");
const adaptThemeApi = await import("@labpics/colors/adapt-theme");
const { applyTheme: applyThemeFromSubpath } = applyThemeApi;
const { watchTheme: watchThemeFromSubpath } = watchThemeApi;
const { adaptTheme: adaptThemeFromSubpath } = adaptThemeApi;
assert.equal(applyThemeFromSubpath, applyTheme, "root and apply-theme share one function");
assert.equal(watchThemeFromSubpath, watchTheme, "root and watch-theme share one function");
assert.equal(adaptThemeFromSubpath, adaptTheme, "root and adapt-theme share one function");

const require = createRequire(import.meta.url);
for (const name of [
  "effectiveBackground",
  "parseCssColor",
  "compositeOver",
  "compositeStackToHex",
  "toHex",
  "oklabLerp",
  "createOutputSink",
  "sequenceIdentityMatches",
]) {
  assert.equal(name in colorsApi, false, name + " must not be a root export");
}
for (const [subpath, namespace] of [
  ["apply-theme", applyThemeApi],
  ["watch-theme", watchThemeApi],
  ["adapt-theme", adaptThemeApi],
]) {
  assert.equal("createOutputSink" in namespace, false, subpath + " must not export a sink factory");
}
await assert.rejects(
  import("@labpics/colors/effective-bg"),
  (error) => error?.code === "ERR_PACKAGE_PATH_NOT_EXPORTED",
);
await assert.rejects(
  import("@labpics/colors/output-sink"),
  (error) => error?.code === "ERR_PACKAGE_PATH_NOT_EXPORTED",
);
await assert.rejects(
  import("@labpics/colors/output-bindings"),
  (error) => error?.code === "ERR_PACKAGE_PATH_NOT_EXPORTED",
);
await assert.rejects(
  import("@labpics/colors/sequence-identity-matches"),
  (error) => error?.code === "ERR_PACKAGE_PATH_NOT_EXPORTED",
);
for (const privateSubpath of [
  "@labpics/colors/private-program/abi-v2.js",
  "@labpics/colors/private-program/consumer.js",
  "@labpics/colors/private-program/labcolors_private_program.wasm",
  "@labpics/colors/private-program/build-metadata.json",
]) {
  await assert.rejects(
    import(privateSubpath),
    (error) => error?.code === "ERR_PACKAGE_PATH_NOT_EXPORTED",
  );
}
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

const runtimeDocument = () => {
  class FakeStyleDeclaration {
    constructor(entries = []) {
      this.values = new Map(entries);
    }

    get length() { return this.values.size; }
    item(index) { return [...this.values.keys()][index] ?? ""; }
    getPropertyValue(name) { return this.values.get(name) ?? ""; }
    setProperty(name, value) {
      if (/^--[a-z0-9-]+$/u.test(name) && typeof value === "string" && value.length > 0) {
        this.values.set(name, value);
      }
    }
    removeProperty(name) {
      const previous = this.getPropertyValue(name);
      this.values.delete(name);
      return previous;
    }
    get cssText() {
      return [...this.values]
        .map(([name, value]) => name + ": " + value + ";")
        .join(" ");
    }
    entries() { return [...this.values]; }
  }

  class FakeRule {
    constructor(selectorText, declarations = []) {
      this.selectorText = selectorText;
      this.style = new FakeStyleDeclaration(declarations);
    }
    get cssText() {
      return this.style.cssText === ""
        ? this.selectorText + " {}"
        : this.selectorText + " { " + this.style.cssText + " }";
    }
  }

  const parseSheet = (text) => {
    if (text === "") return [];
    const match = /^(:root|:host) \{(?: (.*))?\}$/u.exec(text);
    if (!match) return [];
    const declarations = [];
    for (const part of (match[2] ?? "").split("; ")) {
      const declaration = part.endsWith(";") ? part.slice(0, -1) : part;
      if (declaration === "") continue;
      const separator = declaration.indexOf(": ");
      if (separator < 0) continue;
      declarations.push([
        declaration.slice(0, separator),
        declaration.slice(separator + 2),
      ]);
    }
    return [new FakeRule(match[1], declarations)];
  };

  const inlineStyle = new FakeStyleDeclaration();
  const values = new Map();
  let adoptedStyleSheets = [];
  let documentElement = null;
  let constructedSheetCount = 0;
  let scratchReplaceCount = 0;
  let liveReplaceCount = 0;

  const document = {
    nodeType: 9,
    defaultView: null,
    documentElement: null,
  };

  const selectorMatches = (selector) => selector === ":root";
  const syncEffectiveValues = () => {
    values.clear();
    for (const sheet of adoptedStyleSheets) {
      for (const rule of sheet.cssRules) {
        if (!selectorMatches(rule.selectorText)) continue;
        for (const [name, value] of rule.style.entries()) values.set(name, value);
      }
    }
    for (const [name, value] of inlineStyle.entries()) values.set(name, value);
  };

  class FakeCSSStyleSheet {
    constructor() {
      constructedSheetCount++;
      this.cssRules = [];
    }
    replaceSync(text) {
      const live = adoptedStyleSheets.includes(this);
      this.cssRules = parseSheet(text);
      if (live) {
        liveReplaceCount++;
        syncEffectiveValues();
      } else {
        scratchReplaceCount++;
      }
    }
  }

  const realm = { CSSStyleSheet: FakeCSSStyleSheet, document };
  document.defaultView = realm;
  documentElement = {
    nodeType: 1,
    isConnected: true,
    ownerDocument: document,
    getRootNode: () => document,
    style: inlineStyle,
  };
  runtimeDocuments.add(document);
  runtimeElements.add(documentElement);
  document.documentElement = documentElement;
  Object.defineProperty(document, "adoptedStyleSheets", {
    get() { return adoptedStyleSheets; },
    set(next) {
      adoptedStyleSheets = Array.from(next);
      syncEffectiveValues();
    },
  });

  return {
    document,
    target: documentElement,
    values,
    get constructedSheetCount() { return constructedSheetCount; },
    get scratchReplaceCount() { return scratchReplaceCount; },
    get liveReplaceCount() { return liveReplaceCount; },
  };
};

const ownedVar = "--lab-token-7f3a";
const assertPublished = (host, label) => {
  assert.equal(host.document.documentElement, host.target, label + " root identity");
  assert.equal(typeof host.values.get(ownedVar), "string", label + " effective value");
  assert.equal(host.document.adoptedStyleSheets.length, 1, label + " live sheet");
  assert.equal(host.liveReplaceCount, 1, label + " one live publication");
  assert.ok(host.scratchReplaceCount > 0, label + " detached scratch validation");
  assert.ok(host.constructedSheetCount > 1, label + " scratch stays distinct from live sheet");
};
const assertDisposed = (host, label) => {
  assert.equal(host.values.has(ownedVar), false, label + " effective value revoked");
  assert.equal(host.document.adoptedStyleSheets.length, 0, label + " no residual sheet");
};

const appliedHost = runtimeDocument();
const appliedTarget = appliedHost.document.documentElement;
const applied = applyTheme(appliedTarget, resolved);
assertPublished(appliedHost, "applyTheme");
assert.equal(applyTheme(appliedTarget, resolved), applied);
assertPublished(appliedHost, "identical applyTheme");
applied.dispose();
assertDisposed(appliedHost, "applyTheme dispose");
applied.dispose();
assertDisposed(appliedHost, "applyTheme repeated dispose");

const subpathAppliedHost = runtimeDocument();
const subpathAppliedTarget = subpathAppliedHost.document.documentElement;
const subpathApplied = applyThemeFromSubpath(subpathAppliedTarget, resolved);
assertPublished(subpathAppliedHost, "applyTheme subpath");
assert.equal(applyThemeFromSubpath(subpathAppliedTarget, resolved), subpathApplied);
assertPublished(subpathAppliedHost, "identical applyTheme subpath");
subpathApplied.dispose();
assertDisposed(subpathAppliedHost, "applyTheme subpath dispose");
subpathApplied.dispose();
assertDisposed(subpathAppliedHost, "applyTheme subpath repeated dispose");

const watchedHost = runtimeDocument();
const watchedTarget = watchedHost.document.documentElement;
const watcher = watchTheme(watchedTarget, {
  colors: engine,
  theme: "light",
  background,
  observe: false,
  win: {},
});
assert.equal(watcher.background(), background);
assertPublished(watchedHost, "watchTheme");
watcher.refresh();
assertPublished(watchedHost, "identical watchTheme refresh");
watcher.dispose();
assertDisposed(watchedHost, "watchTheme dispose");
watcher.dispose();
assertDisposed(watchedHost, "watchTheme repeated dispose");

const subpathWatchedHost = runtimeDocument();
const subpathWatchedTarget = subpathWatchedHost.document.documentElement;
const subpathWatcher = watchThemeFromSubpath(subpathWatchedTarget, {
  colors: engine,
  theme: "light",
  background,
  observe: false,
  win: {},
});
assert.equal(subpathWatcher.background(), background);
assertPublished(subpathWatchedHost, "watchTheme subpath");
subpathWatcher.refresh();
assertPublished(subpathWatchedHost, "watchTheme subpath refresh");
subpathWatcher.dispose();
assertDisposed(subpathWatchedHost, "watchTheme subpath dispose");
subpathWatcher.dispose();
assertDisposed(subpathWatchedHost, "watchTheme subpath repeated dispose");

const adaptedHost = runtimeDocument();
const adaptedTarget = adaptedHost.document.documentElement;
const adaptive = adaptTheme(adaptedTarget, {
  colors: engine,
  theme: "light",
  background,
  target: adaptedTarget,
  now: () => 0,
  win: {},
});
assertPublished(adaptedHost, "adaptTheme");
adaptive.tick(0);
assertPublished(adaptedHost, "identical adaptTheme tick");
assert.equal(typeof adaptive.current()["--lab-token-7f3a"], "string");
adaptive.dispose();
assertDisposed(adaptedHost, "adaptTheme dispose");
adaptive.dispose();
assertDisposed(adaptedHost, "adaptTheme repeated dispose");

const subpathAdaptedHost = runtimeDocument();
const subpathAdaptedTarget = subpathAdaptedHost.document.documentElement;
const subpathAdaptive = adaptThemeFromSubpath(subpathAdaptedTarget, {
  colors: engine,
  theme: "light",
  background,
  target: subpathAdaptedTarget,
  now: () => 0,
  win: {},
});
assertPublished(subpathAdaptedHost, "adaptTheme subpath");
subpathAdaptive.tick(0);
assertPublished(subpathAdaptedHost, "adaptTheme subpath tick");
assert.equal(typeof subpathAdaptive.current()[ownedVar], "string");
subpathAdaptive.dispose();
assertDisposed(subpathAdaptedHost, "adaptTheme subpath dispose");
subpathAdaptive.dispose();
assertDisposed(subpathAdaptedHost, "adaptTheme subpath repeated dispose");

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

export function typeSmokeSource() {
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
  type AdaptController as RootAdaptController,
  type ApplyThemeAttachment as RootApplyThemeAttachment,
  type OutputBindingSet,
  type ResolvedTheme,
  type ThemeConfig,
  type TranslucentRole,
  type WatchController as RootWatchController,
  type Wcag22AssessmentV1,
  type Wcag22CriterionV1,
} from "@labpics/colors";
import {
  applyTheme,
  type ApplyThemeAttachment as SubpathApplyThemeAttachment,
} from "@labpics/colors/apply-theme";
import {
  watchTheme,
  type WatchController as SubpathWatchController,
  type WatchThemeOptions,
} from "@labpics/colors/watch-theme";
import {
  adaptTheme,
  type AdaptController as SubpathAdaptController,
  type AdaptThemeOptions,
} from "@labpics/colors/adapt-theme";

const initialise: typeof init = init;
const apply: typeof applyTheme = applyTheme;
const watch: typeof watchTheme = watchTheme;
const adapt: typeof adaptTheme = adaptTheme;
type PublicSubpathTypes =
  | SubpathApplyThemeAttachment
  | SubpathWatchController
  | WatchThemeOptions
  | SubpathAdaptController
  | AdaptThemeOptions;
declare const publicSubpathType: PublicSubpathTypes;
void [apply, watch, adapt, publicSubpathType];
declare const rootAttachment: RootApplyThemeAttachment;
declare const subpathAttachment: SubpathApplyThemeAttachment;
const attachmentFromRoot: SubpathApplyThemeAttachment = rootAttachment;
const attachmentFromSubpath: RootApplyThemeAttachment = subpathAttachment;
declare const rootWatchController: RootWatchController;
declare const subpathWatchController: SubpathWatchController;
const watchControllerFromRoot: SubpathWatchController = rootWatchController;
const watchControllerFromSubpath: RootWatchController = subpathWatchController;
declare const rootAdaptController: RootAdaptController;
declare const subpathAdaptController: SubpathAdaptController;
const adaptControllerFromRoot: SubpathAdaptController = rootAdaptController;
const adaptControllerFromSubpath: RootAdaptController = subpathAdaptController;
for (const disposable of [
  attachmentFromRoot,
  attachmentFromSubpath,
  watchControllerFromRoot,
  watchControllerFromSubpath,
  adaptControllerFromRoot,
  adaptControllerFromSubpath,
]) {
  disposable.dispose();
  disposable[Symbol.dispose]?.();
}
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
const outputBindings: OutputBindingSet = resolved.outputBindings;
const firstOutputBinding: string | undefined = outputBindings[0];
// @ts-expect-error OutputBindingSet is immutable at the consumer boundary.
outputBindings.push("--lab-illegal");
declare const documentElement: HTMLElement;
const appliedAttachment: SubpathApplyThemeAttachment = applyTheme(documentElement, resolved);
appliedAttachment.dispose();
appliedAttachment[Symbol.dispose]?.();
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
  firstOutputBinding,
  appliedAttachment,
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
  tarballBytes,
  packageJson,
  typescriptCompilers,
  expectedBuildMetadata,
  expectedPrivateProgramMetadata,
  expectedPrivateProgramArtifacts,
  expectedNumericalArtifacts,
) {
  const consumer = await mkdtemp(join(tmpdir(), "labcolors-release-consumer-"));
  try {
    const tarballPath = resolve(consumer, "verified-package.tgz");
    await writeFile(tarballPath, tarballBytes, { flag: "wx", mode: 0o600 });
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
    const installedPrivateProgramMetadata = await readJson(
      resolve(installed, PRIVATE_PROGRAM_METADATA_PATH),
    );
    if (!isDeepStrictEqual(installedPrivateProgramMetadata, expectedPrivateProgramMetadata)) {
      fail("clean-installed private Program metadata differs from the verified release inputs");
    }
    for (const [name, expected] of Object.entries(expectedPrivateProgramArtifacts)) {
      const installedBytes = await readFile(resolve(installed, expected.path));
      if (
        expected.bytes !== installedBytes.length ||
        expected.sha256 !== sha256(installedBytes)
      ) {
        fail(`clean-installed private Program ${name} differs from the packed release input`);
      }
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
  const expectedSource = verifiedSourceSha();
  // The canonical build runs in the shared hermetic child boundary so ambient
  // caller-workflow executor overrides cannot influence it; the strict direct
  // validator is unchanged and the child's failure propagates loudly.
  runCanonicalPrivateProgramBuild();
  const { sourceSha: source, privateProgramBuild } = await prepareNpmPackage();
  if (source !== expectedSource) {
    fail("release source HEAD changed while the private Program was built");
  }
  python(["scripts/verify_wcag22_q55.py"], REPO_ROOT);
  python(["scripts/verify_point_support_surplus.py"], REPO_ROOT);
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
    validateRuntimeWasmIsolation(bytes);
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
  const privateProgramWasmBytes = await readFile(PRIVATE_PROGRAM_WASM);
  if (
    privateProgramWasmBytes.length < 8 ||
    !privateProgramWasmBytes.subarray(0, 4).equals(Buffer.from([0, 97, 115, 109]))
  ) {
    fail(`${PRIVATE_PROGRAM_WASM_PATH} is absent or has no WebAssembly magic header`);
  }
  const privateProgramArtifacts = {
    consumer: await hashedArtifact(
      PRIVATE_PROGRAM_CONSUMER,
      PRIVATE_PROGRAM_CONSUMER_PATH,
    ),
    wasm: await hashedArtifact(PRIVATE_PROGRAM_WASM, PRIVATE_PROGRAM_WASM_PATH),
  };
  const privateProgramMetadataValue = await readJson(PRIVATE_PROGRAM_METADATA);
  validatePrivateProgramMetadata(privateProgramMetadataValue, {
    packageJson,
    source,
    coreVersion,
    privateProgramBuild,
    artifacts: privateProgramArtifacts,
  });
  const privateProgramBuildMetadata = await hashedArtifact(
    PRIVATE_PROGRAM_METADATA,
    PRIVATE_PROGRAM_METADATA_PATH,
  );

  await rm(RELEASE_DIR, { recursive: true, force: true });
  await mkdir(RELEASE_DIR, { recursive: true });

  const canonicalPack = await packInto(RELEASE_DIR, packageJson);
  const reproductionDir = await mkdtemp(join(tmpdir(), "labcolors-release-reproduction-"));
  try {
    const reproducedPack = await packInto(reproductionDir, packageJson);
    if (canonicalPack.tarballName !== reproducedPack.tarballName) {
      fail(
        `same-executor npm pack passes produced different filenames: ` +
          `${canonicalPack.tarballName} != ${reproducedPack.tarballName}`,
      );
    }
    if (!canonicalPack.bytes.equals(reproducedPack.bytes)) {
      fail("same-executor npm pack passes produced different tarball bytes");
    }
  } finally {
    await rm(reproductionDir, { recursive: true, force: true });
  }

  await validatePackedNumericalEvidence(canonicalPack.bytes, numericalEvidenceArtifacts);

  await verifyCleanConsumer(
    canonicalPack.bytes,
    packageJson,
    typescriptCompilers,
    buildMetadataValue,
    privateProgramMetadataValue,
    privateProgramArtifacts,
    numericalEvidenceArtifacts,
  );

  const verifiedTarball = await materializeVerifiedTarballSnapshot(canonicalPack);
  const tarball = {
    path: `.release/${basename(verifiedTarball.path)}`,
    bytes: verifiedTarball.bytes.length,
    sha256: verifiedTarball.sha256,
  };
  const manifest = {
    // V5 joins every packed private Program byte to publish provenance without
    // changing the public runtime build-metadata contract.
    schemaVersion: 5,
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
      method: "same-executor-two-pass-npm-pack",
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
    ],
    artifacts: {
      tarball,
      wasm: [
        { role: "runtime", ...wasm.runtime },
        { role: PRIVATE_PROGRAM_ROLE, ...privateProgramArtifacts.wasm },
      ],
      buildMetadata,
      privateProgramConsumer: {
        role: PRIVATE_PROGRAM_ROLE,
        buildMetadata: privateProgramBuildMetadata,
        consumer: privateProgramArtifacts.consumer,
      },
    },
  };
  try {
    if (
      manifest.artifacts.tarball.path !== `.release/${verifiedTarball.sha256}.tgz` ||
      manifest.artifacts.tarball.bytes !== verifiedTarball.bytes.length ||
      manifest.artifacts.tarball.sha256 !== verifiedTarball.sha256
    ) {
      fail("release manifest does not exactly bind the verified tarball snapshot");
    }
    await atomicWriteGeneratedFile(RELEASE_MANIFEST, `${JSON.stringify(manifest, null, 2)}\n`);
  } catch (error) {
    await rm(dirname(verifiedTarball.path), { recursive: true, force: true });
    throw error;
  }

  return { manifest: RELEASE_MANIFEST, tarball: verifiedTarball.path };
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
