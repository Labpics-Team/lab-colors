import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { appendFile, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { verifyPackageRelease } from "./verify-package-release.mjs";

const ROOT = fileURLToPath(new URL("../", import.meta.url));
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");

export function artifactAdmissionScript(workflow) {
  const lines = workflow.split("\n");
  const indices = lines.flatMap((line, index) => line === "        id: verified-artifact" ? [index] : []);
  assert.equal(indices.length, 1, "publication must have one artifact admission step");
  const start = indices[0];
  const next = lines.findIndex((line, index) => index > start && /^      - /u.test(line));
  const step = lines.slice(start, next < 0 ? undefined : next);
  const opening = step.indexOf("          node <<'NODE'");
  const closing = step.indexOf("          NODE", opening + 1);
  assert.ok(opening > 0 && closing > opening, "artifact admission must contain its complete Node script");
  return step.slice(opening + 1, closing).map((line) => {
    assert.ok(line === "" || line.startsWith("          "), "artifact admission indentation changed");
    return line.slice(10);
  }).join("\n");
}

export function repackPublicationTarball(tarball, mode, env) {
  // stdlib tarfile edits a copy of the already-qualified archive, never the
  // source tree. A positive re-encoding control prevents format errors from
  // being mistaken for successful detection of a semantic substitution.
  const editTar = `import copy, gzip, io, json, sys, tarfile
mode = sys.argv[2]
raw = io.BytesIO()
changed = False
with tarfile.open(sys.argv[1], 'r:gz') as source, tarfile.open(fileobj=raw, mode='w', format=tarfile.USTAR_FORMAT) as target:
    for item in source:
        if not item.isfile():
            raise ValueError('qualified archive contains a non-file')
        data = source.extractfile(item).read()
        if item.name == 'package/NOTICE.md' and mode == 'missing':
            changed = True
            continue
        if item.name == 'package/package.json':
            package = json.loads(data)
            if mode == 'exports':
                package['exports']['./unreviewed'] = './index.js'
                changed = True
            elif mode == 'files':
                package['files'].append('README.md')
                changed = True
            elif mode == 'extra':
                package['files'].append('unreviewed.js')
                changed = True
            elif mode == 'missing':
                package['files'].remove('NOTICE.md')
            if mode in ('exports', 'files', 'extra', 'missing'):
                data = json.dumps(package, ensure_ascii=False).encode('utf-8')
        member = copy.copy(item)
        member.size = len(data)
        member.pax_headers = {}
        target.addfile(member, io.BytesIO(data))
    if mode == 'extra':
        data = b'export const unreviewed = true;\\n'
        member = tarfile.TarInfo('package/unreviewed.js')
        member.size = len(data)
        member.mode = 0o644
        target.addfile(member, io.BytesIO(data))
    members_end = raw.tell()
if mode != 'control' and not changed:
    raise ValueError('archive mutation did not reach its target')
# tarfile.close() pads to a record; npm admission requires exactly two end blocks.
payload = raw.getvalue()[:members_end] + bytes(2 * tarfile.BLOCKSIZE)
sys.stdout.buffer.write(gzip.compress(payload, mtime=0))
`;
  const edited = spawnSync("python3", ["-c", editTar, tarball, mode], {
    cwd: ROOT, env, timeout: 30_000, maxBuffer: 64 * 1024 * 1024,
  });
  if (edited.error) throw edited.error;
  assert.equal(edited.status, 0, edited.stderr.toString("utf8"));
  return edited.stdout;
}

// Preserve the publisher's real parser and IO. Only the source artifact and
// process environment vary; this path never extracts or executes npm publish.
export async function qualifyPublication({ manifest, tarball }) {
  const [manifestBytes, originalBytes, workflow] = await Promise.all([
    readFile(manifest), readFile(tarball),
    readFile(join(ROOT, ".github/workflows/publish-worker.yml"), "utf8"),
  ]);
  const record = JSON.parse(manifestBytes.toString("utf8"));
  assert.equal(digest(originalBytes), record.artifacts.tarball.sha256);
  assert.equal(originalBytes.length, record.artifacts.tarball.bytes);
  const directory = await mkdtemp(join(tmpdir(), "colors-publication-"));
  try {
    const script = join(directory, "artifact-admission.cjs");
    await writeFile(script, artifactAdmissionScript(workflow), { flag: "wx", mode: 0o600 });
    // Do not inherit process credentials, loader hooks, user npm configuration,
    // or a caller-selected script. PATH only selects the existing tar/Python tools.
    const env = {
      PATH: process.env.PATH,
      ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}),
      HOME: directory,
      NPM_CONFIG_USERCONFIG: join(directory, "empty.npmrc"),
      NPM_CONFIG_CACHE: join(directory, "npm-cache"),
      GITHUB_WORKSPACE: ROOT,
      EXPECTED_SHA: record.sourceSha,
      EXPECTED_TAG: "colors-v" + record.npm,
      EXPECTED_NODE: process.versions.node,
      EXPECTED_NPM: record.requirements.buildToolchain.npm,
    };
    await writeFile(env.NPM_CONFIG_USERCONFIG, "", { flag: "wx", mode: 0o600 });
    let sequence = 0;
    async function run(label, {
      bytes = originalBytes, edit = () => {}, rebind = false,
      verifiedSha256 = digest(bytes), workspace = ROOT,
    } = {}) {
      const root = join(directory, String(sequence++));
      const artifacts = join(root, "artifacts");
      const temporary = join(root, "temporary");
      await Promise.all([mkdir(artifacts, { recursive: true }), mkdir(temporary, { recursive: true })]);
      const input = structuredClone(record);
      if (rebind) input.artifacts.tarball = {
        path: ".release/" + digest(bytes) + ".tgz", bytes: bytes.length, sha256: digest(bytes),
      };
      edit(input);
      const path = join(artifacts, basename(input.artifacts.tarball.path));
      await Promise.all([
        writeFile(path, bytes, { flag: "wx" }),
        writeFile(join(artifacts, "release-manifest.json"), JSON.stringify(input), { flag: "wx" }),
      ]);
      const output = join(root, "output");
      const result = spawnSync(process.execPath, [script], {
        cwd: ROOT, encoding: "utf8", timeout: 120_000, maxBuffer: 2 * 1024 * 1024,
        env: {
          ...env, ARTIFACT_DIR: artifacts, RUNNER_TEMP: temporary,
          TMPDIR: temporary, GITHUB_OUTPUT: output, GITHUB_WORKSPACE: workspace,
          EXPECTED_TARBALL_SHA256: verifiedSha256,
        },
      });
      if (result.error) throw result.error;
      assert.equal(result.signal, null, label + " was terminated");
      let outputs = "";
      try { outputs = await readFile(output, "utf8"); }
      catch (error) { if (error.code !== "ENOENT") throw error; }
      return { ...result, output: outputs, temporary, label, bytes };
    }
    async function accepted(result) {
      assert.equal(result.status, 0, result.label + ": " + result.stderr);
      const entries = result.output.trimEnd().split("\n").map((line) => {
        const separator = line.indexOf("=");
        assert.ok(separator > 0, "publication output must be a key/value pair");
        return [line.slice(0, separator), line.slice(separator + 1)];
      });
      assert.deepEqual(entries.map(([key]) => key), ["tarball", "sha256"]);
      const output = Object.fromEntries(entries);
      assert.equal(output.sha256, digest(result.bytes));
      assert.ok(isAbsolute(output.tarball));
      const path = relative(result.temporary, output.tarball);
      assert.ok(path !== "" && !isAbsolute(path) && path.split(sep)[0] !== "..");
      assert.deepEqual(await readFile(output.tarball), result.bytes, "published snapshot must preserve every byte");
    }
    function rejected(result, reason) {
      assert.equal(result.status, 1, result.label + ": " + result.stderr);
      assert.equal(result.output, "", result.label + " exported partial publication authority");
      assert.match(result.stderr, reason, result.label + " failed for another reason");
    }
    await accepted(await run("prepared package"));
    rejected(await run("foreign source", { edit: (input) => { input.sourceSha = "0".repeat(40); } }), /manifest sourceSha/u);
    rejected(await run("foreign version", { edit: (input) => { input.npm = "999.0.0"; } }), /manifest npm/u);
    rejected(await run("lost reproducibility", { edit: (input) => { input.reproducibility.byteIdentical = false; } }), /reproducibility contract/u);
    rejected(await run("changed bytes", { bytes: Buffer.concat([originalBytes, Buffer.from([0])]) }), /tarball bytes|tarball sha256/u);
    for (const verifiedSha256 of ["", "invalid", "0".repeat(63), "A".repeat(64)]) {
      rejected(await run("invalid preparation digest", { verifiedSha256 }), /verified tarball digest/u);
    }
    rejected(await run("foreign preparation digest", { verifiedSha256: "0".repeat(64) }), /verified preparation output/u);

    // A source fixture varies only the checked-out package contract. The archive,
    // tag and manifest keep their independent identity, and ROOT remains untouched.
    for (const [label, editSource] of [
      ["matching source copy", () => {}],
      ["foreign source name", (source) => { source.name = "@other/colors"; }],
      ["foreign source version", (source) => { source.version += "-other"; }],
    ]) {
      const workspace = await mkdtemp(join(directory, "source-"));
      for (const path of [
        "packages/colors/package.json",
        "scripts/point-support-release-contract.cjs",
        "scripts/inspect-npm-tarball.py",
      ]) {
        const destination = join(workspace, path);
        await mkdir(dirname(destination), { recursive: true });
        await copyFile(join(ROOT, path), destination);
      }
      const sourcePath = join(workspace, "packages/colors/package.json");
      const source = JSON.parse(await readFile(sourcePath, "utf8"));
      editSource(source);
      await writeFile(sourcePath, JSON.stringify(source));
      const result = await run(label, { workspace });
      if (label === "matching source copy") await accepted(result);
      else rejected(result, /source package identity differs from the release tag/u);
    }

    for (const [mode, reason] of [
      ["control", null],
      ["exports", /packed package exports differ/u],
      ["files", /packed package files declaration differs/u],
      ["extra", /inventory|member|declared|unexpected/iu],
      ["missing", /inventory|member|declared|missing/iu],
    ]) {
      const edited = repackPublicationTarball(tarball, mode, env);
      const result = await run("repacked " + mode, { bytes: edited, rebind: true });
      if (reason === null) {
        await accepted(result);
        assert.notEqual(digest(edited), digest(originalBytes), "substitution control must change archive bytes");
        rejected(await run("self-consistent archive substitution", {
          bytes: edited, rebind: true, verifiedSha256: digest(originalBytes),
        }), /verified preparation output/u);
      } else rejected(result, reason);
    }
    console.log("Publication admission: prepared archive, source copy and re-encoding accepted; invalid sources, evidence and delivery refused without outputs");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  verifyPackageRelease().then(async (result) => {
    await qualifyPublication(result);
    if (process.env.GITHUB_OUTPUT) {
      const entries = ["manifest", "tarball"].map((key) => {
        const path = result[key];
        assert.ok(!path.includes("\n") && !path.includes("\r"), "unsafe release output path");
        return key + "=" + path + "\n";
      });
      await appendFile(process.env.GITHUB_OUTPUT, entries.join(""), "utf8");
    }
    console.log("verified " + result.tarball);
  }).catch((error) => {
    console.error(error instanceof Error ? error.stack : String(error));
    process.exitCode = 1;
  });
}
