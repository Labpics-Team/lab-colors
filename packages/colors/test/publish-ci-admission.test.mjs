import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import { gunzipSync, gzipSync } from "node:zlib";
import { repackPublicationTarball } from "../../../scripts/verify-publication.mjs";

const workflow = (name) => readFileSync(new URL("../../../.github/workflows/" + name, import.meta.url), "utf8");
const publisher = workflow("publish-worker.yml");
const sha = "1234567890abcdef1234567890abcdef12345678";
const repository = "Labpics-Team/lab-colors";

function stepScript(source, id) {
  const lines = source.split("\n");
  const anchors = lines.flatMap((line, index) => line.trim() === "id: " + id ? [index] : []);
  assert.equal(anchors.length, 1, "publication must execute one " + id + " admission");
  const start = lines.findIndex((line, index) => index > anchors[0] && /^\s+node <<'NODE'\s*$/u.test(line));
  assert.ok(start > anchors[0], id + " admission script is absent");
  const indent = lines[start].match(/^\s*/u)[0];
  const end = lines.findIndex((line, index) => index > start && line === indent + "NODE");
  assert.ok(end > start, id + " admission script is not terminated");
  return lines.slice(start + 1, end).map((line) => {
    assert.ok(line === "" || line.startsWith(indent), "malformed workflow script indentation");
    return line.slice(indent.length);
  }).join("\n");
}

// Fixture job names come from the producer, never from the publisher's allowlist.
// Only literal job-level names participate; matrix leaves are covered by their
// existing aggregate jobs. Unexpected naming syntax cannot silently shrink it.
function producerNames(files) {
  const names = files.flatMap((file) => [...workflow(file).matchAll(/^    name: (.+)$/gmu)]
    .map((match) => match[1].trim())
    .filter((name) => !name.includes("${{"))
    .map((name) => name.startsWith('"') ? JSON.parse(name) : name.startsWith("'") ? name.slice(1, -1) : name));
  assert.ok(names.length > 0, "producer fixture must contain actual jobs");
  return [...new Set(names)];
}

function evidence() {
  const nativeReferences = [...workflow("native-conformance.yml").matchAll(/^    uses: (\S+)$/gmu)];
  assert.deepEqual(nativeReferences.map((match) => match[1]),
    ["./.github/workflows/native-conformance-worker.yml"],
    "native caller must bind one worker from its own source revision");
  const ciNames = producerNames(["ci-worker.yml", "package-worker.yml"]);
  assert.ok(ciNames.includes("core tests and artifact contracts"), "healthy control lacks the Core aggregate");
  assert.ok(ciNames.includes("test"), "healthy control lacks the full verification aggregate");
  const nativeNames = producerNames(["native-conformance-worker.yml"]);
  const run = (id, file, references) => ({
    id, run_attempt: 1, path: ".github/workflows/" + file,
    head_sha: sha, head_branch: "main", event: "push",
    status: "completed", conclusion: "success", referenced_workflows: references,
  });
  const ci = run(101, "ci.yml", ["ci-worker.yml", "package-worker.yml"].map((file) => ({
    path: repository + "/.github/workflows/" + file + "@" + sha, sha,
  })));
  const native = run(202, "native-conformance.yml", [{
    path: repository + nativeReferences[0][1].slice(1) + "@" + sha, sha,
  }]);
  const jobs = (id, names) => names.map((name, index) => ({
    id: id * 100 + index, run_id: id, name: "caller / nested caller / " + name,
    status: "completed", conclusion: "success",
  }));
  return {
    runs: { "ci.yml": [ci], "native-conformance.yml": [native] },
    jobs: { 101: jobs(101, ciNames), 202: jobs(202, nativeNames) },
  };
}

