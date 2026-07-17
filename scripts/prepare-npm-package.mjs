import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmod, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { workspaceVersion } from "./cargo-workspace.mjs";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = resolve(SCRIPT_DIR, "..");
export const PACKAGE_DIR = resolve(REPO_ROOT, "packages/colors");

const SOURCE_LICENSE = resolve(REPO_ROOT, "LICENSE");
const PACKED_LICENSE = resolve(PACKAGE_DIR, "LICENSE");
const BUILD_METADATA = resolve(PACKAGE_DIR, "build-metadata.json");
const WCAG22_CONTRACT_DIR = resolve(REPO_ROOT, "crates/labcolors-core/contracts");
const PACKED_WCAG22_EVIDENCE_DIR = resolve(PACKAGE_DIR, "evidence");
const WCAG22_EVIDENCE_FILES = [
  "wcag22-srgb8-v1.json",
  "wcag22-srgb8-q55-v1.bin",
  "wcag22-srgb8-q55-proof-v1.json",
];
const CONFORMANCE_DIR = resolve(REPO_ROOT, "conformance/vectors");
// Полный состав пака 8.0.0: семь семейств. В npm-тарболл эти файлы НЕ
// копируются — байты хешируются из репозитория в build-metadata provenance
// (packDigest/familySetSha256); публикуемая поверхность это код адаптеров.
const CONFORMANCE_FILES = [
  "contrasts.json",
  "ladders.json",
  "alpha.json",
  "solve.json",
  "muddiness.json",
  "wcag22.json",
  "wcag22-feasibility.json",
];

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function git(args) {
  return execFileSync("git", args, {
    cwd: REPO_ROOT,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

/**
 * Bind generated packing inputs to one exact, clean checkout.
 *
 * This guard lives in the prepack helper itself so `npm pack` and the release
 * verifier cannot diverge: neither may write plausible build metadata for a dirty
 * tree or for a tag SHA different from the checked-out commit.
 */
export function verifiedSourceSha() {
  const head = git(["rev-parse", "HEAD"]).toLowerCase();
  const candidate = (process.env.GITHUB_SHA?.trim() || head).toLowerCase();
  if (!/^[0-9a-f]{40}(?:[0-9a-f]{24})?$/u.test(candidate)) {
    throw new Error(`source SHA is not a full Git object id: ${candidate}`);
  }
  if (candidate !== head) {
    throw new Error(`GITHUB_SHA ${candidate} does not equal checked-out HEAD ${head}`);
  }
  const changes = git(["status", "--porcelain=v1", "--untracked-files=normal"]);
  if (changes) {
    throw new Error(`release source is dirty and cannot attest HEAD:\n${changes}`);
  }
  return head;
}

async function atomicWrite(path, bytes) {
  const temporary = `${path}.tmp-${process.pid}`;
  try {
    await writeFile(temporary, bytes, { mode: 0o644 });
    await chmod(temporary, 0o644);
    await rename(temporary, path);
  } finally {
    await rm(temporary, { force: true });
  }
}

/**
 * Copy the repository's canonical licence into the npm package atomically.
 *
 * `packages/colors/LICENSE` is a generated packing input, not a second source of
 * truth. The operation deliberately fails when the root licence is absent or
 * empty: publishing an unlicensed tarball is not a recoverable condition.
 */
export async function prepareNpmPackage() {
  // Must precede even generated/ignored writes. A rejected call leaves no new
  // metadata that could later be packed as if it described the current source.
  const sourceSha = verifiedSourceSha();
  const canonical = await readFile(SOURCE_LICENSE);
  if (canonical.length === 0) {
    throw new Error(`canonical licence is empty: ${SOURCE_LICENSE}`);
  }

  await mkdir(PACKAGE_DIR, { recursive: true });
  await atomicWrite(PACKED_LICENSE, canonical);

  const copied = await readFile(PACKED_LICENSE);
  if (!copied.equals(canonical)) {
    throw new Error("generated npm LICENSE differs from the canonical root LICENSE");
  }

  await mkdir(PACKED_WCAG22_EVIDENCE_DIR, { recursive: true });
  for (const file of WCAG22_EVIDENCE_FILES) {
    const source = await readFile(resolve(WCAG22_CONTRACT_DIR, file));
    if (source.length === 0) throw new Error(`canonical WCAG22 evidence is empty: ${file}`);
    const destination = resolve(PACKED_WCAG22_EVIDENCE_DIR, file);
    await atomicWrite(destination, source);
    if (!(await readFile(destination)).equals(source)) {
      throw new Error(`packed WCAG22 evidence differs from canonical source: ${file}`);
    }
  }

  const [
    packageJsonSource,
    cargoSource,
    conformanceSource,
    runtimeWasm,
    compilerWasm,
    ...familyBytes
  ] =
    await Promise.all([
      readFile(resolve(PACKAGE_DIR, "package.json"), "utf8"),
      readFile(resolve(REPO_ROOT, "Cargo.toml"), "utf8"),
      readFile(resolve(CONFORMANCE_DIR, "manifest.json"), "utf8"),
      readFile(resolve(PACKAGE_DIR, "pkg/labcolors_bg.wasm")),
      readFile(resolve(PACKAGE_DIR, "compiler/labcolors_compiler_bg.wasm")),
      ...CONFORMANCE_FILES.map((file) => readFile(resolve(CONFORMANCE_DIR, file))),
    ]);
  const packageJson = JSON.parse(packageJsonSource);
  const conformance = JSON.parse(conformanceSource);
  const coreVersion = workspaceVersion(cargoSource);
  const metadata = {
    schemaVersion: 2,
    package: { name: packageJson.name, version: packageJson.version },
    sourceSha,
    coreVersion,
    conformance: {
      packVersion: conformance.packVersion,
      packDigest: conformance.packDigest,
      manifestSha256: sha256(Buffer.from(conformanceSource)),
      familySetSha256: sha256(Buffer.concat(familyBytes)),
    },
    wasm: [
      {
        role: "runtime",
        path: "pkg/labcolors_bg.wasm",
        bytes: runtimeWasm.length,
        sha256: sha256(runtimeWasm),
      },
      {
        role: "compiler",
        path: "compiler/labcolors_compiler_bg.wasm",
        bytes: compilerWasm.length,
        sha256: sha256(compilerWasm),
      },
    ],
  };
  await atomicWrite(BUILD_METADATA, `${JSON.stringify(metadata, null, 2)}\n`);

  return { license: PACKED_LICENSE, buildMetadata: BUILD_METADATA, sourceSha };
}

const invokedDirectly =
  process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  prepareNpmPackage().catch((error) => {
    console.error(error instanceof Error ? error.stack : String(error));
    process.exitCode = 1;
  });
}
