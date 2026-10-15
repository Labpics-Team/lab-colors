import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { setTimeout as pause } from "node:timers/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { smokePackedPackage } from "./verify-package-release.mjs";

const REGISTRY = "https://registry.npmjs.org";
const MAX_METADATA_BYTES = 1024 * 1024;
const MAX_ARCHIVE_BYTES = 16 * 1024 * 1024;
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

/** Сверяет публичные байты с независимым результатом подготовительного задания. */
async function downloadOnce({ name, version, sha256 }, fetcher) {
  if (typeof name !== "string" || !name || typeof version !== "string" || !version ||
      !/^[0-9a-f]{64}$/u.test(sha256 ?? "")) {
    fail("expected package identity is incomplete");
  }
  const metadataUrl = `${REGISTRY}/${encodeURIComponent(name)}/${encodeURIComponent(version)}`;
  const metadata = JSON.parse((await readBounded(metadataUrl, MAX_METADATA_BYTES, fetcher)).toString("utf8"));
  if (metadata.name !== name || metadata.version !== version) fail("registry package identity differs");
  const tarball = new URL(metadata.dist?.tarball);
  if (tarball.origin !== REGISTRY || tarball.username || tarball.password || tarball.hash) {
    fail("archive URL is outside the public registry");
  }
  const bytes = await readBounded(tarball.href, MAX_ARCHIVE_BYTES, fetcher);
  if (hash("sha256", bytes) !== sha256) fail("archive differs from the verified publication");
  if (metadata.dist.integrity !== `sha512-${hash("sha512", bytes, "base64")}`) {
    fail("registry integrity differs from the archive");
  }
  return bytes;
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

async function main() {
  const source = JSON.parse(await readFile(new URL("../packages/colors/package.json", import.meta.url), "utf8"));
  const sha256 = process.env.EXPECTED_TARBALL_SHA256;
  const bytes = await downloadPublishedPackage({ name: source.name, version: source.version, sha256 });
  const root = await mkdtemp(join(tmpdir(), "colors-published-"));
  try {
    const tarball = join(root, "package.tgz");
    await writeFile(tarball, bytes, { flag: "wx", mode: 0o600 });
    await smokePackedPackage(tarball);
    console.log(`Installed and verified ${source.name}@${source.version}: ${sha256}`);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.stack : String(error));
    process.exitCode = 1;
  });
}
