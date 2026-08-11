import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdir, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { atomicWriteGeneratedFile } from "./atomic-write.mjs";
import { workspaceVersion } from "./cargo-workspace.mjs";
import {
  PRIVATE_PROGRAM_CONSUMER_PATH,
  PRIVATE_PROGRAM_METADATA_PATH,
  PRIVATE_PROGRAM_ROLE,
  PRIVATE_PROGRAM_WASM_PATH,
  artifactMetadata,
  assertWasm,
  readPrivateProgramBuildReceipt,
} from "./build-private-program.mjs";
import {
  NUMERICAL_EVIDENCE_FILES,
  assertPackageEvidenceInventory,
} from "./release-evidence.mjs";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = resolve(SCRIPT_DIR, "..");
export const PACKAGE_DIR = resolve(REPO_ROOT, "packages/colors");

const SOURCE_LICENSE = resolve(REPO_ROOT, "LICENSE");
const PACKED_LICENSE = resolve(PACKAGE_DIR, "LICENSE");
const BUILD_METADATA = resolve(PACKAGE_DIR, "build-metadata.json");
const PRIVATE_PROGRAM_METADATA = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_METADATA_PATH);
const PRIVATE_PROGRAM_CONSUMER = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_CONSUMER_PATH);
const PRIVATE_PROGRAM_WASM = resolve(PACKAGE_DIR, PRIVATE_PROGRAM_WASM_PATH);
const NUMERICAL_CONTRACT_DIR = resolve(REPO_ROOT, "crates/labcolors-core/contracts");
const PACKED_NUMERICAL_EVIDENCE_DIR = resolve(PACKAGE_DIR, "evidence");
const CONFORMANCE_DIR = resolve(REPO_ROOT, "conformance/vectors");
// Полный состав пака 10.0.0: пять семейств. В npm-тарболл эти файлы НЕ
// копируются — байты хешируются из репозитория в build-metadata provenance
// (packDigest/familySetSha256); публикуемая поверхность это код адаптеров.
const CONFORMANCE_FILES = [
  "contrasts.json",
  "ladders.json",
  "alpha.json",
  "solve.json",
  "wcag22.json",
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
  const [canonical, packageJsonSource] = await Promise.all([
    readFile(SOURCE_LICENSE),
    readFile(resolve(PACKAGE_DIR, "package.json"), "utf8"),
  ]);
  const packageJson = JSON.parse(packageJsonSource);
  assertPackageEvidenceInventory(packageJson.files);
  const privateProgramBuild = await readPrivateProgramBuildReceipt({
    requireOptimizer: true,
  });
  if (canonical.length === 0) {
    throw new Error(`canonical licence is empty: ${SOURCE_LICENSE}`);
  }

  await mkdir(PACKAGE_DIR, { recursive: true });
  await atomicWriteGeneratedFile(PACKED_LICENSE, canonical);

  const copied = await readFile(PACKED_LICENSE);
  if (!copied.equals(canonical)) {
    throw new Error("generated npm LICENSE differs from the canonical root LICENSE");
  }

  await mkdir(PACKED_NUMERICAL_EVIDENCE_DIR, { recursive: true });
  for (const file of NUMERICAL_EVIDENCE_FILES) {
    const source = await readFile(resolve(NUMERICAL_CONTRACT_DIR, file));
    if (source.length === 0) throw new Error(`canonical numerical evidence is empty: ${file}`);
    const destination = resolve(PACKED_NUMERICAL_EVIDENCE_DIR, file);
    await atomicWriteGeneratedFile(destination, source);
    if (!(await readFile(destination)).equals(source)) {
      throw new Error(`packed numerical evidence differs from canonical source: ${file}`);
    }
  }

  const [
    cargoSource,
    conformanceSource,
    runtimeWasm,
    privateProgramConsumer,
    privateProgramWasm,
    ...familyBytes
  ] =
    await Promise.all([
      readFile(resolve(REPO_ROOT, "Cargo.toml"), "utf8"),
      readFile(resolve(CONFORMANCE_DIR, "manifest.json"), "utf8"),
      readFile(resolve(PACKAGE_DIR, "pkg/labcolors_bg.wasm")),
      readFile(PRIVATE_PROGRAM_CONSUMER),
      readFile(PRIVATE_PROGRAM_WASM),
      ...CONFORMANCE_FILES.map((file) => readFile(resolve(CONFORMANCE_DIR, file))),
    ]);
  assertWasm(runtimeWasm, "pkg/labcolors_bg.wasm");
  assertWasm(privateProgramWasm, PRIVATE_PROGRAM_WASM_PATH);
  const privateProgramWasmArtifact = artifactMetadata(
    PRIVATE_PROGRAM_WASM_PATH,
    privateProgramWasm,
  );
  if (
    privateProgramWasmArtifact.bytes !== privateProgramBuild.artifact.bytes ||
    privateProgramWasmArtifact.sha256 !== privateProgramBuild.artifact.sha256
  ) {
    throw new Error("private Program WASM changed after its canonical build receipt");
  }
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
    ],
  };
  await atomicWriteGeneratedFile(BUILD_METADATA, `${JSON.stringify(metadata, null, 2)}\n`);

  const privateProgramMetadata = {
    schemaVersion: 1,
    role: PRIVATE_PROGRAM_ROLE,
    package: { name: packageJson.name, version: packageJson.version },
    source: {
      gitSha: sourceSha,
      core: {
        crate: privateProgramBuild.build.crate,
        version: coreVersion,
        digest: privateProgramBuild.source,
      },
    },
    build: privateProgramBuild.build,
    artifacts: {
      consumer: artifactMetadata(PRIVATE_PROGRAM_CONSUMER_PATH, privateProgramConsumer),
      wasm: privateProgramWasmArtifact,
    },
  };
  await mkdir(dirname(PRIVATE_PROGRAM_METADATA), { recursive: true });
  await atomicWriteGeneratedFile(
    PRIVATE_PROGRAM_METADATA,
    `${JSON.stringify(privateProgramMetadata, null, 2)}\n`,
  );
  if (verifiedSourceSha() !== sourceSha) {
    throw new Error("release source changed while npm packing inputs were prepared");
  }

  return {
    license: PACKED_LICENSE,
    buildMetadata: BUILD_METADATA,
    privateProgramMetadata: PRIVATE_PROGRAM_METADATA,
    privateProgramBuild,
    sourceSha,
  };
}

const invokedDirectly =
  process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  prepareNpmPackage().catch((error) => {
    console.error(error instanceof Error ? error.stack : String(error));
    process.exitCode = 1;
  });
}
