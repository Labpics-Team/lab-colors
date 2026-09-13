import assert from "node:assert/strict";
import fs from "node:fs/promises";
import { syncBuiltinESMExports } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { atomicWriteGeneratedFile, fsyncDirectory } from "../../../scripts/atomic-write.mjs";

const realOpen = fs.open;
const realRm = fs.rm;

async function fixture(t) {
  const directory = await fs.mkdtemp(join(tmpdir(), "colors-atomic-cleanup-"));
  t.after(async () => {
    t.mock.restoreAll();
    syncBuiltinESMExports();
    await realRm(directory, { recursive: true, force: true });
  });
  const path = join(directory, "generated.json");
  await fs.writeFile(path, "previous");
  return { directory, path };
}

function patch(t, target, key, implementation) {
  t.mock.method(target, key, implementation);
  syncBuiltinESMExports();
}

function aggregate(...failures) {
  return (error) => {
    assert.ok(error instanceof AggregateError);
    assert.equal(error.cause, failures[0]);
    assert.equal(error.errors.length, failures.length);
    failures.forEach((failure, index) => assert.equal(error.errors[index], failure));
    return true;
  };
}

async function noTemporary(directory) {
  assert.deepEqual(await fs.readdir(directory), ["generated.json"]);
}

test("atomic write replaces the file, flushes the installed entry and removes its temporary directory", async (t) => {
  const { directory, path } = await fixture(t);
  let flushes = 0;
  assert.equal(await atomicWriteGeneratedFile(path, "replacement", {
    fsyncDirectory: async (parent) => {
      flushes += 1;
      assert.equal(parent, directory);
      assert.equal(await fs.readFile(path, "utf8"), "replacement");
      await fsyncDirectory(parent);
    },
  }), undefined);
  assert.equal(flushes, 1);
  assert.equal(await fs.readFile(path, "utf8"), "replacement");
  if (process.platform !== "win32") assert.equal((await fs.stat(path)).mode & 0o777, 0o644);
  await noTemporary(directory);
});

test("post-rename flush failure stays the original failure and never claims rollback", async (t) => {
  const { directory, path } = await fixture(t);
  const primary = Object.freeze(new Error("directory flush failed"));
  await assert.rejects(atomicWriteGeneratedFile(path, "replacement", {
    fsyncDirectory: async () => { throw primary; },
  }), (error) => error === primary);
  assert.equal(await fs.readFile(path, "utf8"), "replacement");
  await noTemporary(directory);
});

for (const [label, primary] of [
  ["frozen error", Object.freeze(new Error("directory flush failed"))],
  ["undefined", undefined], ["null", null], ["false", false],
]) {
  test(`flush plus temporary-removal failure preserves both thrown values: ${label}`, async (t) => {
    const { directory, path } = await fixture(t);
    const cleanup = new Error("temporary removal failed");
    let removals = 0;
    patch(t, fs, "rm", async (candidate, options) => {
      assert.equal(candidate.startsWith(join(directory, ".generated.json.tmp-")), true);
      assert.deepEqual(options, { recursive: true, force: true });
      removals += 1;
      throw cleanup;
    });
    await assert.rejects(atomicWriteGeneratedFile(path, "replacement", {
      fsyncDirectory: async () => { throw primary; },
    }), aggregate(primary, cleanup));
    assert.equal(removals, 1);
    assert.equal(await fs.readFile(path, "utf8"), "replacement");
  });
}

