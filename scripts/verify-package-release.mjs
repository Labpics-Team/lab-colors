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
const PACKED_WCAG22_EVIDENCE_DIR = resolve(PACKAGE_DIR, "evidence");
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

async function validateWcag22Evidence() {
  const artifacts = [];
  for (const file of WCAG22_EVIDENCE_FILES) {
    const canonical = await readFile(resolve(WCAG22_CONTRACT_DIR, file));
    const packedPath = resolve(PACKED_WCAG22_EVIDENCE_DIR, file);
    const packed = await readFile(packedPath);
    if (!packed.equals(canonical)) {
      fail(`packed WCAG22 evidence differs from canonical source: ${file}`);
    }
    artifacts.push(await hashedArtifact(packedPath, `evidence/${file}`));
  }

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
  if (!/^[0-9a-f]{64}$/u.test(proof.crate_lib_source_sha256 ?? "")) {
    fail("WCAG22 proof lacks the proof-bound crate-root digest");
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

async function validateConformance(conformance) {
  if (conformance.packVersion !== "4.0.0") {
    fail(`release requires conformance pack 4.0.0, got ${conformance.packVersion}`);
  }
  if (!/^[0-9a-f]{8}$/u.test(conformance.packDigest ?? "")) {
    fail(`invalid conformance packDigest: ${conformance.packDigest}`);
  }

  const familyBuffers = await Promise.all(
    CONFORMANCE_FAMILY_FILES.map((name) => readFile(resolve(CONFORMANCE_DIR, name))),
  );
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
  const countKeys = ["contrasts", "ladders", "alpha", "solve", "muddiness", "wcag22"];
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
  const proofPath = resolve(WCAG22_CONTRACT_DIR, "wcag22-srgb8-q55-proof-v1.json");
  const proofBytes = await readFile(proofPath);
  const proof = await readJson(proofPath);
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

function runtimeSmokeSource() {
  return String.raw`
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

import init, {
  LabColors,
  evaluateWcag22,
  numericalCapabilityManifest,
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
  numericalCapabilityManifest,
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
} from "@labpics/colors";

const initialise: typeof init = init;
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

    const installedWasm = await readFile(resolve(installed, "pkg/labcolors_bg.wasm"));
    if (sha256(installedWasm) !== expectedBuildMetadata.wasm.sha256) {
      fail("clean-installed WASM bytes differ from the packed release input");
    }
    const installedBuildMetadata = await readJson(resolve(installed, "build-metadata.json"));
    if (!isDeepStrictEqual(installedBuildMetadata, expectedBuildMetadata)) {
      fail("clean-installed build metadata differs from the verified release inputs");
    }

    const runtimePath = resolve(consumer, "smoke.mjs");
    const typesPath = resolve(consumer, "smoke.ts");
    await writeFile(runtimePath, runtimeSmokeSource());
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
        "ES2022,DOM,ESNext.Disposable",
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

  const tarball = await hashedArtifact(
    canonicalPack.path,
    `.release/${canonicalPack.tarballName}`,
  );
  await verifyCleanConsumer(
    canonicalPack.path,
    packageJson,
    packageLock,
    buildMetadataValue,
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
        libraries: ["ES2022", "DOM", "ESNext.Disposable"],
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