async function admit(record, script = stepScript(publisher, "canonical-runs")) {
  const writes = [], errors = [], requests = [];
  const process = { env: {
    GITHUB_REPOSITORY: repository, GITHUB_SHA: sha,
    GITHUB_API_URL: "https://github-api.example.invalid", GITHUB_OUTPUT: "/fixture/output",
    GH_READ_TOKEN: "fixture-read-token",
  } };
  const fetch = async (input) => {
    const url = new URL(input);
    requests.push(url.href);
    assert.ok(requests.length <= 30, "admission must have bounded API reads");
    assert.equal(url.origin, process.env.GITHUB_API_URL);
    let payload;
    const runMatch = url.pathname.match(/\/actions\/workflows\/([^/]+)\/runs$/u);
    const jobMatch = url.pathname.match(/\/actions\/runs\/(\d+)\/jobs$/u);
    if (runMatch) {
      assert.equal(url.searchParams.get("head_sha"), sha);
      assert.equal(url.searchParams.get("branch"), "main");
      assert.equal(url.searchParams.get("event"), "push");
      payload = { workflow_runs: record.runs[decodeURIComponent(runMatch[1])] ?? [] };
    } else if (jobMatch) {
      payload = { jobs: record.jobs[jobMatch[1]] ?? [] };
    } else {
      throw new Error("unexpected API route: " + url.pathname);
    }
    return { ok: true, json: async () => structuredClone(payload) };
  };
  const require = (name) => {
    assert.equal(name, "node:fs");
    return { appendFileSync(path, value, encoding) {
      assert.equal(path, "/fixture/output");
      assert.equal(encoding, "utf8");
      writes.push(value);
    } };
  };
  await runInNewContext(script, {
    process, fetch, require, URLSearchParams,
    console: { log() {}, error(error) { errors.push(String(error)); } },
  }, { timeout: 1000, filename: "publish-canonical-runs.cjs" });
  return { writes, errors, exitCode: process.exitCode ?? 0, requests };
}

function assertRefused(result, reason) {
  assert.equal(result.exitCode, 1, result.errors.join("\n"));
  assert.deepEqual(result.writes, [], "a refused run must not export partial release authority");
  assert.match(result.errors.join("\n"), reason);
}

test("publication admits successful current producer jobs at arbitrary caller depth", async () => {
  const result = await admit(evidence());
  assert.equal(result.exitCode, 0, result.errors.join("\n"));
  assert.deepEqual(result.errors, []);
  assert.deepEqual(result.writes, ["ci_run_id=101\nci_run_attempt=1\nnative_run_id=202\n"]);
});

for (const condition of ["missing", "duplicate", "foreign-run", "failure", "skipped", "in-progress"]) {
  test("publication refuses " + condition + " verification aggregate without partial outputs", async () => {
    const record = evidence();
    const jobs = record.jobs[101];
    const index = jobs.findIndex((job) => job.name.endsWith(" / test"));
    assert.ok(index >= 0);
    if (condition === "missing") jobs.splice(index, 1);
    else if (condition === "duplicate") jobs.push({ ...jobs[index], id: 999 });
    else if (condition === "foreign-run") jobs[index].run_id = 999;
    else if (condition === "in-progress") Object.assign(jobs[index], { status: "in_progress", conclusion: null });
    else jobs[index].conclusion = condition;
    assertRefused(await admit(record), /test:/u);
  });
}

test("publication does not fall back to an old successful run after a failed rerun", async () => {
  const record = evidence();
  record.runs["ci.yml"].push({ ...record.runs["ci.yml"][0], id: 303, conclusion: "failure" });
  assertRefused(await admit(record), /303.*failure/u);
});

test("publication refuses workflow references from another source revision", async () => {
  for (const [file, index, reason] of [
    ["ci.yml", 1, /escaped the admitted source revision/u],
    ["native-conformance.yml", 0, /primary worker is missing or repeated/u],
  ]) {
    for (const coordinate of ["path", "sha", "both"]) {
      const record = evidence();
      const reference = record.runs[file][0].referenced_workflows[index];
      if (coordinate !== "sha") {
        reference.path = reference.path.replace(/@[0-9a-f]{40}$/u, "@" + "0".repeat(40));
      }
      if (coordinate !== "path") reference.sha = "0".repeat(40);
      assertRefused(await admit(record), reason);
    }
  }
});

test("publication rejects a missing native result after a successful CI run", async () => {
  const record = evidence();
  record.jobs[202] = [];
  assertRefused(await admit(record), /swift conformance/u);
});

