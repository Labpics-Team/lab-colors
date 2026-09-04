import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import test from "node:test";

import {
  assertExactProgramResult,
  launchDriver,
  stopDriver,
} from "./test-program-runtime-browser.mjs";

const LOOPBACK = "127.0.0.1";
const FAKE_DRIVER = String.raw`
  import { createServer } from "node:http";

  const [markerStream, ...args] = process.argv.slice(1);
  const portArgument = args.find((value) => value.startsWith("--port="));
  const baseArgument = args.find((value) => value.startsWith("--url-base="));
  if (!portArgument || !baseArgument) process.exit(97);
  const port = Number(portArgument.slice("--port=".length));
  const basePath = baseArgument.slice("--url-base=".length);
  const server = createServer((request, response) => {
    if (request.url === basePath + "/status") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ value: { ready: true } }));
      return;
    }
    response.writeHead(404).end();
  });
  server.once("error", (error) => {
    console.error(error.code ?? error.message);
    process.exitCode = 98;
  });
  server.listen(port, "127.0.0.1", () => {
    process[markerStream].write(
      "ChromeDriver was started successfully on port " + port + ".\\n",
    );
  });
  process.once("SIGTERM", () => server.close(() => process.exit(0)));
`;

function listen(server, port = 0) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, LOOPBACK, () => {
      const address = server.address();
      assert(address !== null && typeof address !== "string");
      resolve(address.port);
    });
  });
}

function close(server) {
  return new Promise((resolve, reject) => {
    server.close((error) => error ? reject(error) : resolve());
    server.closeAllConnections();
  });
}

async function unusedPort() {
  const reservation = createServer();
  const port = await listen(reservation);
  await close(reservation);
  return port;
}

async function foreignReadyServer() {
  const paths = [];
  const server = createServer((request, response) => {
    paths.push(request.url);
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ value: { ready: true } }));
  });
  const port = await listen(server);
  return { server, port, paths };
}

function fakeSpawner(markerStream, children) {
  return (_driver, arguments_, options) => {
    assert.equal(arguments_.length, 2);
    assert.match(arguments_[0], /^--port=[1-9][0-9]*$/u);
    assert.match(arguments_[1], /^--url-base=\/labcolors-[0-9a-f]{32}$/u);
    const child = spawn(
      process.execPath,
      ["--input-type=module", "--eval", FAKE_DRIVER, markerStream, ...arguments_],
      options,
    );
    children.push(child);
    return child;
  };
}

function tokenSequence() {
  let next = 0n;
  return () => (next++).toString(16).padStart(32, "0");
}

test("a bind collision is cleaned up and retried on a fresh port", async () => {
  const foreign = await foreignReadyServer();
  const secondPort = await unusedPort();
  const candidates = [foreign.port, secondPort];
  const children = [];
  const controller = new AbortController();
  let launched;
  try {
    launched = await launchDriver("unused", controller.signal, {
      reservePort: async () => candidates.shift(),
      spawnDriver: fakeSpawner("stderr", children),
      randomToken: tokenSequence(),
      attempts: 2,
      attemptTimeout: 2_000,
    });
    assert.equal(children.length, 2);
    assert.match(launched.origin, new RegExp(`:${secondPort}/labcolors-[0-9a-f]{32}$`, "u"));
    assert.equal(children[0].exitCode, 98);
    assert.deepEqual(foreign.paths, []);
  } finally {
    if (launched) await stopDriver(launched.child, launched.observation);
    await close(foreign.server);
  }
  assert(children.every((child) => child.exitCode !== null));
});

test("a ready foreign listener cannot impersonate the spawned child", async () => {
  const foreign = await foreignReadyServer();
  const children = [];
  try {
    await assert.rejects(
      launchDriver("unused", AbortSignal.timeout(5_000), {
        reservePort: async () => foreign.port,
        spawnDriver: fakeSpawner("stdout", children),
        randomToken: tokenSequence(),
        attempts: 1,
        attemptTimeout: 1_000,
      }),
      /exhausted 1 bounded start attempts/u,
    );
    assert.deepEqual(foreign.paths, []);
    assert.equal(children.length, 1);
    assert.equal(children[0].exitCode, 98);
  } finally {
    await close(foreign.server);
  }
});

test("bounded collision retries fail closed after exhaustion", async () => {
  const foreign = await foreignReadyServer();
  const children = [];
  try {
    await assert.rejects(
      launchDriver("unused", AbortSignal.timeout(5_000), {
        reservePort: async () => foreign.port,
        spawnDriver: fakeSpawner("stdout", children),
        randomToken: tokenSequence(),
        attempts: 3,
        attemptTimeout: 1_000,
      }),
      (error) => {
        assert.match(error.message, /exhausted 3 bounded start attempts/u);
        assert.match(error.message, /attempt 1\/3/u);
        assert.match(error.message, /attempt 2\/3/u);
        assert.match(error.message, /attempt 3\/3/u);
        return true;
      },
    );
    assert.equal(children.length, 3);
    assert(children.every((child) => child.exitCode === 98));
    assert.deepEqual(foreign.paths, []);
  } finally {
    await close(foreign.server);
  }
});

test("program result proof ignores transport key order but rejects drift", () => {
  const expected = {
    ready: {
      state: "ready",
      count: 1,
      slot: 91,
      rgb: [20, 20, 20],
      opacity: 1,
    },
    invalidRejected: true,
    recovered: {
      state: "ready",
      count: 1,
      slot: 91,
      rgb: [20, 20, 20],
      opacity: 1,
    },
  };
  const reordered = {
    invalidRejected: true,
    recovered: {
      opacity: 1,
      rgb: [20, 20, 20],
      slot: 91,
      count: 1,
      state: "ready",
    },
    ready: {
      rgb: [20, 20, 20],
      state: "ready",
      opacity: 1,
      slot: 91,
      count: 1,
    },
  };
  assertExactProgramResult(reordered, expected);

  for (const [label, mutate] of [
    ["missing field", (value) => {
      const copy = structuredClone(value);
      delete copy.recovered.opacity;
      return copy;
    }],
    ["extra field", (value) => ({ ...value, unexpected: true })],
    ["wrong scalar type", (value) => ({
      ...value,
      invalidRejected: 1,
    })],
    ["wrong array element", (value) => ({
      ...value,
      ready: { ...value.ready, rgb: [20, 20, 21] },
    })],
  ]) {
    assert.throws(
      () => assertExactProgramResult(mutate(expected), expected),
      /terminal Program result drifted/u,
      label,
    );
  }
});
