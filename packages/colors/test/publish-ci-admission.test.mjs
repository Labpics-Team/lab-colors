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
import { parseAllDocuments } from "yaml";

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

function matrixRows(matrix) {
  if (matrix === undefined) return [{}];
  assert.ok(matrix && typeof matrix === "object" && !Array.isArray(matrix), "matrix must be declared in YAML");
  const axes = Object.keys(matrix).filter((key) => key !== "include" && key !== "exclude");
  let original = axes.length ? [{}] : [];
  for (const axis of axes) {
    assert.ok(Array.isArray(matrix[axis]) && matrix[axis].length > 0, "matrix axis must contain values");
    original = original.flatMap((row) => matrix[axis].map((value) => ({ ...row, [axis]: value })));
    assert.ok(original.length <= 256, "producer matrix exceeds the native job limit");
  }
  const includes = matrix.include ?? [], excludes = matrix.exclude ?? [];
  assert.ok(Array.isArray(includes) && Array.isArray(excludes), "matrix include/exclude must be lists");
  const equal = (left, right) => JSON.stringify(left) === JSON.stringify(right);
  for (const exclusion of excludes) {
    assert.ok(exclusion && typeof exclusion === "object" && !Array.isArray(exclusion), "matrix exclude must be a mapping");
  }
  original = original.filter((row) => !excludes.some((exclude) =>
    Object.entries(exclude).every(([key, value]) => equal(row[key], value))));
  const rows = original.map((row) => ({ ...row }));
  for (const addition of includes) {
    assert.ok(addition && typeof addition === "object" && !Array.isArray(addition), "matrix include must be a mapping");
    const matches = original.flatMap((row, index) =>
      axes.every((axis) => !(axis in addition) || equal(row[axis], addition[axis])) ? [index] : []);
    if (matches.length) for (const index of matches) Object.assign(rows[index], addition);
    else rows.push({ ...addition });
  }
  assert.ok(rows.length > 0 && rows.length <= 256, "producer matrix must yield native jobs");
  return rows;
}

// References follow local calls; job names retain caller depth and multiplicity.
function producerGraph(entry, readWorkflow = workflow) {
  const names = [], references = new Set(), documents = new Map();
  function visit(file, callers, ancestors) {
    assert.ok(!ancestors.includes(file), "recursive workflow call: " + [...ancestors, file].join(" -> "));
    if (!documents.has(file)) {
      const docs = parseAllDocuments(readWorkflow(file), { strict: true, uniqueKeys: true });
      assert.equal(docs.length, 1, "producer must contain one YAML document");
      assert.deepEqual([...docs[0].errors, ...docs[0].warnings], [], "ambiguous producer YAML: " + file);
      const value = docs[0].toJS({ maxAliasCount: 100 });
      assert.ok(value?.jobs && typeof value.jobs === "object" && !Array.isArray(value.jobs), "producer lacks jobs: " + file);
      assert.ok(Object.keys(value.jobs).length > 0, "producer has no jobs: " + file);
      documents.set(file, value);
    }
    for (const [id, job] of Object.entries(documents.get(file).jobs)) {
      assert.ok(job && typeof job === "object" && !Array.isArray(job), "invalid producer job: " + id);
      assert.ok(job.if === undefined || job.if === true || job.if === "always()" || job.if === "${{ always() }}",
        "healthy fixture cannot assume conditional producer success: " + id);
      const template = job.name ?? id;
      assert.equal(typeof template, "string", "producer job name must be text");
      for (const row of matrixRows(job.strategy?.matrix)) {
        const name = template.replace(/\$\{\{\s*matrix\.([A-Za-z_][A-Za-z0-9_.-]*)\s*\}\}/gu, (_, key) => {
          const value = key.split(".").reduce((value, part) => value?.[part], row);
          assert.ok(["string", "number", "boolean"].includes(typeof value), "unresolved matrix job name: " + key);
          return String(value);
        });
        assert.ok(name.length > 0 && !name.includes("${{"), "unresolved producer job name: " + name);
        if (job.uses !== undefined) {
          assert.ok(job.steps === undefined && job["runs-on"] === undefined, "workflow call cannot contain an inline job");
          assert.equal(typeof job.uses, "string", "workflow call must be text");
          const local = job.uses.match(/^\.\/\.github\/workflows\/([A-Za-z0-9_-]+\.ya?ml)$/u);
          assert.ok(local, "producer call escaped its own source revision: " + job.uses);
          references.add(local[1]);
          visit(local[1], [...callers, name], [...ancestors, file]);
        } else {
          assert.ok(job["runs-on"] && Array.isArray(job.steps) && job.steps.length > 0, "producer lacks an executable job: " + id);
          names.push([...callers, name].join(" / "));
        }
      }
    }
  }
  visit(entry, [], []);
  assert.ok(names.length > 0, "producer fixture must contain actual jobs");
  return { names, references: [...references] };
}

