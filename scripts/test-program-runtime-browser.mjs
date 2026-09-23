import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { access, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

import { observeChildErrors, releaseChild, waitForDriver } from "./browser-child-lifecycle.mjs";
import { retainImportedRuntimeSnippets } from "./package-runtime-snippets.mjs";

const LOOPBACK = "127.0.0.1";
const REPO_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));
const SOURCE_TREE_DOMAIN = "labpics.colors/core-source-tree-descriptor/v1\0";
const RESOURCE_ORDER = [
  "temp-install",
  "browser",
  "browser-session",
  "server",
  "runtime",
  "snapshot",
  "host",
];

export class BrowserProofCleanupError extends Error {
  constructor(primary, cleanupErrors) {
    super("Program browser proof cleanup failed", { cause: primary });
    this.name = "BrowserProofCleanupError";
    this.code = "BROWSER_PROOF_CLEANUP_FAILED";
    this.cleanupErrors = cleanupErrors;
  }
}

function fail(message, options) {
  throw new Error(`Program browser proof: ${message}`, options);
}

function sourceEnvelopeIdentityAtCommit(sourceSha) {
  if (!/^[0-9a-f]{40}$/u.test(sourceSha ?? "")) return null;
  try {
    const revision = execFileSync(
      "git", ["rev-parse", `${sourceSha}:crates/labcolors-core`],
      { cwd: REPO_ROOT, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    ).trim();
    if (!/^[0-9a-f]{40}$/u.test(revision)) return null;
    const descriptor = Buffer.alloc(47);
    descriptor.write("LCST", 0, "ascii");
    descriptor.writeUInt16BE(1, 4);
    descriptor[6] = 1;
    descriptor.write(revision, 7, "ascii");
    return {
      revision,
      contentIdentityHex: createHash("sha256")
        .update(SOURCE_TREE_DOMAIN, "utf8")
        .update(descriptor)
        .digest("hex"),
    };
  } catch {
    return null;
  }
}

export async function cleanupResources(resources, primary) {
  const cleanupErrors = [];
  for (const resource of resources.toReversed()) {
    try {
      await resource.release();
    } catch (error) {
      cleanupErrors.push({ resource: resource.name, error });
    }
  }
  if (cleanupErrors.length > 0) throw new BrowserProofCleanupError(primary, cleanupErrors);
  if (primary !== undefined) throw primary;
}

export function browserCleanup(primary, resources) {
  const cleanupErrors = [];
  for (const resource of resources) {
    try {
      resource.release();
    } catch (error) {
      cleanupErrors.push({ resource: resource.name, error });
    }
  }
  const outcome = primary === undefined
    ? {}
    : { error: String(primary), code: primary?.code, operation: primary?.operation };
  if (cleanupErrors.length > 0) {
    outcome.cleanupError = {
      code: "BROWSER_PROOF_CLEANUP_FAILED",
      resources: cleanupErrors.map(({ resource }) => resource),
      errors: cleanupErrors.map(({ error }) => ({ message: String(error), code: error?.code })),
    };
  }
  return outcome;
}

function acquisition(evidence, name, fault) {
  evidence.acquired.push(name);
  if (fault?.after === name) {
    throw Object.assign(new Error(`fault after ${name}`), {
      code: "BROWSER_PROOF_INJECTED", operation: name,
    });
  }
}

function released(evidence, name, fault) {
  evidence.released.push(name);
  // Ошибка моделируется после настоящего освобождения, чтобы QA не оставлял утечки.
  if (fault?.cleanup === name) {
    throw Object.assign(new Error(`cleanup fault at ${name}`), { code: "BROWSER_PROOF_INJECTED_CLEANUP" });
  }
}

export async function verifyCleanupFaultMatrix(run) {
  if (typeof run !== "function") fail("fault matrix requires an actual-path runner");
  const reports = [];
  for (const after of RESOURCE_ORDER) {
    for (const cleanup of [undefined, after]) {
      const report = await run({ after, cleanup });
      const expected = RESOURCE_ORDER.slice(0, RESOURCE_ORDER.indexOf(after) + 1);
      if (report.error !== `Error: fault after ${after}` || report.code !== "BROWSER_PROOF_INJECTED" || report.operation !== after) {
        fail(`fault primary evidence drifted after ${after}: ${JSON.stringify(report)}`);
      }
      if (JSON.stringify(report.acquired) !== JSON.stringify(expected)
        || JSON.stringify(report.released) !== JSON.stringify(expected.toReversed())
        || expected.some((name) => report.readback?.[name] !== true)) {
        fail(`fault cleanup evidence drifted after ${after}: ${JSON.stringify(report)}`);
      }
      const cleanupError = report.cleanupError;
      if (cleanup === undefined ? cleanupError !== undefined
        : cleanupError?.code !== "BROWSER_PROOF_CLEANUP_FAILED"
          || JSON.stringify(cleanupError.resources) !== JSON.stringify([cleanup])
          || cleanupError.errors?.[0]?.code !== "BROWSER_PROOF_INJECTED_CLEANUP") {
        fail(`fault typed cleanup evidence drifted after ${after}`);
      }
      reports.push(report);
    }
  }
  return reports;
}

function positiveIntegerEnv(name) {
  const raw = process.env[name];
  if (!/^[1-9][0-9]*$/u.test(raw ?? "")) fail(`${name} must be a positive integer`);
  const value = Number(raw);
  if (!Number.isSafeInteger(value)) fail(`${name} exceeds the safe integer range`);
  return value;
}

async function executableEnv(name) {
  const value = process.env[name];
  if (!value || !isAbsolute(value)) fail(`${name} must name an absolute executable`);
  await access(value);
  if (!(await stat(value)).isFile()) fail(`${name} must name a regular file`);
  return value;
}

function parseArgs() {
  const args = process.argv.slice(2);
  if (args.length !== 2) fail("expected <tarball.tgz> <lowercase-sha256>");
  const tarball = resolve(args[0]);
  if (extname(tarball) !== ".tgz") fail("tarball must use the .tgz suffix");
  if (!/^[0-9a-f]{64}$/u.test(args[1])) fail("tarball digest must be lowercase SHA-256");
  return { tarball, digest: args[1] };
}

function npmInstall(tarball, root, timeout) {
  const npmExec = process.env.npm_execpath;
  const command = process.platform === "win32" && npmExec ? process.execPath : "npm";
  const prefix = process.platform === "win32" && npmExec ? [npmExec] : [];
  execFileSync(command, [...prefix, "install", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--no-package-lock", "--save=false", tarball], {
    cwd: root, stdio: ["ignore", "pipe", "pipe"], timeout,
  });
}

async function listen(server) {
  await new Promise((accept, reject) => {
    server.once("error", reject);
    server.listen(0, LOOPBACK, accept);
  });
  const address = server.address();
  if (address === null || typeof address === "string") fail("proof server has no TCP address");
  return address.port;
}

export async function reserveEphemeralPort() {
  const reservation = createServer();
  const port = await listen(reservation);
  await new Promise((accept, reject) => reservation.close((error) => error ? reject(error) : accept()));
  return port;
}

async function request(base, path, method, body, signal) {
  const response = await fetch(`${base}${path}`, {
    method, body: body === undefined ? undefined : JSON.stringify(body),
    headers: body === undefined ? undefined : { "content-type": "application/json" }, signal,
  });
  const payload = await response.json();
  if (!response.ok || payload?.value?.error) {
    fail(`WebDriver rejected ${method} ${path}: ${payload?.value?.message ?? response.status}`);
  }
  return payload.value;
}

export async function packedBrowserFiles(installed) {
  const paths = ["index.js", "package.json", "build-metadata.json", "program-wire/abi-v1.js", "pkg/labcolors.js", "pkg/labcolors_bg.wasm"];
  const runtimeSource = await readFile(join(installed, "pkg/labcolors.js"), "utf8");
  paths.push(...await retainImportedRuntimeSnippets(installed, runtimeSource));
  return new Map(await Promise.all(
    paths.map(async (path) => [`/${path}`, await readFile(join(installed, path))]),
  ));
}

export function browserScenario(origin, fault) {
  // В браузер передаются те же функции, которые исполняет regression, без второй cleanup-реализации.
  return `const done=arguments[arguments.length-1];
    const browserCleanup=${browserCleanup.toString()};
    const acquisition=${acquisition.toString()};
    const released=${released.toString()};
    (${browserConsumer.toString()})(${JSON.stringify(origin)},${JSON.stringify(fault)})
      .then(proof=>done({proof}),error=>done({proof:browserCleanup(error,[])}));`;
}

async function browserConsumer(origin, fault) {
  const resources = [];
  const evidence = { acquired: [], released: [], readback: {} };
  let primary, result, runtime, snapshot, element;
  try {
    const api = await import(`${origin}/index.js`);
    const wire = await import(`${origin}/program-wire/abi-v1.js`);
    await api.init({ module_or_path: fetch(`${origin}/pkg/labcolors_bg.wasm`) });
    const builder = new wire.ProgramWireBuilderV1();
    // Неравные RGB-каналы и неединичная opacity отличают чтение snapshot от прежних констант.
    builder.source(11, [12, 34, 56]).fixedTarget(21, 11).surfaceInputPort(31).opacityInput(32, 0.875)
      .solidPaint(41, 21).opacityPaint(42, 41, 32).inputSurface(51, 31)
      .sourceOverOccurrence(61, 42, 51, 64, .2, wire.SURROUND_AVERAGE_V1)
      .presentationRoot(71, 61).presentationTarget(71, 61)
      .wcag22VisibleUnary(true, 81, 61, wire.WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
      .exactVisibleUnary(false, 82, 61, [20, 20, 20]).output(91, 42);
    runtime = api.compileProgramWire(builder.finish(), 1);
    resources.push({ name: "runtime", release() { runtime.free(); released(evidence, "runtime", fault); } });
    acquisition(evidence, "runtime", fault);
    snapshot = runtime.updateObserved(2n, new Uint32Array([1]), new Uint8Array([255, 255, 255]), 1);
    resources.push({ name: "snapshot", release() { snapshot.free(); released(evidence, "snapshot", fault); } });
    acquisition(evidence, "snapshot", fault);
    const token = "consumer.foreground", slots = new Map([[token, 91]]);
    element = document.createElement("div");
    let hostDisposed = false;
    const host = { free() {
      if (hostDisposed) return;
      element.style.removeProperty("--consumer-color");
      element.remove();
      hostDisposed = true;
    } };
    resources.push({ name: "host", release() { host.free(); host.free(); released(evidence, "host", fault); } });
    element.style.color = "var(--consumer-color)";
    document.body.append(element);
    acquisition(evidence, "host", fault);
    const materialize = (s) => {
      let found = -1;
      for (let i = 0; i < s.outputCount(); i += 1) if (s.outputSlot(i) === slots.get(token)) found = i;
      if (found < 0) throw new Error("opaque consumer token was not materialized");
      const [r, g, b] = s.outputRgb(found);
      element.style.setProperty("--consumer-color", `rgb(${r} ${g} ${b} / ${s.outputOpacity(found)})`);
    };
    const rgba = (value) => {
      const channels = value.match(/[\d.]+/g)?.map(Number);
      if (!channels || channels.length < 3) throw new Error("computed color was not RGB");
      return [channels[0], channels[1], channels[2], channels[3] ?? 1];
    };
    const readSnapshot = () => ({
      state: snapshot.state,
      outputs: Array.from({ length: snapshot.outputCount() }, (_, i) => ({
        slot: snapshot.outputSlot(i), rgb: Array.from(snapshot.outputRgb(i)), opacity: snapshot.outputOpacity(i),
      })),
    });
    const rejectUpdate = (revision, ids, surfaces) => {
      try {
        const unexpected = runtime.updateObserved(revision, ids, surfaces, 1);
        unexpected.free();
        return { rejected: false };
      } catch (error) {
        return { rejected: api.isProgramError(error), code: error.code, operation: error.operation };
      }
    };
    materialize(snapshot);
    const before = rgba(getComputedStyle(element).color);
    const cssBefore = element.style.getPropertyValue("--consumer-color");
    const snapshotBefore = readSnapshot();
    const malformed = rejectUpdate(2n, new Uint32Array([]), new Uint8Array([]));
    const after = rgba(getComputedStyle(element).color);
    const cssAfter = element.style.getPropertyValue("--consumer-color");
    const snapshotAfter = readSnapshot();
    // Revision ниже принятой при валидном observation проверяет stale, не malformed input.
    const stale = rejectUpdate(1n, new Uint32Array([1]), new Uint8Array([255, 255, 255]));
    const afterStale = rgba(getComputedStyle(element).color);
    const cssAfterStale = element.style.getPropertyValue("--consumer-color");
    const snapshotAfterStale = readSnapshot();
    result = { before, after, afterStale, cssBefore, cssAfter, cssAfterStale,
      snapshotBefore, snapshotAfter, snapshotAfterStale, malformed, stale };
  } catch (error) {
    primary = error;
  }
  const outcome = browserCleanup(primary, resources.toReversed());
  // Проверяется инвалидированный handle точного wasm-bindgen artifact, не размер WASM heap.
  if (runtime) evidence.readback.runtime = runtime.__wbg_ptr === 0;
  if (snapshot) evidence.readback.snapshot = snapshot.__wbg_ptr === 0;
  if (element) evidence.readback.host = !element.isConnected && element.style.getPropertyValue("--consumer-color") === "";
  return { ...result, ...outcome, ...evidence };
}

export function browserAttachmentScenario(origin, fault) {
  return `const done=arguments[arguments.length-1];
    const browserCleanup=${browserCleanup.toString()};
    const acquisition=${acquisition.toString()};
    const released=${released.toString()};
    (${browserAttachmentConsumer.toString()})(${JSON.stringify(origin)},${JSON.stringify(fault)})
      .then(proof=>done({proof}),error=>done({proof:browserCleanup(error,[])}));`;
}

async function browserAttachmentConsumer(origin, fault) {
  const resources = [];
  const evidence = { acquired: [], released: [], readback: {} };
  let primary, result, attachment, snapshot, secondSnapshot, abstentionSnapshot, render, secondRender,
    authority, secondAuthority, preservedAuthority, element;
  let freeBeforeDispose, cssBeforeFree, cssAfterFree;
  let hostDisposed = false;
  let hostAcquired = false;
  try {
    const api = await import(`${origin}/index.js`);
    const wire = await import(`${origin}/program-wire/abi-v1.js`);
    await api.init({ module_or_path: fetch(`${origin}/pkg/labcolors_bg.wasm`) });
    const buildMetadataResponse = await fetch(`${origin}/build-metadata.json`, { cache: "no-store" });
    if (!buildMetadataResponse.ok) throw new Error("installed producer metadata was unavailable");
    const buildMetadata = await buildMetadataResponse.json();
    const sourceEnvelope = api.decodeCertificateEnvelope(api.issueSourceCertificateEnvelope());
    const producerTuple = {
      package: buildMetadata.package,
      sourceSha: buildMetadata.sourceSha,
      runtime: buildMetadata.wasm?.find((entry) => entry.role === "runtime"),
      envelope: {
        runtimeArtifactId: sourceEnvelope.runtimeArtifactId,
        operation: sourceEnvelope.operation,
        authorityKind: sourceEnvelope.authorityKind,
        authorityVersion: sourceEnvelope.authorityVersion,
        producerRevision: sourceEnvelope.producerRevision,
        producerContentIdentity: Array.from(sourceEnvelope.producerContentIdentity),
        contextId: sourceEnvelope.contextId,
        payloadType: sourceEnvelope.payloadType,
        payloadVersion: sourceEnvelope.payloadVersion,
      },
    };
    const token = Object.freeze({
      id: "consumer.foreground",
      outputSlot: 17,
      sinkOutput: 91,
      cssProperty: "--consumer-color",
    });
    const builder = new wire.ProgramWireBuilderV1();
    builder.source(1, [64, 64, 64]).fixedTarget(2, 1).surfaceInputPort(6).opacityInput(5, 0.5)
      .solidPaint(3, 2).opacityPaint(4, 3, 5).inputSurface(7, 6)
      .sourceOverOccurrence(8, 4, 7, 64, .2, wire.SURROUND_AVERAGE_V1)
      .presentationRoot(9, 8).presentationTarget(9, 8)
      .exactVisibleUnary(false, 10, 8, [96, 96, 96]).output(token.outputSlot, 4);
    element = document.createElement("div");
    element.style.background = "rgb(128 128 128)";
    element.style.color = `var(${token.cssProperty})`;
    document.body.append(element);
    const hostState = { sequence: 0n, epoch: null, point: null, intents: [], reject: false };
    const host = (intent) => {
      if (hostState.reenter) {
        hostState.reenter = false;
        try {
          attachment.updateObserved(99n, new Uint32Array([1]), new Uint8Array([160, 160, 160]), 1);
          hostState.reentrant = { rejected: false };
        } catch (error) {
          hostState.reentrant = {
            rejected: true,
            code: error.code,
            operation: error.operation,
            recognized: api.isProgramError(error),
          };
        }
        try {
          attachment.free();
          hostState.reentrantFree = { rejected: false };
        } catch (error) {
          hostState.reentrantFree = {
            rejected: true,
            code: error.code,
            operation: error.operation,
            recognized: api.isProgramError(error),
          };
        }
        return false;
      }
      if (hostState.reject) return false;
      if (intent.sinkOutput !== token.sinkOutput
        || intent.expectedSequence !== hostState.sequence
        || (hostState.epoch !== null && intent.bindingEpoch !== hostState.epoch)) return false;
      const observedPoint = intent.point === null ? null : {
        slot: intent.point.slot, source: Array.from(intent.point.source), opacity: intent.point.opacity,
      };
      let nextPoint = hostState.point;
      if (intent.operation === "setAll") {
        if (intent.point === null || intent.point.slot !== token.outputSlot
          || intent.point.source.length !== 3 || intent.point.opacity !== 0.5) return false;
        nextPoint = observedPoint;
      } else if (intent.operation === "revokeAll") {
        if (intent.point !== null) return false;
        nextPoint = null;
      } else if (intent.operation === "confirmExact") {
        if (intent.desiredSequence !== intent.expectedSequence) return false;
      } else return false;
      // Все проверки выполняются до одной DOM-мутации и локальной смены stamp.
      if (intent.operation === "revokeAll") element.style.removeProperty(token.cssProperty);
      else if (intent.point !== null) {
        const [r, g, b] = intent.point.source;
        element.style.setProperty(token.cssProperty, `rgb(${r} ${g} ${b} / ${intent.point.opacity})`);
      }
      hostState.sequence = intent.desiredSequence;
      hostState.epoch = intent.bindingEpoch;
      hostState.point = nextPoint;
      hostState.intents.push({
        operation: intent.operation,
        revision: intent.revision.toString(),
        expectedSequence: intent.expectedSequence.toString(),
        desiredSequence: intent.desiredSequence.toString(),
        bindingEpoch: intent.bindingEpoch.toString(),
        sinkOutput: intent.sinkOutput,
        point: observedPoint,
      });
      return true;
    };
    const hostResource = {
      name: "attachment-host",
      release() {
        if (hostDisposed) return;
        element.style.removeProperty(token.cssProperty);
        element.style.removeProperty("color");
        element.remove();
        hostDisposed = true;
        if (hostAcquired) released(evidence, "attachment-host", fault);
      },
    };
    try {
      attachment = api.attachProgramWire(builder.finish(), 7, 17, 91, 9, 8, host);
    } catch (error) {
      hostResource.release();
      throw error;
    }
    resources.push({
      name: "attachment",
      release() {
        try {
          // Внешний владелец отзывает свой DOM scope до подтверждения освобождения.
          // Fault-инъекции относятся к проверенному update, а не к cleanup revoke.
          hostState.reenter = false;
          hostState.reject = false;
          hostResource.release();
          attachment.dispose(true);
        } finally {
          attachment.free();
          released(evidence, "attachment", fault);
        }
      },
    });
    acquisition(evidence, "attachment", fault);
    resources.push(hostResource);
    hostAcquired = true;
    acquisition(evidence, "attachment-host", fault);
    snapshot = attachment.updateObserved(1n, new Uint32Array([1]), new Uint8Array([128, 128, 128]), 1);
    const firstStatus = {
      state: snapshot.state,
      hasRender: snapshot.hasRender(),
    };
    resources.push({ name: "attachment-snapshot", release() { snapshot.free(); released(evidence, "attachment-snapshot", fault); } });
    acquisition(evidence, "attachment-snapshot", fault);
    render = snapshot.render();
    resources.push({ name: "attachment-render", release() { render?.free(); released(evidence, "attachment-render", fault); } });
    acquisition(evidence, "attachment-render", fault);
    authority = attachment.materializationAuthority();
    resources.push({ name: "attachment-authority", release() { authority.free(); released(evidence, "attachment-authority", fault); } });
    acquisition(evidence, "attachment-authority", fault);
    cssBeforeFree = element.style.getPropertyValue(token.cssProperty);
    try {
      attachment.free();
      freeBeforeDispose = { rejected: false };
    } catch (error) {
      freeBeforeDispose = {
        rejected: true,
        code: error.code,
        operation: error.operation,
        recognized: api.isProgramError(error),
      };
    }
    cssAfterFree = element.style.getPropertyValue(token.cssProperty);
    element.style.background = "rgb(160 160 160)";
    secondSnapshot = attachment.updateObserved(2n, new Uint32Array([1]), new Uint8Array([160, 160, 160]), 1);
    resources.push({ name: "attachment-second-snapshot", release() { secondSnapshot.free(); released(evidence, "attachment-second-snapshot", fault); } });
    acquisition(evidence, "attachment-second-snapshot", fault);
    const secondStatus = {
      state: secondSnapshot.state,
      hasRender: secondSnapshot.hasRender(),
    };
    secondRender = secondSnapshot.render();
    resources.push({ name: "attachment-second-render", release() { secondRender?.free(); released(evidence, "attachment-second-render", fault); } });
    acquisition(evidence, "attachment-second-render", fault);
    secondAuthority = attachment.materializationAuthority();
    resources.push({ name: "attachment-second-authority", release() { secondAuthority.free(); released(evidence, "attachment-second-authority", fault); } });
    acquisition(evidence, "attachment-second-authority", fault);
    let reentrant;
    hostState.reenter = true;
    try {
      attachment.updateObserved(3n, new Uint32Array([1]), new Uint8Array([160, 160, 160]), 1);
      reentrant = { rejected: false };
    } catch (error) {
      reentrant = {
        rejected: true,
        code: error.code,
        operation: error.operation,
        recognized: api.isProgramError(error),
      };
    }
    hostState.reject = true;
    let hostRejection;
    const cssBefore = element.style.getPropertyValue(token.cssProperty);
    try {
      attachment.updateObserved(3n, new Uint32Array([1]), new Uint8Array([160, 160, 160]), 1);
      hostRejection = { rejected: false };
    } catch (error) {
      hostRejection = {
        rejected: true,
        code: error.code,
        operation: error.operation,
        recognized: api.isProgramError(error),
      };
    }
    preservedAuthority = attachment.materializationAuthority();
    resources.push({ name: "attachment-preserved-authority", release() { preservedAuthority.free(); released(evidence, "attachment-preserved-authority", fault); } });
    acquisition(evidence, "attachment-preserved-authority", fault);
    let stale;
    try {
      attachment.materializationAuthorityFor(
        1n,
        authority.contentIdentity(),
        authority.bindingEpoch(),
      );
      stale = { rejected: false };
    } catch (error) {
      stale = {
        rejected: true,
        code: error.code,
        operation: error.operation,
        recognized: api.isProgramError(error),
      };
    }
    let staleIdentity;
    try {
      attachment.materializationAuthorityFor(
        2n,
        new Uint8Array(32).fill(0xA5),
        secondAuthority.bindingEpoch(),
      );
      staleIdentity = { rejected: false };
    } catch (error) {
      staleIdentity = {
        rejected: true,
        code: error.code,
        operation: error.operation,
        recognized: api.isProgramError(error),
      };
    }
    let foreignEpoch;
    try {
      attachment.materializationAuthorityFor(
        2n,
        secondAuthority.contentIdentity(),
        BigInt(secondAuthority.bindingEpoch()) + 1n,
      );
      foreignEpoch = { rejected: false };
    } catch (error) {
      foreignEpoch = {
        rejected: true,
        code: error.code,
        operation: error.operation,
        recognized: api.isProgramError(error),
      };
    }
    const independentComposite = (backdrop, source, opacity) => backdrop.map((channel, index) =>
      Math.round(channel + opacity * (source[index] - channel)));
    const parseComputed = (value) => {
      const channels = value.match(/[\d.]+/g)?.map(Number);
      if (!channels || channels.length < 3) throw new Error("computed color was not RGB");
      return [channels[0], channels[1], channels[2], channels[3] ?? 1];
    };
    const computed = parseComputed(getComputedStyle(element).color);
    hostState.reject = false;
    const materializedBeforeAbstention = element.style.getPropertyValue(token.cssProperty);
    abstentionSnapshot = attachment.updateUnknown(3n, 0xA11CE);
    resources.push({
      name: "attachment-abstention-snapshot",
      release() { abstentionSnapshot.free(); released(evidence, "attachment-abstention-snapshot", fault); },
    });
    acquisition(evidence, "attachment-abstention-snapshot", fault);
    let abstentionAuthority;
    try {
      const unexpected = attachment.materializationAuthority();
      unexpected.free();
      abstentionAuthority = { rejected: false };
    } catch (error) {
      abstentionAuthority = {
        rejected: true,
        code: error.code,
        operation: error.operation,
        recognized: api.isProgramError(error),
      };
    }
    const materializedAfterAbstention = element.style.getPropertyValue(token.cssProperty);
    result = {
      // Cleanup revokeAll — lifecycle readback, не часть двух update oracle.
      hostIntents: hostState.intents.slice(),
      declaredConsumer: {
        token,
        producerTuple,
        values: {
          materialized: materializedBeforeAbstention,
          computed,
        },
        abstention: {
          state: abstentionSnapshot.state,
          hasRender: abstentionSnapshot.hasRender(),
          outputCount: abstentionSnapshot.outputCount(),
          before: materializedBeforeAbstention,
          after: materializedAfterAbstention,
          authority: abstentionAuthority,
        },
        errors: {
          rejectedUpdate: hostRejection,
          staleMaterialization: stale,
        },
      },
      first: {
        ...firstStatus,
        output: Array.from(snapshot.outputRgb(0)),
        opacity: snapshot.outputOpacity(0),
        renderComposite: Array.from(render.terminalCompositeRgb() ?? []),
        renderRevision: render.revision().toString(),
        renderSinkSequence: render.sinkSequence().toString(),
        renderBindingEpoch: render.bindingEpoch().toString(),
        renderContentIdentity: Array.from(render.contentIdentity()),
        renderContextIdentity: Array.from(render.contextIdentity()),
        renderRoot: render.presentationRoot(),
        renderOccurrence: render.occurrence(),
        renderPhysicalIdentity: render.physicalIdentity(),
        authorityComposite: Array.from(authority.terminalCompositeRgb()),
        authorityRevision: authority.revision().toString(),
        authoritySinkSequence: authority.sinkSequence().toString(),
        authorityBindingEpoch: authority.bindingEpoch().toString(),
        authorityContentIdentity: Array.from(authority.contentIdentity()),
        authorityContextIdentity: Array.from(authority.contextIdentity()),
        authorityRoot: authority.presentationRoot(),
        authorityOccurrence: authority.occurrence(),
        authorityPhysicalIdentity: authority.physicalIdentity(),
        rendererProvenance: authority.rendererProvenance(),
      },
      second: {
        state: secondSnapshot.state,
        hasRender: secondSnapshot.hasRender(),
        renderComposite: Array.from(secondRender.terminalCompositeRgb() ?? []),
        renderRevision: secondRender.revision().toString(),
        renderSinkSequence: secondRender.sinkSequence().toString(),
        renderBindingEpoch: secondRender.bindingEpoch().toString(),
        renderContentIdentity: Array.from(secondRender.contentIdentity()),
        renderContextIdentity: Array.from(secondRender.contextIdentity()),
        renderRoot: secondRender.presentationRoot(),
        renderOccurrence: secondRender.occurrence(),
        renderPhysicalIdentity: secondRender.physicalIdentity(),
        authorityComposite: Array.from(secondAuthority.terminalCompositeRgb()),
        authorityRevision: secondAuthority.revision().toString(),
        authoritySinkSequence: secondAuthority.sinkSequence().toString(),
        authorityBindingEpoch: secondAuthority.bindingEpoch().toString(),
        authorityContentIdentity: Array.from(secondAuthority.contentIdentity()),
        authorityContextIdentity: Array.from(secondAuthority.contextIdentity()),
        authorityRoot: secondAuthority.presentationRoot(),
        authorityOccurrence: secondAuthority.occurrence(),
        authorityPhysicalIdentity: secondAuthority.physicalIdentity(),
        rendererProvenance: secondAuthority.rendererProvenance(),
      },
      computed,
      independentFirstComposite: independentComposite([128, 128, 128], [64, 64, 64], 0.5),
      independentSecondComposite: independentComposite([160, 160, 160], [64, 64, 64], 0.5),
      stale,
      staleSnapshotHasAuthority: typeof snapshot.materializationAuthority === "function",
      cssBefore,
      staleIdentity,
      foreignEpoch,
      cssAfterForeignEpoch: element.style.getPropertyValue(token.cssProperty),
      reentrant: { outer: reentrant, nested: hostState.reentrant },
      reentrantFree: hostState.reentrantFree,
      freeBeforeDispose,
      cssBeforeFree,
      cssAfterFree,
      hostRejection,
      preservedAuthorityComposite: Array.from(preservedAuthority.terminalCompositeRgb()),
      cssAfterRejection: element.style.getPropertyValue(token.cssProperty),
    };
  } catch (error) {
    primary = error;
    result = {};
  }
  const outcome = browserCleanup(primary, resources.toReversed());
  if (attachment) evidence.readback.attachment = attachment.__wbg_ptr === 0;
  if (snapshot) evidence.readback["attachment-snapshot"] = snapshot.__wbg_ptr === 0;
  if (render) evidence.readback["attachment-render"] = render.__wbg_ptr === 0;
  if (authority) evidence.readback["attachment-authority"] = authority.__wbg_ptr === 0;
  if (secondSnapshot) evidence.readback["attachment-second-snapshot"] = secondSnapshot.__wbg_ptr === 0;
  if (abstentionSnapshot) evidence.readback["attachment-abstention-snapshot"] = abstentionSnapshot.__wbg_ptr === 0;
  if (secondRender) evidence.readback["attachment-second-render"] = secondRender.__wbg_ptr === 0;
  if (secondAuthority) evidence.readback["attachment-second-authority"] = secondAuthority.__wbg_ptr === 0;
  if (preservedAuthority) evidence.readback["attachment-preserved-authority"] = preservedAuthority.__wbg_ptr === 0;
  if (element) evidence.readback["attachment-host"] = !element.isConnected
    && element.style.getPropertyValue("color") === ""
    && element.style.getPropertyValue("--consumer-color") === "";
  return { ...result, ...outcome, ...evidence };
}

export async function runBrowserAttachmentProof(options) {
  return runBrowserProof({ ...options, scenario: browserAttachmentScenario }, {});
}

export async function runBrowserProof({ tarball, timeout, chrome, driver, scenario = browserScenario }, fault) {
  const resources = [];
  const evidence = { acquired: [], released: [], readback: {} };
  let primary, result, root, child, childErrors, server, base;
  let timer;
  try {
    root = await mkdtemp(join(tmpdir(), "labcolors-program-browser-"));
    resources.push({ name: "temp-install", release: async () => {
      await rm(root, { recursive: true, force: true });
      released(evidence, "temp-install", fault);
    } });
    acquisition(evidence, "temp-install", fault);
    await writeFile(join(root, "package.json"), '{"private":true,"type":"module"}\n');
    npmInstall(tarball, root, timeout);
    const driverPort = await reserveEphemeralPort();
    child = spawn(driver, [`--port=${driverPort}`], { env: process.env, stdio: "ignore", windowsHide: true });
    childErrors = observeChildErrors(child);
    resources.push({
      name: "browser",
      release: async () => {
        await releaseChild(child, childErrors, 2_000);
        released(evidence, "browser", fault);
      },
    });
    acquisition(evidence, "browser", fault);
    const controller = new AbortController();
    timer = setTimeout(() => controller.abort(new Error("browser proof timed out")), timeout);
    await waitForDriver(controller.signal, driverPort, child, childErrors);
    const session = await request(`http://${LOOPBACK}:${driverPort}`, "/session", "POST", { capabilities: { alwaysMatch: { browserName: "chrome", "goog:chromeOptions": { binary: chrome, args: ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"] } } } }, controller.signal);
    base = `http://${LOOPBACK}:${driverPort}/session/${session.sessionId}`;
    resources.push({ name: "browser-session", release: async () => {
      await request(base, "", "DELETE", undefined, AbortSignal.timeout(5_000));
      const response = await fetch(`${base}/url`, { signal: AbortSignal.timeout(5_000) });
      evidence.readback["browser-session"] = (await response.json())?.value?.error === "invalid session id";
      released(evidence, "browser-session", fault);
    } });
    acquisition(evidence, "browser-session", fault);
    const installed = join(root, "node_modules", "@labpics", "colors");
    const files = await packedBrowserFiles(installed);
    const packageBytes = files.get("/package.json");
    const runtimeBytes = files.get("/pkg/labcolors_bg.wasm");
    if (!packageBytes || !runtimeBytes) fail("served package evidence is incomplete");
    const servedPackage = JSON.parse(packageBytes.toString("utf8"));
    evidence.servedArtifact = {
      package: { name: servedPackage.name, version: servedPackage.version },
      runtime: {
        path: "pkg/labcolors_bg.wasm",
        bytes: runtimeBytes.length,
        sha256: createHash("sha256").update(runtimeBytes).digest("hex"),
      },
    };
    server = createServer((req, res) => {
      const pathname = new URL(req.url ?? "/", `http://${LOOPBACK}`).pathname;
      if (pathname === "/") { res.writeHead(200, { "content-type": "text/html; charset=utf-8" }); res.end("<!doctype html><style>#proof{color:var(--consumer-color)}</style><div id=proof></div>"); return; }
      const bytes = files.get(pathname);
      if (!bytes) { res.writeHead(404).end(); return; }
      const contentType = pathname.endsWith(".wasm")
        ? "application/wasm"
        : pathname.endsWith(".json") ? "application/json; charset=utf-8" : "text/javascript; charset=utf-8";
      res.writeHead(200, { "cache-control": "no-store", "content-type": contentType, "x-content-type-options": "nosniff" }); res.end(bytes);
    });
    resources.push({ name: "server", release: async () => {
      if (server.listening) {
        server.closeAllConnections();
        await new Promise((accept, reject) => server.close((error) => error ? reject(error) : accept()));
      }
      released(evidence, "server", fault);
    } });
    const port = await listen(server);
    acquisition(evidence, "server", fault);
    const origin = `http://${LOOPBACK}:${port}`;
    await request(base, "/url", "POST", { url: origin }, controller.signal);
    const response = await request(base, "/execute/async", "POST", { script: scenario(origin, fault), args: [] }, controller.signal);
    result = response.proof;
    evidence.acquired.push(...result.acquired);
    evidence.released.push(...result.released);
    Object.assign(evidence.readback, result.readback);
  } catch (error) {
    primary = error;
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
  let outcome = {};
  try {
    await cleanupResources(resources, primary);
  } catch (error) {
    if (error instanceof BrowserProofCleanupError) {
      outcome = browserCleanup(error.cause, []);
      outcome.cleanupError = {
        code: error.code,
        resources: error.cleanupErrors.map(({ resource }) => resource),
        errors: error.cleanupErrors.map(({ error: failure }) => ({ message: String(failure), code: failure?.code })),
      };
    } else {
      outcome = browserCleanup(error, []);
    }
  }
  if (root) {
    try { await stat(root); evidence.readback["temp-install"] = false; }
    catch (error) {
      if (error.code !== "ENOENT") throw error;
      evidence.readback["temp-install"] = true;
    }
  }
  if (child) {
    let absent = false;
    if (child.pid !== undefined) {
      try { process.kill(child.pid, 0); }
      catch (error) { if (error.code !== "ESRCH") throw error; absent = true; }
    }
    evidence.readback.browser = absent && childErrors.closed && child.listenerCount("error") === 0;
  }
  if (server) evidence.readback.server = !server.listening && server.address() === null;
  // Ошибки двух execution realms объединяются, а не перекрываются object spread.
  if (result?.cleanupError && outcome.cleanupError) {
    outcome.cleanupError.resources.unshift(...result.cleanupError.resources);
    outcome.cleanupError.errors.unshift(...result.cleanupError.errors);
  }
  return { ...result, ...outcome, ...evidence };
}

export function verifyBrowserConsumer(result) {
  const equal = isDeepStrictEqual;
  const expectedSnapshot = { state: "ready", outputs: [{ slot: 91, rgb: [12, 34, 56], opacity: 0.875 }] };
  if (result.error || result.cleanupError
    || ![result.malformed, result.stale].every((error) => error?.rejected === true
      && error.code === "program_update" && error.operation === "updateObserved")
    || ![result.before, result.after, result.afterStale].every((color) => equal(color, [12, 34, 56, 0.875]))
    || ![result.snapshotBefore, result.snapshotAfter, result.snapshotAfterStale].every((value) => equal(value, expectedSnapshot))
    || typeof result.cssBefore !== "string" || result.cssBefore === ""
    || result.cssAfter !== result.cssBefore || result.cssAfterStale !== result.cssBefore) {
    fail(`browser consumer result drifted: ${JSON.stringify(result)}`);
  }
}

export function verifyBrowserAttachmentConsumer(result) {
  const equal = isDeepStrictEqual;
  const intents = result.hostIntents ?? [];
  const resources = [
    "temp-install", "browser", "browser-session", "server", "attachment", "attachment-host",
    "attachment-snapshot", "attachment-render", "attachment-authority", "attachment-second-snapshot",
    "attachment-second-render", "attachment-second-authority", "attachment-preserved-authority",
    "attachment-abstention-snapshot",
  ];
  const consumer = result.declaredConsumer;
  const tuple = consumer?.producerTuple;
  const envelope = tuple?.envelope;
  const contentIdentity = envelope?.producerContentIdentity;
  const contentIdentityHex = Array.isArray(contentIdentity)
    ? contentIdentity.map((byte) => byte.toString(16).padStart(2, "0")).join("")
    : "";
  const sourceProfile = sourceEnvelopeIdentityAtCommit(tuple?.sourceSha);
  const servedArtifact = result.servedArtifact;
  if (result.error || result.cleanupError
    || consumer?.token?.id !== "consumer.foreground"
    || consumer.token.outputSlot !== 17
    || consumer.token.sinkOutput !== 91
    || consumer.token.cssProperty !== "--consumer-color"
    || tuple?.package?.name !== "@labpics/colors"
    || typeof tuple.package.version !== "string" || tuple.package.version.length === 0
    || !/^[0-9a-f]{40}$/u.test(tuple?.sourceSha ?? "")
    || tuple?.runtime?.role !== "runtime"
    || tuple.runtime.path !== "pkg/labcolors_bg.wasm"
    || !Number.isSafeInteger(tuple.runtime.bytes) || tuple.runtime.bytes <= 0
    || !/^[0-9a-f]{64}$/u.test(tuple.runtime.sha256 ?? "")
    || servedArtifact?.package?.name !== tuple.package.name
    || servedArtifact.package.version !== tuple.package.version
    || servedArtifact?.runtime?.path !== tuple.runtime.path
    || servedArtifact.runtime.bytes !== tuple.runtime.bytes
    || servedArtifact.runtime.sha256 !== tuple.runtime.sha256
    || envelope?.operation !== "issue-certificate"
    || envelope.authorityKind !== "generic-typed-certificate"
    || envelope.authorityVersion !== 1
    || !/^[0-9a-f]{40}$/u.test(envelope.producerRevision ?? "")
    || !Array.isArray(contentIdentity) || contentIdentity.length !== 32
    || contentIdentity.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255)
    || sourceProfile?.revision !== envelope.producerRevision
    || sourceProfile.contentIdentityHex !== contentIdentityHex
    || envelope.runtimeArtifactId !== `labcolors-core:source-tree-v1:${contentIdentityHex}`
    || envelope.contextId !== "core-source-tree-transport-v1"
    || envelope.payloadType !== "non-semantic-transport-v1"
    || envelope.payloadVersion !== 1
    || typeof consumer.values?.materialized !== "string" || consumer.values.materialized === ""
    || !equal(consumer.values.computed, [64, 64, 64, 0.5])
    || consumer.abstention?.state !== "stale"
    || consumer.abstention.hasRender !== false
    || consumer.abstention.before !== consumer.values.materialized
    || consumer.abstention.after !== ""
    || consumer.abstention.authority?.rejected !== true
    || consumer.abstention.authority.recognized !== true
    || consumer.abstention.authority.code !== "program_materialization_not_ready"
    || consumer.abstention.authority.operation !== "materializationAuthority"
    || consumer.errors?.rejectedUpdate?.rejected !== true
    || consumer.errors.rejectedUpdate.code !== "program_attachment_host_rejected"
    || consumer.errors.rejectedUpdate.operation !== "attachmentUpdateObserved"
    || consumer.errors.rejectedUpdate.recognized !== true
    || consumer.errors?.staleMaterialization?.rejected !== true
    || consumer.errors.staleMaterialization.code !== "program_materialization_stale_revision"
    || consumer.errors.staleMaterialization.operation !== "materializationAuthority"
    || consumer.errors.staleMaterialization.recognized !== true
    || !equal(result.first?.output, [64, 64, 64])
    || result.first?.opacity !== 0.5
    || !equal(result.first?.renderComposite, [96, 96, 96])
    || !equal(result.first?.authorityComposite, [96, 96, 96])
    || result.first?.rendererProvenance !== "unverified"
    || result.first?.renderRevision !== "1"
    || result.first?.renderSinkSequence !== "1"
    || result.first?.renderBindingEpoch === "0"
    || result.first?.authorityRevision !== "1"
    || result.first?.authoritySinkSequence !== "1"
    || result.first?.authorityBindingEpoch !== result.first?.renderBindingEpoch
    || !equal(result.first?.renderContentIdentity, result.first?.authorityContentIdentity)
    || result.first?.renderContentIdentity?.length !== 32
    || !equal(result.first?.renderContextIdentity, result.first?.authorityContextIdentity)
    || result.first?.renderContextIdentity?.length !== 32
    || result.first?.renderRoot !== 9 || result.first?.authorityRoot !== 9
    || result.first?.renderOccurrence !== 8 || result.first?.authorityOccurrence !== 8
    || result.first?.renderPhysicalIdentity !== "unknown"
    || result.first?.authorityPhysicalIdentity !== "unknown"
    || result.first?.state !== "ready"
    || result.first?.hasRender !== true
    || !equal(result.second?.renderComposite, [112, 112, 112])
    || !equal(result.second?.authorityComposite, [112, 112, 112])
    || result.second?.renderRevision !== "2"
    || result.second?.renderSinkSequence !== "2"
    || result.second?.renderBindingEpoch !== result.first?.renderBindingEpoch
    || !equal(result.second?.renderContentIdentity, result.first?.renderContentIdentity)
    || !equal(result.second?.renderContextIdentity, result.first?.renderContextIdentity)
    || result.second?.renderRoot !== 9 || result.second?.authorityRoot !== 9
    || result.second?.renderOccurrence !== 8 || result.second?.authorityOccurrence !== 8
    || result.second?.renderPhysicalIdentity !== "unknown"
    || result.second?.authorityPhysicalIdentity !== "unknown"
    || result.second?.authorityRevision !== "2"
    || result.second?.authoritySinkSequence !== "2"
    || result.second?.authorityBindingEpoch !== result.second?.renderBindingEpoch
    || !equal(result.second?.authorityContentIdentity, result.second?.renderContentIdentity)
    || !equal(result.second?.authorityContextIdentity, result.second?.renderContextIdentity)
    || result.second?.state !== "ready"
    || result.second?.hasRender !== true
    || !equal(result.computed, [64, 64, 64, 0.5])
    || !equal(result.independentFirstComposite, [96, 96, 96])
    || !equal(result.independentSecondComposite, [112, 112, 112])
    || result.stale?.rejected !== true
    || result.stale.recognized !== true
    || result.stale.code !== "program_materialization_stale_revision"
    || result.stale.operation !== "materializationAuthority"
    || result.staleIdentity?.rejected !== true
    || result.staleIdentity.recognized !== true
    || result.staleIdentity.code !== "program_materialization_stale_identity"
    || result.staleIdentity.operation !== "materializationAuthority"
    || result.foreignEpoch?.rejected !== true
    || result.foreignEpoch.recognized !== true
    || result.foreignEpoch.code !== "program_materialization_foreign_binding_epoch"
    || result.foreignEpoch.operation !== "materializationAuthority"
    || result.cssAfterForeignEpoch !== result.cssBefore
    || result.reentrant?.outer?.rejected !== true
    || result.reentrant.outer.code !== "program_attachment_host_rejected"
    || result.reentrant.outer.operation !== "attachmentUpdateObserved"
    || result.reentrant.outer.recognized !== true
    || result.reentrant?.nested?.rejected !== true
    || result.reentrant.nested.code !== "program_attachment_busy"
    || result.reentrant.nested.operation !== "attachmentUpdateObserved"
    || result.reentrant.nested.recognized !== true
    || result.reentrantFree?.rejected !== true
    || result.reentrantFree.recognized !== true
    || result.reentrantFree.code !== "program_attachment_busy"
    || result.reentrantFree.operation !== "attachmentFree"
    || result.freeBeforeDispose?.rejected !== true
    || result.freeBeforeDispose.code !== "program_attachment_revoke_unconfirmed"
    || result.freeBeforeDispose.operation !== "attachmentFree"
    || result.freeBeforeDispose.recognized !== true
    || result.cssAfterFree !== result.cssBeforeFree
    || typeof result.cssBeforeFree !== "string" || result.cssBeforeFree === ""
    || result.second?.rendererProvenance !== "unverified"
    || result.staleSnapshotHasAuthority !== false
    || result.hostRejection?.rejected !== true
    || result.hostRejection.recognized !== true
    || result.hostRejection.code !== "program_attachment_host_rejected"
    || result.hostRejection.operation !== "attachmentUpdateObserved"
    || !equal(result.preservedAuthorityComposite, [112, 112, 112])
    || result.cssAfterRejection !== result.cssBefore
    || typeof result.cssBefore !== "string" || result.cssBefore === ""
    || intents.length !== 3
    || intents.slice(0, 2).some((intent) => intent.operation !== "setAll" || intent.sinkOutput !== 91
      || intent.point?.slot !== 17 || intent.point?.opacity !== 0.5)
    || intents[2]?.operation !== "revokeAll"
    || intents[2]?.sinkOutput !== 91
    || intents[2]?.point !== null
    || intents[0]?.expectedSequence !== "0"
    || intents[0]?.desiredSequence !== "1"
    || intents[1]?.expectedSequence !== "1"
    || intents[1]?.desiredSequence !== "2"
    || intents[2]?.expectedSequence !== "2"
    || intents[2]?.desiredSequence !== "3"
    || intents[0]?.bindingEpoch === "0"
    || intents[0]?.bindingEpoch !== intents[1]?.bindingEpoch
    || intents[1]?.bindingEpoch !== intents[2]?.bindingEpoch
    || result.readback?.["attachment-host"] !== true
    || result.readback?.attachment !== true
    || result.readback?.["attachment-snapshot"] !== true
    || result.readback?.["attachment-second-snapshot"] !== true
    || result.readback?.["attachment-preserved-authority"] !== true
    || !equal(result.acquired, resources)
    || !equal(result.released, resources.toReversed())
    || resources.some((name) => result.readback?.[name] !== true)) {
    fail(`browser attachment consumer result drifted: ${JSON.stringify(result)}`);
  }
}

async function main() {
  const { tarball, digest } = parseArgs();
  const timeout = positiveIntegerEnv("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS");
  const [chrome, driver] = await Promise.all([executableEnv("CHROME_PATH"), executableEnv("CHROMEDRIVER_PATH")]);
  const actualDigest = createHash("sha256").update(await readFile(tarball)).digest("hex");
  if (actualDigest !== digest) fail(`tarball digest mismatch: ${actualDigest}`);
  const run = (fault) => runBrowserProof({ tarball, timeout, chrome, driver }, fault);
  const result = await run();
  verifyBrowserConsumer(result);
  if (JSON.stringify(result.acquired) !== JSON.stringify(RESOURCE_ORDER)
    || JSON.stringify(result.released) !== JSON.stringify(RESOURCE_ORDER.toReversed())
    || RESOURCE_ORDER.some((name) => result.readback[name] !== true)) {
    fail(`browser consumer result drifted: ${JSON.stringify(result)}`);
  }
  console.log(`LAB_COLORS_PROGRAM_BROWSER_RESULT ${JSON.stringify(result)}`);
  const attachmentResult = await runBrowserAttachmentProof({ tarball, timeout, chrome, driver });
  verifyBrowserAttachmentConsumer(attachmentResult);
  console.log(`LAB_COLORS_PROGRAM_ATTACHMENT_BROWSER_RESULT ${JSON.stringify(attachmentResult)}`);
  const reports = await verifyCleanupFaultMatrix(run);
  for (const report of reports) console.log(`LAB_COLORS_PROGRAM_BROWSER_FAULT ${JSON.stringify(report)}`);
  console.log(`LAB_COLORS_PROGRAM_BROWSER_PASS sha256=${digest}`);
}

const invokedDirectly = process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) main().catch((error) => { console.error(error instanceof Error ? error.stack : String(error)); process.exitCode = 1; });