for (const operation of ["chmod", "writeFile", "sync"]) {
  for (const closeFails of [false, true]) {
    test(`${operation} failure ${closeFails ? "with" : "without"} close failure releases all acquired resources`, async (t) => {
      const { directory, path } = await fixture(t);
      const primary = Object.freeze(new Error(`${operation} failed`));
      const cleanup = new Error("close failed");
      let opened;
      let closes = 0;
      patch(t, fs, "open", async (...args) => {
        opened = await realOpen(...args);
        const close = opened.close.bind(opened);
        t.mock.method(opened, operation, async () => { throw primary; });
        t.mock.method(opened, "close", async () => {
          closes += 1;
          await close();
          if (closeFails) throw cleanup;
        });
        return opened;
      });
      await assert.rejects(atomicWriteGeneratedFile(path, "replacement"),
        closeFails ? aggregate(primary, cleanup) : (error) => error === primary);
      assert.equal(closes, 1);
      assert.equal(opened.fd, -1);
      assert.equal(await fs.readFile(path, "utf8"), "previous");
      await noTemporary(directory);
    });
  }
}

test("a failed pre-rename close is not retried and cannot replace the destination", async (t) => {
  const { directory, path } = await fixture(t);
  const failure = new Error("close failed");
  let closes = 0;
  patch(t, fs, "open", async (...args) => {
    const handle = await realOpen(...args);
    const close = handle.close.bind(handle);
    t.mock.method(handle, "close", async () => {
      closes += 1;
      await close();
      throw failure;
    });
    return handle;
  });
  await assert.rejects(atomicWriteGeneratedFile(path, "replacement"), (error) => error === failure);
  assert.equal(closes, 1);
  assert.equal(await fs.readFile(path, "utf8"), "previous");
  await noTemporary(directory);
});

test("a failure to open the file still removes the acquired temporary directory", async (t) => {
  const { directory, path } = await fixture(t);
  const failure = new Error("open failed");
  patch(t, fs, "open", async () => { throw failure; });
  await assert.rejects(atomicWriteGeneratedFile(path, "replacement"), (error) => error === failure);
  assert.equal(await fs.readFile(path, "utf8"), "previous");
  await noTemporary(directory);
});

test("directory-flush lifecycle preserves its platform contract and ordered failures", async (t) => {
  const { directory } = await fixture(t);
  const primary = new Error("directory sync failed");
  const cleanup = new Error("directory close failed");
  let opens = 0;
  let closes = 0;
  let opened;
  patch(t, fs, "open", async (...args) => {
    opens += 1;
    opened = await realOpen(...args);
    const close = opened.close.bind(opened);
    t.mock.method(opened, "sync", async () => { throw primary; });
    t.mock.method(opened, "close", async () => { closes += 1; await close(); throw cleanup; });
    return opened;
  });
  if (process.platform === "win32") {
    await fsyncDirectory(directory);
    assert.equal(opens, 0);
  } else {
    await assert.rejects(fsyncDirectory(directory), aggregate(primary, cleanup));
    assert.equal(opens, 1);
    assert.equal(closes, 1);
    assert.equal(opened.fd, -1);
  }
});


test("cleanup-only failure is propagated without wrapping after a successful replacement", async (t) => {
  const { path } = await fixture(t);
  const failure = new Error("temporary removal failed");
  patch(t, fs, "rm", async () => { throw failure; });
  await assert.rejects(atomicWriteGeneratedFile(path, "replacement"), (error) => error === failure);
  assert.equal(await fs.readFile(path, "utf8"), "replacement");
});

test("write, close and removal failures remain ordered and every cleanup is attempted", async (t) => {
  const { path } = await fixture(t);
  const primary = new Error("write failed");
  const closeFailure = new Error("close failed");
  const removalFailure = new Error("removal failed");
  const events = [];
  patch(t, fs, "open", async (...args) => {
    const handle = await realOpen(...args);
    const close = handle.close.bind(handle);
    t.mock.method(handle, "writeFile", async () => { events.push("write"); throw primary; });
    t.mock.method(handle, "close", async () => { events.push("close"); await close(); throw closeFailure; });
    return handle;
  });
  patch(t, fs, "rm", async () => { events.push("remove"); throw removalFailure; });
  await assert.rejects(atomicWriteGeneratedFile(path, "replacement"), aggregate(primary, closeFailure, removalFailure));
  assert.deepEqual(events, ["write", "close", "remove"]);
  assert.equal(await fs.readFile(path, "utf8"), "previous");
});
