import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { test } from "node:test";

import {
  atomicWriteGeneratedFile,
  fsyncDirectory,
} from "../../../scripts/atomic-write.mjs";

function temporaryRoot() {
  return mkdtempSync(join(tmpdir(), "labcolors-atomic-durability-"));
}

function leftoverTemporaryDirectories(root, destination) {
  return readdirSync(root).filter((name) =>
    name.startsWith(`.${basename(destination)}.tmp-`),
  );
}

test("atomic write flushes the destination directory after the rename", async () => {
  const root = temporaryRoot();
  try {
    const destination = join(root, "artifact.json");
    const flushed = [];
    await atomicWriteGeneratedFile(destination, "replacement", {
      fsyncDirectory: async (directory) => {
        flushed.push(directory);
      },
    });
    assert.deepEqual(
      flushed,
      [root],
      "the destination directory must be flushed exactly once, after the rename",
    );
    assert.equal(readFileSync(destination, "utf8"), "replacement");
    assert.deepEqual(
      leftoverTemporaryDirectories(root, destination),
      [],
      "temporary directories must be removed after a successful write",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("atomic write fails closed when the directory flush errors", async () => {
  const root = temporaryRoot();
  try {
    const destination = join(root, "artifact.json");
    await assert.rejects(
      atomicWriteGeneratedFile(destination, "replacement", {
        fsyncDirectory: async () => {
          throw new Error("simulated directory flush failure");
        },
      }),
      /simulated directory flush failure/u,
    );
    assert.deepEqual(
      leftoverTemporaryDirectories(root, destination),
      [],
      "temporary directories must be removed even when the flush fails",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fsyncDirectory flushes a real directory on POSIX and stays a no-op on Win32", async () => {
  const root = temporaryRoot();
  try {
    // Must not throw on either platform: POSIX runners fsync a real directory
    // handle, while Win32 keeps the documented file-fsync durability floor.
    await fsyncDirectory(root);
    // The default seam must keep working end to end on the host platform.
    const destination = join(root, "artifact.json");
    await atomicWriteGeneratedFile(destination, "replacement");
    assert.equal(readFileSync(destination, "utf8"), "replacement");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