function evidence(readWorkflow = workflow) {
  const ciGraph = producerGraph("ci.yml", readWorkflow), nativeGraph = producerGraph("native-conformance.yml", readWorkflow);
  assert.ok(ciGraph.names.some((name) => name.endsWith(" / core tests and artifact contracts")), "healthy control lacks the Core aggregate");
  assert.ok(ciGraph.names.some((name) => name.endsWith(" / test")), "healthy control lacks the full verification aggregate");
  const run = (id, file, references) => ({
    id, run_attempt: 1, path: ".github/workflows/" + file,
    head_sha: sha, head_branch: "main", event: "push",
    status: "completed", conclusion: "success", referenced_workflows: references,
  });
  const references = (graph) => graph.references.map((file) => ({
    path: repository + "/.github/workflows/" + file + "@" + sha, sha,
  }));
  const ci = run(101, "ci.yml", references(ciGraph));
  const native = run(202, "native-conformance.yml", references(nativeGraph));
  const jobs = (id, names) => names.map((name, index) => ({
    id: id * 100 + index, run_id: id, name: "caller / nested caller / " + name,
    status: "completed", conclusion: "success",
  }));
  return {
    runs: { "ci.yml": [ci], "native-conformance.yml": [native] },
    jobs: { 101: jobs(101, ciGraph.names), 202: jobs(202, nativeGraph.names) },
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

test("producer fixture preserves YAML names, transitive calls and duplicate job suffixes", () => {
  const files = {
    "entry.yml": "jobs:\n  left:\n    name: \"first: caller\"\n    uses: &call ./.github/workflows/child.yml\n  right:\n    name: >-\n      another caller\n    uses: *call\n",
    "child.yml": "jobs:\n  nested:\n    uses: ./.github/workflows/leaf.yml\n",
    "leaf.yml": "jobs:\n  check:\n    name: 'quoted: leaf'\n    runs-on: ubuntu-latest\n    steps: [{run: true}]\n",
  };
  const graph = producerGraph("entry.yml", (file) => files[file]);
  assert.deepEqual(graph.references, ["child.yml", "leaf.yml"]);
  assert.deepEqual(graph.names, ["first: caller / nested / quoted: leaf", "another caller / nested / quoted: leaf"]);
});

test("producer fixture rejects unknown, cyclic and ambiguous workflow declarations", () => {
  const leaf = "jobs:\n  check:\n    name: check\n    steps: [{run: true}]\n";
  for (const [source, reason] of [
    ["jobs:\n  call:\n    uses: foreign/repo/.github/workflows/worker.yml@main\n", /escaped/u],
    ["jobs:\n  call:\n    uses: ./.github/workflows/unknown.yml\n", /unknown workflow/u],
    ["jobs:\n  call:\n    uses: ./.github/workflows/entry.yml\n", /recursive/u],
    ["jobs:\n  check: {name: first}\n  check: {name: second}\n", /ambiguous/u],
    ["jobs:\n  check:\n    name: ${{ github.unknown }}\n", /unresolved/u],
    ["jobs:\n  check:\n    if: false\n    name: check\n", /conditional producer/u],
  ]) {
    assert.throws(() => producerGraph("entry.yml", (file) => {
      assert.ok(file === "entry.yml" || file === "leaf.yml", "unknown workflow: " + file);
      return file === "entry.yml" ? source : leaf;
    }), reason);
  }
});

test("producer fixture matrix names preserve multiplicity and include after exclude", () => {
  assert.deepEqual(matrixRows({ os: ["linux", "mac"], exclude: [{ os: "mac" }], include: [{ os: "mac", extra: true }] }),
    [{ os: "linux" }, { os: "mac", extra: true }]);
  const graph = producerGraph("entry.yml", () =>
    "jobs:\n  check:\n    name: check\n    runs-on: ubuntu-latest\n    strategy:\n      matrix:\n        shard: [0, 1]\n    steps: [{run: true}]\n");
  assert.deepEqual(graph.names, ["check", "check"]);
});

test("publication refuses duplicate actual reusable calls instead of deduplicating their jobs", async () => {
  const record = evidence((file) => workflow(file) + (file === "ci.yml" ?
    "\n  duplicate-ci:\n    name: Other caller\n    uses: ./.github/workflows/ci-worker.yml\n" : ""));
  assert.equal(record.runs["ci.yml"][0].referenced_workflows.filter((reference) => reference.path.includes("/ci-worker.yml@")).length, 1);
  assertRefused(await admit(record), /expected one job/u);
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
  assertRefused(await admit(record), /swift package/u);
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
