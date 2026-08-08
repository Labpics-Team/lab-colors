import { spawn } from "node:child_process";
import { access, readFile, stat } from "node:fs/promises";
import { createServer } from "node:http";
import { isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const PASS_RECEIPT = "LAB_COLORS_BROWSER_OUTPUT_SINK_PASS v1 checks=10";
const EXPECTED_CHECKS = Object.freeze([
  "constructed-stylesheet-computed-values",
  "single-live-replacement",
  "target-scope",
  "inline-preservation",
  "scratch-and-hostile-safety",
  "post-replace-drift-recovery",
  "detached-sheet-stale",
  "moved-target-stale",
  "dispose",
  "no-inline-writer",
]);
const DRIVER_LOG_LIMIT_BYTES = 8_192;
const DRIVER_START_FRACTION = 3;
const COMMAND_TIMEOUT_FRACTION = 2;
const DRIVER_READY_POLL_MS = 50;
const PROCESS_STOP_TIMEOUT_MS = 1_000;
const LOOPBACK_HOST = "127.0.0.1";

function fail(message, options) {
  throw new Error(`browser output-sink proof: ${message}`, options);
}

function parseProofTimeout() {
  const raw = process.env.LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS;
  if (!/^[1-9][0-9]*$/u.test(raw ?? "")) {
    fail("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS must be a positive integer");
  }
  const milliseconds = Number(raw);
  if (!Number.isSafeInteger(milliseconds)) {
    fail("LAB_COLORS_BROWSER_PROOF_TIMEOUT_MS exceeds the safe integer range");
  }
  return milliseconds;
}

async function executableFromEnv(name) {
  const value = process.env[name];
  if (!value || !isAbsolute(value)) fail(`${name} must name an absolute executable path`);
  await access(value);
  const metadata = await stat(value);
  if (!metadata.isFile()) fail(`${name} does not name a regular file`);
  return value;
}

function boundedLog(text) {
  return text.length <= DRIVER_LOG_LIMIT_BYTES
    ? text
    : text.slice(text.length - DRIVER_LOG_LIMIT_BYTES);
}

function abortableDelay(milliseconds, signal) {
  if (signal.aborted) return Promise.reject(signal.reason);
  return new Promise((resolveDelay, rejectDelay) => {
    const onAbort = () => {
      clearTimeout(timeout);
      rejectDelay(signal.reason);
    };
    const timeout = setTimeout(() => {
      signal.removeEventListener("abort", onAbort);
      resolveDelay();
    }, milliseconds);
    signal.addEventListener("abort", onAbort, { once: true });
  });
}

function timeoutSignal(milliseconds, label) {
  const controller = new AbortController();
  const expiresAt = performance.now() + milliseconds;
  const reason = new Error(`${label} exceeded ${milliseconds} ms`);
  const expire = () => controller.abort(reason);
  const timeout = setTimeout(expire, milliseconds);
  return {
    controller,
    clear: () => clearTimeout(timeout),
    throwIfExpired() {
      if (!controller.signal.aborted && performance.now() >= expiresAt) expire();
      controller.signal.throwIfAborted();
    },
  };
}

async function startChromeDriver(executable, timeoutMilliseconds, overall) {
  overall.throwIfExpired();
  const driver = spawn(executable, ["--port=0"], {
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  let log = "";
  let settled = false;
  const startup = timeoutSignal(timeoutMilliseconds, "ChromeDriver startup");

  let port;
  try {
    port = await new Promise((resolvePort, rejectPort) => {
      const finish = (callback, value) => {
        if (settled) return;
        settled = true;
        startup.controller.signal.removeEventListener("abort", onStartupAbort);
        overall.controller.signal.removeEventListener("abort", onOverallAbort);
        callback(value);
      };
      const onStartupAbort = () => finish(rejectPort, startup.controller.signal.reason);
      const onOverallAbort = () => finish(rejectPort, overall.controller.signal.reason);
      const inspect = (chunk) => {
        log = boundedLog(log + chunk.toString("utf8"));
        const match = /started successfully on port ([1-9][0-9]*)/iu.exec(log);
        if (match) finish(resolvePort, Number(match[1]));
      };

      driver.stdout.on("data", inspect);
      driver.stderr.on("data", inspect);
      driver.once("error", (cause) => finish(rejectPort, cause));
      driver.once("exit", (code, signal) => {
        finish(
          rejectPort,
          new Error(
            `ChromeDriver exited before readiness (code=${String(code)}, signal=${String(signal)}): ${log}`,
          ),
        );
      });
      startup.controller.signal.addEventListener("abort", onStartupAbort, { once: true });
      overall.controller.signal.addEventListener("abort", onOverallAbort, { once: true });
      if (overall.controller.signal.aborted) {
        onOverallAbort();
      } else if (startup.controller.signal.aborted) {
        onStartupAbort();
      }
    });
    startup.throwIfExpired();
    overall.throwIfExpired();
  } catch (error) {
    await stopProcess(driver);
    throw error;
  } finally {
    startup.clear();
  }

  return { driver, port, log: () => log };
}

async function driverRequest(port, path, init, timeoutMilliseconds, overall) {
  overall.throwIfExpired();
  const command = timeoutSignal(timeoutMilliseconds, `WebDriver ${init.method} ${path}`);
  const overallSignal = overall.controller.signal;
  const onAbort = () => command.controller.abort(overallSignal.reason);
  overallSignal.addEventListener("abort", onAbort, { once: true });
  if (overallSignal.aborted) onAbort();
  try {
    const response = await fetch(`http://${LOOPBACK_HOST}:${port}${path}`, {
      ...init,
      headers: init.body === undefined ? undefined : { "content-type": "application/json" },
      signal: command.controller.signal,
    });
    const payload = await response.json().catch((cause) => {
      fail(`WebDriver returned non-JSON HTTP ${response.status}`, { cause });
    });
    command.throwIfExpired();
    overall.throwIfExpired();
    const protocolError = typeof payload?.value?.error === "string"
      ? payload.value.error
      : null;
    if (!response.ok || protocolError !== null) {
      fail(
        `WebDriver rejected ${init.method} ${path}: ${protocolError ?? response.status} ${payload?.value?.message ?? ""}`,
      );
    }
    return payload.value;
  } finally {
    command.clear();
    overallSignal.removeEventListener("abort", onAbort);
  }
}

async function waitForDriver(port, timeoutMilliseconds, overall) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    overall.throwIfExpired();
    try {
      const value = await driverRequest(
        port,
        "/status",
        { method: "GET" },
        Math.max(1, deadline - Date.now()),
        overall,
      );
      if (value?.ready === true) return;
    } catch (error) {
      if (overall.controller.signal.aborted) throw error;
    }
    await abortableDelay(
      Math.min(DRIVER_READY_POLL_MS, Math.max(1, deadline - Date.now())),
      overall.controller.signal,
    );
  }
  fail("ChromeDriver did not become ready within its declared startup budget");
}

async function listen(server, timeoutMilliseconds, overall) {
  overall.throwIfExpired();
  const deadline = timeoutSignal(timeoutMilliseconds, "proof server startup");
  const listenSignal = AbortSignal.any([
    deadline.controller.signal,
    overall.controller.signal,
  ]);
  try {
    await new Promise((resolveListen, rejectListen) => {
      let settled = false;
      const finish = (callback, value) => {
        if (settled) return;
        settled = true;
        server.removeListener("error", onError);
        server.removeListener("listening", onListening);
        listenSignal.removeEventListener("abort", onAbort);
        callback(value);
      };
      const onError = (cause) => finish(rejectListen, cause);
      const onListening = () => finish(resolveListen);
      const onAbort = () => finish(rejectListen, listenSignal.reason);

      server.once("error", onError);
      server.once("listening", onListening);
      listenSignal.addEventListener("abort", onAbort, { once: true });
      if (listenSignal.aborted) {
        onAbort();
        return;
      }
      try {
        server.listen({ port: 0, host: LOOPBACK_HOST, signal: listenSignal });
      } catch (cause) {
        finish(rejectListen, cause);
      }
    });
    deadline.throwIfExpired();
    overall.throwIfExpired();
  } finally {
    deadline.clear();
  }
  const address = server.address();
  if (address === null || typeof address === "string") fail("proof server has no TCP address");
  return address.port;
}

async function closeServer(server, timeoutMilliseconds) {
  if (!server.listening) return;
  const deadline = timeoutSignal(timeoutMilliseconds, "proof server cleanup");
  try {
    await new Promise((resolveClose, rejectClose) => {
      const onAbort = () => rejectClose(deadline.controller.signal.reason);
      deadline.controller.signal.addEventListener("abort", onAbort, { once: true });
      server.close((error) => {
        deadline.controller.signal.removeEventListener("abort", onAbort);
        if (error) rejectClose(error);
        else resolveClose();
      });
      server.closeAllConnections();
    });
    deadline.throwIfExpired();
  } finally {
    deadline.clear();
  }
}

function processExited(child) {
  return child.exitCode !== null || child.signalCode !== null;
}

async function waitForProcessExit(child) {
  if (processExited(child)) return true;
  return new Promise((resolveWait) => {
    let settled = false;
    let timeout;
    const finish = (exited) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      child.removeListener("exit", onExit);
      resolveWait(exited);
    };
    const onExit = () => finish(true);
    timeout = setTimeout(() => finish(false), PROCESS_STOP_TIMEOUT_MS);
    child.once("exit", onExit);
    if (processExited(child)) finish(true);
  });
}