test("publication inventory and exports match the source package contract", () => {
  const script = stepScript(publisher, "verified-artifact");
  const boundary = script.indexOf("(async () => {");
  assert.ok(boundary > 0, "artifact validator must expose its declaration before IO");
  const declared = runInNewContext(script.slice(0, boundary) +
    "\n({ files: EXPECTED_PACKAGE_FILES, exports: EXPECTED_PACKAGE_EXPORTS });", {
    require: createRequire(import.meta.url), Buffer,
    process: { env: { GITHUB_WORKSPACE: fileURLToPath(new URL("../../../", import.meta.url)) } },
  }, { timeout: 1000, filename: "publish-package-contract.cjs" });
  const actual = JSON.parse(JSON.stringify(declared));
  const source = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.deepEqual(actual.files, source.files, "publication must retain the source package inventory");
  assert.deepEqual(actual.exports, source.exports, "publication must retain the source public API");
});

test("publication repacking controls remain canonical and preserve zero-filled payloads", (t) => {
  const directory = mkdtempSync(join(tmpdir(), "colors-publication-repack-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const tarball = join(directory, "source.tgz");
  const repacked = join(directory, "repacked.tgz");
  const inventory = join(directory, "inventory.json");
  const inspector = fileURLToPath(new URL("../../../scripts/inspect-npm-tarball.py", import.meta.url));
  const env = { PATH: process.env.PATH };
  // Include full zero blocks and partial blocks: trimming NUL bytes would erase
  // real file contents as well as the padding appended by tarfile.close().
  for (const size of [0, 511, 512, 513, 8192]) {
    const files = {
      "package.json": Buffer.from(JSON.stringify({
        files: ["NOTICE.md", "payload.bin"], exports: { ".": "./index.js" },
      })),
      "NOTICE.md": Buffer.from("synthetic attribution\n"),
      "payload.bin": Buffer.alloc(size),
    };
    execFileSync("python3", ["-c", `import base64, io, json, sys, tarfile
with tarfile.open(sys.argv[1], 'w:gz', format=tarfile.USTAR_FORMAT) as target:
    for name, encoded in json.load(sys.stdin).items():
        data = base64.b64decode(encoded)
        member = tarfile.TarInfo('package/' + name)
        member.size = len(data)
        target.addfile(member, io.BytesIO(data))
`, tarball], {
      env, input: JSON.stringify(Object.fromEntries(
        Object.entries(files).map(([name, bytes]) => [name, bytes.toString("base64")]),
      )),
    });
    for (const mode of ["control", "exports", "files", "extra", "missing"]) {
      writeFileSync(repacked, repackPublicationTarball(tarball, mode, env));
      const expected = Object.keys(files).filter((name) => mode !== "missing" || name !== "NOTICE.md");
      if (mode === "extra") expected.push("unreviewed.js");
      writeFileSync(inventory, JSON.stringify({ schemaVersion: 1, files: expected.sort() }));
      const result = spawnSync("python3", [inspector,
        "--tarball", repacked, "--declared-inventory-json", inventory,
      ], { env, encoding: "utf8" });
      assert.ifError(result.error);
      assert.equal(result.status, 0, `${mode}, payload size ${size}: ${result.stderr}`);
      const report = JSON.parse(result.stdout);
      assert.equal(report.verdict, "canonical");
      for (const member of report.members) {
        if (mode !== "control" && member.normalizedPath === "package.json") continue;
        const bytes = member.normalizedPath === "unreviewed.js"
          ? Buffer.from("export const unreviewed = true;\n") : files[member.normalizedPath];
        assert.equal(member.size, bytes.length);
        assert.equal(member.sha256, createHash("sha256").update(bytes).digest("hex"));
      }
      if (mode === "control" && size === 513) {
        writeFileSync(repacked, gzipSync(Buffer.concat([
          gunzipSync(readFileSync(repacked)), Buffer.alloc(512),
        ])));
        const padded = spawnSync("python3", [inspector,
          "--tarball", repacked, "--declared-inventory-json", inventory,
        ], { env, encoding: "utf8" });
        assert.ifError(padded.error);
        assert.equal(padded.status, 1);
        assert.match(padded.stderr, /tar stream must end with exactly two zero blocks/u);
      }
    }
  }
});
