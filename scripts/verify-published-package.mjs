import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { setTimeout as pause } from "node:timers/promises";
import { isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { smokePackedPackage } from "./verify-package-release.mjs";

const REGISTRY = "https://registry.npmjs.org";
const REPOSITORY = "https://github.com/Labpics-Team/lab-colors";
const SLSA_TYPE = "https://slsa.dev/provenance/v1";
const MAX_METADATA_BYTES = 1024 * 1024;
const MAX_ARCHIVE_BYTES = 16 * 1024 * 1024;
const MAX_ATTESTATION_BYTES = 4 * 1024 * 1024;
const hash = (algorithm, bytes, encoding = "hex") =>
  createHash(algorithm).update(bytes).digest(encoding);
const fail = (message) => { throw new Error(`Published package: ${message}`); };

class RegistryHttpError extends Error {
  constructor(status, url) {
    super(`Published package: HTTP ${status} for ${url}`);
    this.status = status;
  }
}

const transientStatuses = new Set([404, 408, 425, 429, 500, 502, 503, 504]);
const transientCodes = new Set(["ECONNRESET", "ETIMEDOUT", "EAI_AGAIN", "UND_ERR_CONNECT_TIMEOUT", "UND_ERR_SOCKET"]);
const temporary = (error) =>
  (error instanceof RegistryHttpError && transientStatuses.has(error.status)) ||
  error?.name === "TimeoutError" || transientCodes.has(error?.cause?.code ?? error?.code);

async function readBounded(url, limit, fetcher) {
  const response = await fetcher(url, {
    redirect: "error",
    signal: AbortSignal.timeout(15_000),
  });
  if (!response.ok) {
    await response.body?.cancel().catch(() => {});
    throw new RegistryHttpError(response.status, url);
  }
  const declared = response.headers.get("content-length");
  if (declared !== null && (!/^\d+$/u.test(declared) || Number(declared) > limit)) {
    await response.body?.cancel().catch(() => {});
    fail("response exceeds its byte limit");
  }
  if (!response.body) fail("response has no body");
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) fail("response exceeds its byte limit");
      chunks.push(Buffer.from(value));
    }
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
  return Buffer.concat(chunks, size);
}

function exactKeys(value, keys) {
  return value !== null && typeof value === "object" && !Array.isArray(value) &&
    Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

// npm 11.9 generates these fields from the caller workflow and GITHUB_SHA.
// This policy reads the very envelope later supplied to the signature verifier.
function selectProvenance(attestations, { name, version, gitSha }, bytes) {
  const matches = Array.isArray(attestations?.attestations)
    ? attestations.attestations.filter((row) => row?.predicateType === SLSA_TYPE) : [];
  if (matches.length !== 1) fail("expected exactly one SLSA provenance attestation");
  const bundle = matches[0].bundle;
  const envelope = bundle?.dsseEnvelope;
  const encoded = envelope?.payload;
  if (envelope?.payloadType !== "application/vnd.in-toto+json" || typeof encoded !== "string" ||
      !encoded || Buffer.from(encoded, "base64").toString("base64") !== encoded) {
    fail("SLSA envelope payload is invalid");
  }
  const statement = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.from(encoded, "base64")));
  if (statement?._type !== "https://in-toto.io/Statement/v1") fail("provenance statement type differs");
  if (statement.predicateType !== SLSA_TYPE) fail("signed provenance predicate type differs");
  const subject = statement.subject;
  const packagePurl = `pkg:npm/${name.split("/").map(encodeURIComponent).join("/")}@${version}`;
  if (!Array.isArray(subject) || subject.length !== 1 || !exactKeys(subject[0], ["name", "digest"]) ||
      subject[0].name !== packagePurl) {
    fail("signed provenance subject differs from the package");
  }
  if (!exactKeys(subject[0].digest, ["sha512"]) || subject[0].digest.sha512 !== hash("sha512", bytes)) {
    fail("signed provenance digest differs from the archive");
  }
  const definition = statement.predicate?.buildDefinition;
  if (definition?.buildType !== "https://slsa-framework.github.io/github-actions-buildtypes/workflow/v1") {
    fail("signed provenance build type differs");
  }
  const tag = `refs/tags/colors-v${version}`;
  const workflow = definition.externalParameters?.workflow;
  if (!exactKeys(workflow, ["ref", "repository", "path"]) || workflow.ref !== tag ||
      workflow.repository !== REPOSITORY || workflow.path !== ".github/workflows/publish.yml") {
    fail("signed provenance workflow differs from the release");
  }
  if (definition.internalParameters?.github?.event_name !== "push") fail("signed provenance trigger differs");
  const dependencies = definition.resolvedDependencies;
  if (!Array.isArray(dependencies) || dependencies.length !== 1 ||
      !exactKeys(dependencies[0], ["uri", "digest"]) ||
      dependencies[0].uri !== `git+${REPOSITORY}@${tag}` ||
      !exactKeys(dependencies[0].digest, ["gitCommit"]) || dependencies[0].digest.gitCommit !== gitSha) {
    fail("signed provenance source differs from the checked-out release");
  }
  return bundle;
}