async function stopProcess(child) {
  if (processExited(child)) return;
  child.kill();
  if (await waitForProcessExit(child)) return;
  child.kill("SIGKILL");
  if (!(await waitForProcessExit(child))) {
    fail("ChromeDriver did not exit after forced termination");
  }
}

async function runInBrowser(moduleUrl) {
  function assert(condition, message) {
    if (!condition) throw new Error(`assertion failed: ${message}`);
  }

  function equal(actual, expected, message) {
    if (!Object.is(actual, expected)) {
      throw new Error(
        `assertion failed: ${message}; expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`,
      );
    }
  }

  function expectCode(action, expectedCode, message) {
    let error;
    try {
      action();
    } catch (caught) {
      error = caught;
    }
    assert(error, `${message}: operation did not fail`);
    equal(error.code, expectedCode, `${message}: typed error code`);
    return error;
  }

  function markerName(target) {
    const names = target.getAttributeNames().filter((name) =>
      name.startsWith("data-lab-colors-output-sink-")
    );
    equal(names.length, 1, "target has one sink marker");
    return names[0];
  }

  function sheetText(sheet) {
    return Array.from(sheet.cssRules, (rule) => rule.cssText).join("\n");
  }

  function styleNames(style) {
    return Array.from({ length: style.length }, (_, index) => style.item(index)).sort();
  }

  function computed(target, name) {
    target.getBoundingClientRect();
    return getComputedStyle(target).getPropertyValue(name).trim();
  }

  function appendTarget(id) {
    const target = document.createElement("div");
    target.id = id;
    document.body.append(target);
    return target;
  }

  function acquireWithSheet(
    acquireOutputLease,
    target,
    bindings,
    context,
    root = target.getRootNode(),
  ) {
    const before = new Set(root.adoptedStyleSheets);
    const lease = acquireOutputLease(target, bindings, context);
    const added = root.adoptedStyleSheets.filter((sheet) => !before.has(sheet));
    equal(added.length, 1, `${context}: exactly one constructed sheet attached`);
    return { lease, sheet: added[0] };
  }

  const checks = [];
  const mark = (name) => checks.push(name);
  const module = await import(moduleUrl);
  const { acquireOutputLease } = module;
  assert(typeof acquireOutputLease === "function", "output sink exports acquireOutputLease");
  assert(typeof CSSStyleSheet === "function", "constructed CSSStyleSheet is available");
  assert(Array.isArray(document.adoptedStyleSheets), "Document exposes adoptedStyleSheets");

  const replaceDescriptor = Object.getOwnPropertyDescriptor(
    CSSStyleSheet.prototype,
    "replaceSync",
  );
  const setDescriptor = Object.getOwnPropertyDescriptor(
    CSSStyleDeclaration.prototype,
    "setProperty",
  );
  const removeDescriptor = Object.getOwnPropertyDescriptor(
    CSSStyleDeclaration.prototype,
    "removeProperty",
  );
  const declarationTextDescriptor = Object.getOwnPropertyDescriptor(
    CSSStyleDeclaration.prototype,
    "cssText",
  );
  const selectorDescriptor = Object.getOwnPropertyDescriptor(
    CSSStyleRule.prototype,
    "selectorText",
  );
  const elementSetAttributeDescriptor = Object.getOwnPropertyDescriptor(
    Element.prototype,
    "setAttribute",
  );
  const elementRemoveAttributeDescriptor = Object.getOwnPropertyDescriptor(
    Element.prototype,
    "removeAttribute",
  );
  const sheetMutationDescriptors = new Map(
    ["replace", "insertRule", "deleteRule", "addRule", "removeRule"].map((name) => [
      name,
      Object.getOwnPropertyDescriptor(CSSStyleSheet.prototype, name),
    ]),
  );
  const typedStyleMutationDescriptors = new Map(
    ["set", "append", "delete", "clear"].map((name) => [
      name,
      Object.getOwnPropertyDescriptor(StylePropertyMap.prototype, name),
    ]),
  );
  assert(replaceDescriptor?.value, "replaceSync has an own callable descriptor");
  assert(setDescriptor?.value, "setProperty has an own callable descriptor");
  assert(removeDescriptor?.value, "removeProperty has an own callable descriptor");
  assert(declarationTextDescriptor?.set, "cssText has an own setter descriptor");
  assert(selectorDescriptor?.set, "selectorText has an own setter descriptor");
  assert(elementSetAttributeDescriptor?.value, "Element.setAttribute has an own descriptor");
  assert(elementRemoveAttributeDescriptor?.value, "Element.removeAttribute has an own descriptor");
  assert(
    [...sheetMutationDescriptors.values()].every((descriptor) => descriptor?.value),
    "constructed stylesheet mutation methods have own callable descriptors",
  );
  assert(
    [...typedStyleMutationDescriptors.values()].every((descriptor) => descriptor?.value),
    "Typed OM mutation methods have own callable descriptors",
  );

  const replaceEvents = [];
  const monitoredInlineStyles = new WeakSet();
  const monitoredTargets = new WeakSet();
  let inlineWrites = 0;
  let liveSequentialWrites = 0;
  let afterLiveReplace = null;
  const recordInlineMutations = (records) => {
    for (const record of records) {
      if (monitoredTargets.has(record.target)) inlineWrites += 1;
    }
  };
  const inlineMutationObserver = new MutationObserver(recordInlineMutations);
  inlineMutationObserver.observe(document, {
    attributes: true,
    attributeFilter: ["style"],
    subtree: true,
  });
  const monitorTarget = (target) => {
    recordInlineMutations(inlineMutationObserver.takeRecords());
    monitoredInlineStyles.add(target.style);
    monitoredTargets.add(target);
  };
  const flushInlineMutations = () => {
    recordInlineMutations(inlineMutationObserver.takeRecords());
  };
  const isLiveSheet = (sheet) => document.adoptedStyleSheets.includes(sheet);
  const isLiveRule = (candidate) => document.adoptedStyleSheets.some((sheet) =>
    Array.from(sheet.cssRules).includes(candidate)
  );
  const isLiveDeclaration = (candidate) => document.adoptedStyleSheets.some((sheet) =>
    Array.from(sheet.cssRules).some((rule) => rule.style === candidate)
  );
  const isLiveStyleMap = (candidate) => document.adoptedStyleSheets.some((sheet) =>
    Array.from(sheet.cssRules).some((rule) => rule.styleMap === candidate)
  );
  Object.defineProperty(CSSStyleSheet.prototype, "replaceSync", {
    ...replaceDescriptor,
    value(text) {
      const live = isLiveSheet(this);
      const result = replaceDescriptor.value.call(this, text);
      replaceEvents.push({ live, text, rules: this.cssRules.length });
      if (live && afterLiveReplace !== null) {
        const callback = afterLiveReplace;
        afterLiveReplace = null;
        callback(this, text);
      }
      return result;
    },
  });
  Object.defineProperty(CSSStyleDeclaration.prototype, "setProperty", {
    ...setDescriptor,
    value(...args) {
      if (monitoredInlineStyles.has(this)) inlineWrites += 1;
      if (isLiveDeclaration(this)) liveSequentialWrites += 1;
      return setDescriptor.value.apply(this, args);
    },
  });
  Object.defineProperty(CSSStyleDeclaration.prototype, "removeProperty", {
    ...removeDescriptor,
    value(...args) {
      if (monitoredInlineStyles.has(this)) inlineWrites += 1;
      if (isLiveDeclaration(this)) liveSequentialWrites += 1;
      return removeDescriptor.value.apply(this, args);
    },
  });
  Object.defineProperty(CSSStyleDeclaration.prototype, "cssText", {
    ...declarationTextDescriptor,
    set(value) {
      if (monitoredInlineStyles.has(this)) inlineWrites += 1;
      if (isLiveDeclaration(this)) liveSequentialWrites += 1;
      return declarationTextDescriptor.set.call(this, value);
    },
  });
  Object.defineProperty(CSSStyleRule.prototype, "selectorText", {
    ...selectorDescriptor,
    set(value) {
      if (isLiveRule(this)) liveSequentialWrites += 1;
      return selectorDescriptor.set.call(this, value);
    },
  });
  Object.defineProperty(Element.prototype, "setAttribute", {
    ...elementSetAttributeDescriptor,
    value(name, ...args) {
      if (monitoredTargets.has(this) && String(name).toLowerCase() === "style") {
        inlineWrites += 1;
      }
      return elementSetAttributeDescriptor.value.call(this, name, ...args);
    },
  });
  Object.defineProperty(Element.prototype, "removeAttribute", {
    ...elementRemoveAttributeDescriptor,
    value(name, ...args) {
      if (monitoredTargets.has(this) && String(name).toLowerCase() === "style") {
        inlineWrites += 1;
      }
      return elementRemoveAttributeDescriptor.value.call(this, name, ...args);
    },
  });
  for (const [name, descriptor] of sheetMutationDescriptors) {
    Object.defineProperty(CSSStyleSheet.prototype, name, {
      ...descriptor,
      value(...args) {
        if (isLiveSheet(this)) liveSequentialWrites += 1;
        return descriptor.value.apply(this, args);
      },
    });
  }
  for (const [name, descriptor] of typedStyleMutationDescriptors) {
    Object.defineProperty(StylePropertyMap.prototype, name, {
      ...descriptor,
      value(...args) {
        if (isLiveStyleMap(this)) liveSequentialWrites += 1;
        return descriptor.value.apply(this, args);
      },
    });
  }

  try {
    const inlineProbe = appendTarget("inline-observer-probe");
    monitorTarget(inlineProbe);
    inlineProbe.setAttributeNS(null, "style", "--lab-observer-probe: observed");
    flushInlineMutations();
    assert(inlineWrites > 0, "style MutationObserver is sensitive to namespace writes");
    inlineProbe.remove();
    inlineWrites = 0;

    const cssomProbe = new CSSStyleSheet();
    replaceDescriptor.value.call(cssomProbe, ":root { --lab-cssom-probe: initial; }");
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, cssomProbe];
    let probeWrites = liveSequentialWrites;
    cssomProbe.addRule(":root", "--lab-legacy-probe: observed");
    assert(liveSequentialWrites > probeWrites,
      "legacy live CSSOM instrumentation is mutation-sensitive");
    probeWrites = liveSequentialWrites;
    cssomProbe.cssRules[0].styleMap.clear();
    assert(liveSequentialWrites > probeWrites,
      "live Typed OM instrumentation is mutation-sensitive");
    document.adoptedStyleSheets = document.adoptedStyleSheets.filter(
      (sheet) => sheet !== cssomProbe,
    );
    liveSequentialWrites = 0;

    const targetA = appendTarget("target-a");
    const targetB = appendTarget("target-b");
    setDescriptor.value.call(targetA.style, "--lab-consumer", "consumer-owned");
    monitorTarget(targetA);
    monitorTarget(targetB);

    const first = acquireWithSheet(
      acquireOutputLease,
      targetA,
      ["--lab-a", "--lab-missing"],
      "browser/target-a",
    );
    const disjoint = acquireOutputLease(targetA, ["--lab-c"], "browser/target-a-disjoint");
    equal(document.adoptedStyleSheets.filter((sheet) => sheet === first.sheet).length, 1,
      "disjoint leases share one target sheet");

    let liveBefore = replaceEvents.filter((event) => event.live).length;
    equal(first.lease.publish({ "--lab-a": "#111111" }), true, "first publication commits");
    equal(
      replaceEvents.filter((event) => event.live).length - liveBefore,
      1,
      "publication performs one live replacement",
    );
    equal(computed(targetA, "--lab-a"), "#111111", "constructed sheet reaches computed style");
    equal(computed(targetA, "--lab-missing"), "", "missing binding tombstone has no value");
    mark("constructed-stylesheet-computed-values");

    liveBefore = replaceEvents.filter((event) => event.live).length;
    equal(disjoint.publish({ "--lab-c": "#333333" }), true, "disjoint publication commits");
    equal(
      replaceEvents.filter((event) => event.live).length - liveBefore,
      1,
      "disjoint publication performs one live replacement",
    );
    equal(first.sheet.cssRules.length, 1, "live sink contains one complete target rule");
    const targetARule = first.sheet.cssRules[0];
    const targetAMarker = markerName(targetA);
    equal(targetARule.selectorText, `[${targetAMarker}]`, "live rule is target-scoped");
    assert(
      JSON.stringify(styleNames(targetARule.style)) ===
        JSON.stringify(["--lab-a", "--lab-c"]),
      "live rule has exactly the published subset of the merged owned binding set",
    );
    equal(liveSequentialWrites, 0,
      "publication reaches the live sheet only through replaceSync");
    mark("single-live-replacement");

    const second = acquireWithSheet(
      acquireOutputLease,
      targetB,
      ["--lab-a"],
      "browser/target-b",
    );
    equal(second.lease.publish({ "--lab-a": "#222222" }), true, "second target publication commits");
    equal(computed(targetA, "--lab-a"), "#111111", "target A keeps its scoped value");
    equal(computed(targetB, "--lab-a"), "#222222", "target B gets its scoped value");
    mark("target-scope");

    equal(
      targetA.style.getPropertyValue("--lab-consumer").trim(),
      "consumer-owned",
      "nonbinding inline declaration remains byte-equivalent",
    );
    equal(
      computed(targetA, "--lab-consumer"),
      "consumer-owned",
      "nonbinding inline declaration remains effective",
    );
    mark("inline-preservation");

    const invalidTarget = appendTarget("invalid-binding");
    invalidTarget.style.color = "rgb(4, 5, 6)";
    monitorTarget(invalidTarget);
    const invalidSheets = document.adoptedStyleSheets.length;
    const invalidLiveBefore = replaceEvents.filter((event) => event.live).length;
    const suspiciousBinding = "--lab-injected; color: red";
    let suspiciousError;
    let suspiciousLease;
    let suspiciousSheet;
    try {
      const acquired = acquireWithSheet(
        acquireOutputLease,
        invalidTarget,
        [suspiciousBinding],
        "browser/invalid-binding",
      );
      suspiciousLease = acquired.lease;
      suspiciousSheet = acquired.sheet;
    } catch (error) {
      suspiciousError = error;
    }
    equal(replaceEvents.filter((event) => event.live).length - invalidLiveBefore, 0,
      "binding admission performs no live replacement before commit");
    if (suspiciousError) {
      equal(suspiciousError.code, "OUTPUT_BINDING_INVALID",
        "scratch binding rejection is typed");
      equal(document.adoptedStyleSheets.length, invalidSheets,
        "rejected binding cannot attach a live sheet");
      equal(
        invalidTarget.getAttributeNames().filter((name) =>
          name.startsWith("data-lab-colors-output-sink-")
        ).length,
        0,
        "rejected binding cannot install a target marker",
      );
    } else {
      const suspiciousMarker = markerName(invalidTarget);
      liveBefore = replaceEvents.filter((event) => event.live).length;
      equal(
        suspiciousLease.publish({ [suspiciousBinding]: "safe-binding-value" }),
        true,
        "CSSOM-admitted suspicious binding publishes explicitly",
      );
      equal(replaceEvents.filter((event) => event.live).length - liveBefore, 1,
        "CSSOM-admitted suspicious binding performs one live replacement");
      equal(suspiciousSheet.cssRules.length, 1,
        "CSSOM-admitted suspicious binding cannot create another rule");
      const suspiciousRule = suspiciousSheet.cssRules[0];
      equal(suspiciousRule.style.length, 1,
        "CSSOM-admitted suspicious binding cannot create another declaration");
      equal(suspiciousRule.style.item(0), suspiciousBinding,
        "CSSOM-admitted suspicious binding name round-trips exactly");
      equal(suspiciousRule.style.getPropertyValue(suspiciousBinding), "safe-binding-value",
        "CSSOM-admitted suspicious binding value round-trips exactly");
      equal(suspiciousRule.style.getPropertyValue("color"), "",
        "CSSOM-admitted suspicious binding cannot inject color");
      equal(getComputedStyle(invalidTarget).color, "rgb(4, 5, 6)",
        "CSSOM-admitted suspicious binding leaves element color untouched");
      equal(suspiciousLease.dispose(), true,
        "CSSOM-admitted suspicious binding lease disposes cleanly");
      assert(!document.adoptedStyleSheets.includes(suspiciousSheet),
        "suspicious binding disposal detaches its sheet");
      assert(!invalidTarget.hasAttribute(suspiciousMarker),
        "suspicious binding disposal removes its marker");
    }

    const inlineConflict = appendTarget("inline-conflict");
    setDescriptor.value.call(inlineConflict.style, "--lab-owned", "inline-owner");
    monitorTarget(inlineConflict);
    const conflictSheets = document.adoptedStyleSheets.length;
    expectCode(
      () => acquireOutputLease(inlineConflict, ["--lab-owned"], "browser/inline-conflict"),
      "OUTPUT_INLINE_BINDING_CONFLICT",
      "owned inline declaration fails closed",
    );
    equal(document.adoptedStyleSheets.length, conflictSheets,
      "inline conflict cannot attach a live sheet");
    equal(inlineConflict.style.getPropertyValue("--lab-owned").trim(), "inline-owner",
      "inline conflict leaves its declaration untouched");

    const hostileTarget = appendTarget("hostile-value");
    hostileTarget.style.color = "rgb(1, 2, 3)";
    monitorTarget(hostileTarget);
    const hostile = acquireWithSheet(
      acquireOutputLease,
      hostileTarget,
      ["--lab-safe"],
      "browser/hostile-value",
    );
    equal(hostile.lease.publish({ "--lab-safe": "baseline" }), true,
      "hostile-value baseline commits");
    const hostileBefore = sheetText(hostile.sheet);
    const hostileStamp = hostile.lease.stamp;
    liveBefore = replaceEvents.filter((event) => event.live).length;
    const hostileValue = "safe; color: rgb(255, 0, 0)";
    let hostileError;
    let hostileCommitted = false;
    try {
      hostileCommitted = hostile.lease.publish({ "--lab-safe": hostileValue });
    } catch (error) {
      hostileError = error;
    }
    const hostileRule = hostile.sheet.cssRules[0];
    equal(hostile.sheet.cssRules.length, 1, "hostile input cannot create another CSS rule");
    assert(hostileRule, "hostile input leaves one readable target rule");
    assert(
      JSON.stringify(styleNames(hostileRule.style)) === JSON.stringify(["--lab-safe"]),
      "hostile input cannot create an extra declaration",
    );
    equal(getComputedStyle(hostileTarget).color, "rgb(1, 2, 3)",
      "hostile custom-property value cannot inject color");
    if (hostileError) {
      equal(hostileError.code, "OUTPUT_STYLESHEET_INVALID",
        "hostile scratch rejection is typed");
      equal(sheetText(hostile.sheet), hostileBefore,
        "hostile rejection leaves prior live bytes unchanged");
      equal(hostile.lease.stamp, hostileStamp,
        "hostile rejection leaves publication stamp unchanged");
      equal(replaceEvents.filter((event) => event.live).length - liveBefore, 0,
        "hostile rejection performs no live replacement");
    } else {
      equal(hostileCommitted, true, "safe hostile serialization commits explicitly");
      equal(hostileRule.style.getPropertyValue("--lab-safe").trim(), hostileValue,
        "accepted hostile value round-trips exactly as one custom property");
      equal(replaceEvents.filter((event) => event.live).length - liveBefore, 1,
        "safe hostile serialization performs one live replacement");
    }
    mark("scratch-and-hostile-safety");

    const inlineRaceTarget = appendTarget("post-replace-inline-race");
    const inlineRace = acquireWithSheet(
      acquireOutputLease,
      inlineRaceTarget,
      ["--lab-inline-race"],
      "browser/post-replace-inline-race",
    );
    const inlineRaceStamp = inlineRace.lease.stamp;
    afterLiveReplace = (sheet) => {
      equal(sheet, inlineRace.sheet, "inline race mutates the intended live sheet boundary");
      setDescriptor.value.call(inlineRaceTarget.style, "--lab-inline-race", "foreign-inline");
    };
    const inlineRaceError = expectCode(
      () => inlineRace.lease.publish({ "--lab-inline-race": "owned-sheet" }),
      "OUTPUT_ATOMICITY_VIOLATION",
      "owned inline mutation during live replacement fails closed",
    );
    equal(inlineRaceError.cause?.code, "OUTPUT_INLINE_BINDING_CONFLICT",
      "inline replacement race preserves its typed host-drift cause");
    equal(inlineRace.lease.stamp, inlineRaceStamp,
      "inline replacement race cannot advance the logical stamp");
    equal(inlineRace.lease.state, "active",
      "inline replacement race keeps a caller-reachable cleanup lease");
    equal(inlineRace.lease.dispose(), true,
      "poisoned inline replacement race remains disposable");
    equal(inlineRace.lease.state, "disposed", "inline race lease is released");
    assert(!document.adoptedStyleSheets.includes(inlineRace.sheet),
      "inline race cleanup detaches the poisoned live sheet");
    equal(inlineRaceTarget.style.getPropertyValue("--lab-inline-race").trim(), "foreign-inline",
      "inline race cleanup preserves the foreign inline declaration");
    removeDescriptor.value.call(inlineRaceTarget.style, "--lab-inline-race");
    const inlineRaceRetry = acquireWithSheet(
      acquireOutputLease,
      inlineRaceTarget,
      ["--lab-inline-race"],
      "browser/post-replace-inline-race-retry",
    );
    equal(inlineRaceRetry.lease.publish({ "--lab-inline-race": "recovered" }), true,
      "inline race target can be reacquired after cleanup");
    equal(inlineRaceRetry.lease.dispose(), true, "inline race retry disposes");

    const markerRaceTarget = appendTarget("post-replace-marker-race");
    const markerRace = acquireWithSheet(
      acquireOutputLease,
      markerRaceTarget,
      ["--lab-marker-race"],
      "browser/post-replace-marker-race",
    );
    const markerRaceName = markerName(markerRaceTarget);
    const markerClone = appendTarget("post-replace-marker-clone");
    const markerRaceStamp = markerRace.lease.stamp;
    afterLiveReplace = (sheet) => {
      equal(sheet, markerRace.sheet, "marker race mutates the intended live sheet boundary");
      elementSetAttributeDescriptor.value.call(markerClone, markerRaceName, "");
    };
    const markerRaceError = expectCode(
      () => markerRace.lease.publish({ "--lab-marker-race": "owned-sheet" }),
      "OUTPUT_ATOMICITY_VIOLATION",
      "marker duplication during live replacement fails closed",
    );
    equal(markerRaceError.cause?.code, "OUTPUT_TARGET_STALE",
      "marker replacement race preserves its typed host-drift cause");
    equal(markerRace.lease.stamp, markerRaceStamp,
      "marker replacement race cannot advance the logical stamp");
    equal(markerRace.lease.state, "active",
      "marker replacement race keeps a caller-reachable cleanup lease");
    equal(markerRace.lease.dispose(), true,
      "poisoned marker replacement race remains disposable");
    assert(!document.adoptedStyleSheets.includes(markerRace.sheet),
      "marker race cleanup detaches the poisoned live sheet");
    assert(!markerRaceTarget.hasAttribute(markerRaceName),
      "marker race cleanup removes only the owned target marker");
    assert(markerClone.hasAttribute(markerRaceName),
      "marker race cleanup does not mutate the foreign clone");
    elementRemoveAttributeDescriptor.value.call(markerClone, markerRaceName);
    markerClone.remove();
    const markerRaceRetry = acquireWithSheet(
      acquireOutputLease,
      markerRaceTarget,
      ["--lab-marker-race"],
      "browser/post-replace-marker-race-retry",
    );
    equal(markerRaceRetry.lease.publish({ "--lab-marker-race": "recovered" }), true,
      "marker race target can be reacquired after hostile marker cleanup");
    equal(markerRaceRetry.lease.dispose(), true, "marker race retry disposes");

    const rootRaceTarget = appendTarget("post-replace-root-race");
    const rootRace = acquireWithSheet(
      acquireOutputLease,
      rootRaceTarget,
      ["--lab-root-race"],
      "browser/post-replace-root-race",
    );
    const rootRaceStamp = rootRace.lease.stamp;
    const rootRaceHost = appendTarget("post-replace-root-host");
    const rootRaceShadow = rootRaceHost.attachShadow({ mode: "open" });
    afterLiveReplace = (sheet) => {
      equal(sheet, rootRace.sheet, "root race mutates the intended live sheet boundary");
      rootRaceShadow.append(rootRaceTarget);
    };
    const rootRaceError = expectCode(
      () => rootRace.lease.publish({ "--lab-root-race": "owned-sheet" }),
      "OUTPUT_ATOMICITY_VIOLATION",
      "root move during live replacement fails closed",
    );
    equal(rootRaceError.cause?.code, "OUTPUT_TARGET_STALE",
      "root replacement race preserves its typed host-drift cause");
    equal(rootRace.lease.stamp, rootRaceStamp,
      "root replacement race cannot advance the logical stamp");
    equal(rootRace.lease.state, "active",
      "root replacement race keeps a caller-reachable cleanup lease");
    equal(rootRace.lease.dispose(), true,
      "poisoned cross-root replacement race remains disposable");
    assert(!document.adoptedStyleSheets.includes(rootRace.sheet),
      "cross-root race cleanup detaches the old-root sheet");
    const rootRaceRetry = acquireWithSheet(
      acquireOutputLease,
      rootRaceTarget,
      ["--lab-root-race"],
      "browser/post-replace-root-race-retry",
      rootRaceShadow,
    );
    equal(rootRaceRetry.lease.publish({ "--lab-root-race": "recovered" }), true,
      "cross-root race target can be reacquired in its new ShadowRoot");
    equal(computed(rootRaceTarget, "--lab-root-race"), "recovered",
      "reacquired ShadowRoot sheet reaches computed style");
    equal(rootRaceRetry.lease.dispose(), true, "cross-root race retry disposes");
    assert(!rootRaceShadow.adoptedStyleSheets.includes(rootRaceRetry.sheet),
      "cross-root retry cleanup detaches the ShadowRoot sheet");
    mark("post-replace-drift-recovery");

    const detachedTarget = appendTarget("detached-sheet");
    monitorTarget(detachedTarget);
    const detached = acquireWithSheet(
      acquireOutputLease,
      detachedTarget,
      ["--lab-detached"],
      "browser/detached-sheet",
    );
    detached.lease.publish({ "--lab-detached": "before-detach" });
    const detachedText = sheetText(detached.sheet);
    const detachedStamp = detached.lease.stamp;
    document.adoptedStyleSheets = document.adoptedStyleSheets.filter(
      (sheet) => sheet !== detached.sheet,
    );
    liveBefore = replaceEvents.filter((event) => event.live).length;
    expectCode(
      () => detached.lease.publish({ "--lab-detached": "after-detach" }),
      "OUTPUT_TARGET_STALE",
      "detached live sheet fails closed",
    );
    equal(detached.lease.stamp, detachedStamp, "detached failure leaves stamp unchanged");
    equal(sheetText(detached.sheet), detachedText, "detached failure leaves sheet bytes unchanged");
    equal(replaceEvents.filter((event) => event.live).length - liveBefore, 0,
      "detached failure performs no live replacement");
    equal(detached.lease.dispose(), true,
      "detached stale lease remains explicitly revocable");
    equal(detached.lease.state, "disposed", "detached stale lease is released");
    mark("detached-sheet-stale");

    const movedTarget = appendTarget("moved-target");
    monitorTarget(movedTarget);
    const moved = acquireWithSheet(
      acquireOutputLease,
      movedTarget,
      ["--lab-moved"],
      "browser/moved-target",
    );
    moved.lease.publish({ "--lab-moved": "before-move" });
    const movedText = sheetText(moved.sheet);
    const movedStamp = moved.lease.stamp;
    const shadowHost = appendTarget("shadow-host");
    shadowHost.attachShadow({ mode: "open" }).append(movedTarget);
    liveBefore = replaceEvents.filter((event) => event.live).length;
    expectCode(
      () => moved.lease.publish({ "--lab-moved": "after-move" }),
      "OUTPUT_TARGET_STALE",
      "root move fails closed",
    );
    equal(moved.lease.stamp, movedStamp, "move failure leaves stamp unchanged");
    equal(sheetText(moved.sheet), movedText, "move failure leaves sheet bytes unchanged");
    equal(replaceEvents.filter((event) => event.live).length - liveBefore, 0,
      "move failure performs no live replacement");
    equal(moved.lease.dispose(), true, "moved stale lease remains explicitly revocable");
    assert(!document.adoptedStyleSheets.includes(moved.sheet),
      "moved stale cleanup detaches the old-root sheet");
    mark("moved-target-stale");

    liveBefore = replaceEvents.filter((event) => event.live).length;
    equal(first.lease.dispose(), true, "dispose commits");
    equal(first.lease.state, "disposed", "disposed lease becomes stale");
    equal(first.lease.publish({ "--lab-a": "#aaaaaa" }), false,
      "disposed lease cannot publish");
    equal(replaceEvents.filter((event) => event.live).length - liveBefore, 1,
      "dispose performs one live replacement");
    equal(computed(targetA, "--lab-a"), "", "dispose removes only the released binding");
    equal(computed(targetA, "--lab-c"), "#333333",
      "dispose preserves the disjoint lease");
    equal(computed(targetA, "--lab-consumer"), "consumer-owned",
      "dispose preserves nonbinding inline state");
    liveBefore = replaceEvents.filter((event) => event.live).length;
    equal(disjoint.dispose(), true, "last lease disposal commits");
    equal(replaceEvents.filter((event) => event.live).length - liveBefore, 1,
      "last lease disposal performs one empty live replacement");
    assert(!document.adoptedStyleSheets.includes(first.sheet),
      "last lease disposal detaches the dormant constructed sheet");
    assert(!targetA.hasAttribute(targetAMarker),
      "last lease disposal removes the target marker");
    equal(first.sheet.cssRules.length, 0,
      "last lease disposal leaves the detached sheet empty");
    equal(sheetText(first.sheet), "",
      "last lease disposal leaves no detached stylesheet bytes");
    equal(computed(targetA, "--lab-c"), "", "last lease disposal removes its binding");
    equal(computed(targetA, "--lab-consumer"), "consumer-owned",
      "last lease disposal preserves nonbinding inline state");
    equal(second.lease.dispose(), true, "second target lease disposes cleanly");
    equal(hostile.lease.dispose(), true, "hostile-value lease disposes cleanly");
    mark("dispose");

    equal(liveSequentialWrites, 0,
      "sink never mutates a live rule or stylesheet sequentially");
    flushInlineMutations();
    equal(inlineWrites, 0,
      "sink never mutates monitored inline declarations or style attributes");
    mark("no-inline-writer");
  } finally {
    flushInlineMutations();
    inlineMutationObserver.disconnect();
    Object.defineProperty(CSSStyleSheet.prototype, "replaceSync", replaceDescriptor);
    Object.defineProperty(CSSStyleDeclaration.prototype, "setProperty", setDescriptor);
    Object.defineProperty(CSSStyleDeclaration.prototype, "removeProperty", removeDescriptor);
    Object.defineProperty(
      CSSStyleDeclaration.prototype,
      "cssText",
      declarationTextDescriptor,
    );
    Object.defineProperty(CSSStyleRule.prototype, "selectorText", selectorDescriptor);
    Object.defineProperty(Element.prototype, "setAttribute", elementSetAttributeDescriptor);
    Object.defineProperty(Element.prototype, "removeAttribute", elementRemoveAttributeDescriptor);
    for (const [name, descriptor] of sheetMutationDescriptors) {
      Object.defineProperty(CSSStyleSheet.prototype, name, descriptor);
    }
    for (const [name, descriptor] of typedStyleMutationDescriptors) {
      Object.defineProperty(StylePropertyMap.prototype, name, descriptor);
    }
  }

  return { checks };
}

