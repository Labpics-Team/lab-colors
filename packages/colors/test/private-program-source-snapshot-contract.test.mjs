import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  mkdir,
  mkdtemp,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  admitSourceEntry,
  readAdmittedSource,
} from "../../../scripts/build-private-program.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const read = (...parts) => readFileSync(join(root, ...parts), "utf8");
const fileLink = (target, path) =>
  symlink(target, path, process.platform === "win32" ? "file" : undefined);
const directoryLink = (target, path) =>
  symlink(target, path, process.platform === "win32" ? "junction" : "dir");

// RED/GREEN boundary: the digest must bind every digested byte to the inode it
// admitted during the walk, so a same-UID swap between admission and read can
// never change what the receipt binds. Removing the discipline makes these
// assertions (and the swap/replace tests below) fail.
test("the private Program source digest binds every digested byte to its admitted inode", () => {
  const source = read("scripts", "build-private-program.mjs");
  assert.match(
    source,
    /readAdmittedSource\(await admitSourceEntry\(path\)\)/u,
    "the digest must read every source file through the admitted snapshot handle",
  );
  assert.match(
    source,
    /opened\.ino !== admitted\.ino/u,
    "a path swap between admission and read must fail closed",
  );
  assert.match(
    source,
    /source changed between admission and read/u,
    "the fail-closed outcome must name the admission/read mismatch",
  );
});

test("admitted source bytes are read from the exact admitted inode", async () => {
  const rootDir = await mkdtemp(join(tmpdir(), "labcolors-source-snapshot-"));
  try {
    await writeFile(join(rootDir, "a.txt"), "admitted bytes\n", "utf8");
    const admitted = await admitSourceEntry(join(rootDir, "a.txt"), {
      repoRoot: rootDir,
    });
    assert.equal(resolve(admitted.target), resolve(join(rootDir, "a.txt")));
    assert.equal((await readAdmittedSource(admitted)).toString("utf8"), "admitted bytes\n");
  } finally {
    await rm(rootDir, { recursive: true, force: true });
  }
});

test("a contained file symlink is read through its resolved target", async () => {
  const rootDir = await mkdtemp(join(tmpdir(), "labcolors-source-snapshot-"));
  try {
    await writeFile(join(rootDir, "a.txt"), "linked bytes\n", "utf8");
    await fileLink(join(rootDir, "a.txt"), join(rootDir, "link.txt"));
    const admitted = await admitSourceEntry(join(rootDir, "link.txt"), {
      repoRoot: rootDir,
    });
    assert.equal((await readAdmittedSource(admitted)).toString("utf8"), "linked bytes\n");
  } finally {
    await rm(rootDir, { recursive: true, force: true });
  }
});

test("swapping the symlink after admission cannot change the digested bytes", async () => {
  const rootDir = await mkdtemp(join(tmpdir(), "labcolors-source-snapshot-"));
  try {
    await writeFile(join(rootDir, "a.txt"), "admitted\n", "utf8");
    await writeFile(join(rootDir, "b.txt"), "swapped target\n", "utf8");
    const linkPath = join(rootDir, "link.txt");
    await fileLink(join(rootDir, "a.txt"), linkPath);
    const admitted = await admitSourceEntry(linkPath, { repoRoot: rootDir });
    await rm(linkPath);
    await fileLink(join(rootDir, "b.txt"), linkPath);
    assert.equal(
      (await readAdmittedSource(admitted)).toString("utf8"),
      "admitted\n",
      "the digest must bind the admitted inode, not the swapped path resolution",
    );
  } finally {
    await rm(rootDir, { recursive: true, force: true });
  }
});

test("replacing the admitted target before the read fails closed", async () => {
  const rootDir = await mkdtemp(join(tmpdir(), "labcolors-source-snapshot-"));
  try {
    const targetPath = join(rootDir, "a.txt");
    await writeFile(targetPath, "first generation\n", "utf8");
    const admitted = await admitSourceEntry(targetPath, { repoRoot: rootDir });
    await rm(targetPath);
    await writeFile(targetPath, "second generation\n", "utf8");
    await assert.rejects(
      readAdmittedSource(admitted),
      /source changed between admission and read/u,
      "a replaced target must be rejected, not silently hashed",
    );
  } finally {
    await rm(rootDir, { recursive: true, force: true });
  }
});

test("admission rejects a target reached through a swapped directory symlink", async () => {
  const rootDir = await mkdtemp(join(tmpdir(), "labcolors-source-snapshot-"));
  try {
    const sourceRoot = join(rootDir, "source");
    const outsideRoot = join(rootDir, "outside");
    const subdir = join(sourceRoot, "sub");
    await mkdir(subdir, { recursive: true });
    await writeFile(join(subdir, "a.txt"), "inside\n", "utf8");
    const admitted = await admitSourceEntry(join(subdir, "a.txt"), {
      repoRoot: sourceRoot,
    });
    await mkdir(outsideRoot, { recursive: true });
    await writeFile(join(outsideRoot, "a.txt"), "outside bytes\n", "utf8");
    await rm(subdir, { recursive: true, force: true });
    await directoryLink(outsideRoot, subdir);
    await assert.rejects(
      readAdmittedSource(admitted),
      /source changed between admission and read/u,
      "a directory-component swap must fail closed",
    );
  } finally {
    await rm(rootDir, { recursive: true, force: true });
  }
});

test("admission rejects a symlink escaping the repository boundary", async () => {
  const rootDir = await mkdtemp(join(tmpdir(), "labcolors-source-snapshot-"));
  try {
    const sourceRoot = join(rootDir, "source");
    const outsideRoot = join(rootDir, "outside");
    await mkdir(sourceRoot, { recursive: true });
    await mkdir(outsideRoot, { recursive: true });
    await writeFile(join(outsideRoot, "secret.txt"), "outside\n", "utf8");
    await fileLink(
      join(outsideRoot, "secret.txt"),
      join(sourceRoot, "escape.txt"),
    );
    await assert.rejects(
      admitSourceEntry(join(sourceRoot, "escape.txt"), { repoRoot: sourceRoot }),
      /does not resolve to a repository file/u,
    );
  } finally {
    await rm(rootDir, { recursive: true, force: true });
  }
});