/** Один snapshot связывает проверенный архив и его подписанное provenance. */
async function downloadOnce(expected, fetcher) {
  const { name, version, sha256, gitSha } = expected;
  if (typeof name !== "string" || !name || typeof version !== "string" || !version ||
      !/^[0-9a-f]{64}$/u.test(sha256 ?? "") || !/^[0-9a-f]{40}$/u.test(gitSha ?? "")) {
    fail("expected package identity is incomplete");
  }
  const metadataUrl = `${REGISTRY}/${encodeURIComponent(name)}/${encodeURIComponent(version)}`;
  const metadata = JSON.parse((await readBounded(metadataUrl, MAX_METADATA_BYTES, fetcher)).toString("utf8"));
  if (metadata.name !== name || metadata.version !== version) fail("registry package identity differs");
  const tarball = new URL(metadata.dist?.tarball);
  if (tarball.origin !== REGISTRY || tarball.username || tarball.password || tarball.hash) {
    fail("archive URL is outside the public registry");
  }
  let attestationUrl;
  try {
    attestationUrl = new URL(metadata.dist?.attestations?.url);
    if (attestationUrl.origin !== REGISTRY || attestationUrl.username || attestationUrl.password ||
        attestationUrl.search || attestationUrl.hash ||
        decodeURIComponent(attestationUrl.pathname) !== `/-/npm/v1/attestations/${name}@${version}`) {
      fail("attestation URL differs from this public package version");
    }
  } catch {
    fail("attestation URL differs from this public package version");
  }
  const bytes = await readBounded(tarball.href, MAX_ARCHIVE_BYTES, fetcher);
  if (hash("sha256", bytes) !== sha256) fail("archive differs from the verified publication");
  if (metadata.dist.integrity !== `sha512-${hash("sha512", bytes, "base64")}`) {
    fail("registry integrity differs from the archive");
  }
  const attestations = JSON.parse((await readBounded(attestationUrl.href, MAX_ATTESTATION_BYTES, fetcher)).toString("utf8"));
  return { bytes, bundle: selectProvenance(attestations, expected, bytes) };
}

/** Повторяет только временный отказ чтения; неверные байты остаются ошибкой. */
export async function downloadPublishedPackage(expected, fetcher = fetch, wait = pause) {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await downloadOnce(expected, fetcher);
    } catch (error) {
      if (attempt === 2 || !temporary(error)) throw error;
      await wait(1000 * (2 ** attempt));
    }
  }
}

// npm scans newly published versions before making them public. Publication
// success is authoritative; temporary 404s on the readback path are not.
// Bound both wall-clock wait and attempt count. Identity/integrity refusals
// remain terminal and never cause another publish.
export async function waitForPublishedPackage(
  expected, fetcher = fetch, wait = pause, now = Date.now, budgetMs = 20 * 60_000,
) {
  if (!Number.isSafeInteger(budgetMs) || budgetMs < 0 || budgetMs > 20 * 60_000) {
    fail("invalid publication visibility budget");
  }
  const deadline = now() + budgetMs;
  for (let attempt = 0; attempt < 81; attempt += 1) {
    try {
      return await downloadOnce(expected, fetcher);
    } catch (error) {
      const remaining = deadline - now();
      if (!temporary(error) || remaining <= 0 || attempt === 80) throw error;
      await wait(Math.min(15_000, remaining));
    }
  }
  fail("publication visibility attempt limit exceeded");
}

export async function verifyPublishedPackage(expected, {
  fetcher = fetch, wait = pause, now = Date.now, budgetMs = 20 * 60_000,
  cosignBin = process.env.COSIGN_BIN, executeVerifier = execFileSync, smoke = smokePackedPackage,
} = {}) {
  if (typeof cosignBin !== "string" || !isAbsolute(cosignBin)) fail("verified COSIGN_BIN path is required");
  const { bytes, bundle } = await waitForPublishedPackage(expected, fetcher, wait, now, budgetMs);
  const root = await mkdtemp(join(tmpdir(), "colors-published-"));
  try {
    const tarball = join(root, "package.tgz");
    const provenance = join(root, "provenance.sigstore.json");
    await writeFile(tarball, bytes, { flag: "wx", mode: 0o600 });
    await writeFile(provenance, JSON.stringify(bundle), { flag: "wx", mode: 0o600 });
    // The caller pins cosign. Its official verifier checks the same envelope,
    // Fulcio identity, transparency evidence and the exact SHA-512 artifact.
    await executeVerifier(cosignBin, [
      "verify-blob-attestation", "--bundle", provenance,
      "--certificate-oidc-issuer", "https://token.actions.githubusercontent.com",
      "--certificate-identity", `${REPOSITORY}/.github/workflows/publish-worker.yml@refs/tags/colors-v${expected.version}`,
      "--type", SLSA_TYPE, tarball,
    ], {
      encoding: "utf8", timeout: 3 * 60_000, maxBuffer: MAX_METADATA_BYTES,
      stdio: ["ignore", "pipe", "pipe"], shell: false, windowsHide: true,
    });
    await smoke(tarball);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function main() {
  const source = JSON.parse(await readFile(new URL("../packages/colors/package.json", import.meta.url), "utf8"));
  const expected = {
    name: source.name, version: source.version,
    sha256: process.env.EXPECTED_TARBALL_SHA256, gitSha: process.env.GITHUB_SHA,
  };
  const checkedOut = execFileSync("git", ["--no-replace-objects", "rev-parse", "HEAD"], {
    cwd: fileURLToPath(new URL("..", import.meta.url)), encoding: "utf8",
    timeout: 15_000, stdio: ["ignore", "pipe", "pipe"], shell: false, windowsHide: true,
  }).trim();
  if (checkedOut !== expected.gitSha || source.repository?.url !== REPOSITORY ||
      process.env.GITHUB_REPOSITORY !== "Labpics-Team/lab-colors" ||
      process.env.GITHUB_REF !== `refs/tags/colors-v${source.version}`) {
    fail("checked-out source differs from the GitHub release identity");
  }
  await verifyPublishedPackage(expected, {
    wait: async (ms) => {
      console.log("Waiting for npm to expose the verified publication and provenance");
      await pause(ms);
    },
  });
  console.log(`Installed and verified ${source.name}@${source.version}: ${expected.sha256}, source ${expected.gitSha}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.stack : String(error));
    process.exitCode = 1;
  });
}