async function main() {
  const proofTimeoutMilliseconds = parseProofTimeout();
  const driverStartTimeoutMilliseconds = Math.max(
    1,
    Math.floor(proofTimeoutMilliseconds / DRIVER_START_FRACTION),
  );
  const commandTimeoutMilliseconds = Math.max(
    1,
    Math.floor(proofTimeoutMilliseconds / COMMAND_TIMEOUT_FRACTION),
  );
  const chromePath = await executableFromEnv("CHROME_PATH");
  const chromeDriverPath = await executableFromEnv("CHROMEDRIVER_PATH");
  const modulePath = resolve(
    fileURLToPath(new URL("..", import.meta.url)),
    "packages/colors/output-sink.js",
  );
  const moduleSource = await readFile(modulePath);
  const server = createServer((request, response) => {
    const pathname = new URL(request.url ?? "/", `http://${LOOPBACK_HOST}`).pathname;
    if (request.method !== "GET") {
      response.writeHead(405).end();
      return;
    }
    if (pathname === "/") {
      response.writeHead(200, {
        "cache-control": "no-store",
        "content-security-policy":
          "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'",
        "content-type": "text/html; charset=utf-8",
      });
      response.end("<!doctype html><meta charset=utf-8><title>Lab Colors output sink proof</title>");
      return;
    }
    if (pathname === "/output-sink.js") {
      response.writeHead(200, {
        "cache-control": "no-store",
        "content-type": "text/javascript; charset=utf-8",
        "x-content-type-options": "nosniff",
      });
      response.end(moduleSource);
      return;
    }
    response.writeHead(404).end();
  });

  const overall = timeoutSignal(proofTimeoutMilliseconds, "browser output-sink proof");
  let driver;
  let driverPort;
  let sessionId;
  let proofComplete = false;
  let failure;
  try {
    const serverPort = await listen(server, commandTimeoutMilliseconds, overall);
    const started = await startChromeDriver(
      chromeDriverPath,
      driverStartTimeoutMilliseconds,
      overall,
    );
    driver = started.driver;
    driverPort = started.port;
    await waitForDriver(
      started.port,
      driverStartTimeoutMilliseconds,
      overall,
    );
    const session = await driverRequest(
      started.port,
      "/session",
      {
        method: "POST",
        body: JSON.stringify({
          capabilities: {
            alwaysMatch: {
              browserName: "chrome",
              "goog:chromeOptions": {
                binary: chromePath,
                args: [
                  "--headless=new",
                  "--disable-background-networking",
                  "--disable-component-update",
                  "--disable-default-apps",
                  "--disable-dev-shm-usage",
                  "--disable-gpu",
                  "--disable-sync",
                  "--metrics-recording-only",
                  "--no-first-run",
                  "--window-size=800,600",
                ],
              },
            },
          },
        }),
      },
      commandTimeoutMilliseconds,
      overall,
    );
    sessionId = session?.sessionId;
    if (typeof sessionId !== "string" || sessionId.length === 0) {
      fail("ChromeDriver created no W3C session id");
    }
    await driverRequest(
      started.port,
      `/session/${sessionId}/timeouts`,
      {
        method: "POST",
        body: JSON.stringify({
          implicit: 0,
          pageLoad: commandTimeoutMilliseconds,
          script: commandTimeoutMilliseconds,
        }),
      },
      commandTimeoutMilliseconds,
      overall,
    );
    const origin = `http://${LOOPBACK_HOST}:${serverPort}`;
    await driverRequest(
      started.port,
      `/session/${sessionId}/url`,
      { method: "POST", body: JSON.stringify({ url: origin }) },
      commandTimeoutMilliseconds,
      overall,
    );
    const browserScript = `
      const done = arguments[arguments.length - 1];
      (${runInBrowser.toString()})(arguments[0]).then(
        (value) => done({ ok: true, value }),
        (error) => done({
          ok: false,
          error: {
            name: error?.name ?? "Error",
            code: error?.code ?? null,
            message: error?.message ?? String(error),
            stack: error?.stack ?? null,
          },
        }),
      );
    `;
    const result = await driverRequest(
      started.port,
      `/session/${sessionId}/execute/async`,
      {
        method: "POST",
        body: JSON.stringify({
          script: browserScript,
          args: [`${origin}/output-sink.js?proof=v1`],
        }),
      },
      commandTimeoutMilliseconds,
      overall,
    );
    if (result?.ok !== true) {
      fail(
        `browser assertion failed: ${result?.error?.code ?? result?.error?.name ?? "Error"}: ${result?.error?.message ?? "unknown failure"}\n${result?.error?.stack ?? ""}`,
      );
    }
    if (JSON.stringify(result.value?.checks) !== JSON.stringify(EXPECTED_CHECKS)) {
      fail(`browser returned an incomplete or reordered check receipt: ${JSON.stringify(result.value)}`);
    }
    overall.throwIfExpired();
    proofComplete = true;
  } catch (error) {
    failure = error;
  } finally {
    const cleanupErrors = [];
    try {
      overall.throwIfExpired();
    } catch (error) {
      if (failure === undefined) failure = error;
    }
    if (driverPort && sessionId && !overall.controller.signal.aborted) {
      await driverRequest(
        driverPort,
        `/session/${sessionId}`,
        { method: "DELETE" },
        commandTimeoutMilliseconds,
        overall,
      ).catch((error) => cleanupErrors.push(error));
    }
    if (driver) {
      await stopProcess(driver).catch((error) => cleanupErrors.push(error));
    }
    await closeServer(server, PROCESS_STOP_TIMEOUT_MS).catch((error) => cleanupErrors.push(error));
    try {
      overall.throwIfExpired();
    } catch (error) {
      if (failure === undefined) failure = error;
      else if (error !== failure) cleanupErrors.push(error);
    }
    overall.clear();
    if (cleanupErrors.length > 0) {
      failure = new AggregateError(
        failure === undefined ? cleanupErrors : [failure, ...cleanupErrors],
        "browser output-sink proof or lifecycle cleanup failed",
      );
    }
  }
  if (failure !== undefined) throw failure;
  if (!proofComplete) fail("proof ended without a complete browser receipt");
  process.stdout.write(`${PASS_RECEIPT}\n`);
}

main().catch((error) => {
  process.stderr.write(`${error?.stack ?? error}\n`);
  process.exitCode = 1;
});
