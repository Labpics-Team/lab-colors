from pathlib import Path

path = Path("packages/colors/test/program-runtime-browser-harness.test.mjs")
text = path.read_text()
text = text.replace(
    '  const names = ["temp-install", "browser", "browser-session", "server", "runtime", "snapshot", "host"];',
    '  const names = ["temp-install", "browser", "browser-session", "server", "host", "runtime", "authority"];',
    1,
)
old_cleanup = '''test("browser cleanup preserves the primary and releases snapshot, runtime, then host", () => {
  const primary = new Error("browser scenario failed");
  const events = [];
  const outcome = browserCleanup(primary, [
    { name: "snapshot", release: () => { events.push("snapshot"); throw new Error("snapshot free failed"); } },
    { name: "runtime", release: () => events.push("runtime") },
    { name: "host", release: () => events.push("host") },
  ]);
  assert.deepEqual(events, ["snapshot", "runtime", "host"]);
  assert.equal(outcome.error, "Error: browser scenario failed");
  assert.equal(outcome.cleanupError.code, "BROWSER_PROOF_CLEANUP_FAILED");
  assert.deepEqual(outcome.cleanupError.resources, ["snapshot"]);
});
'''
new_cleanup = '''test("browser cleanup preserves the primary and releases authority, runtime, then host", () => {
  const primary = new Error("browser scenario failed");
  const events = [];
  const outcome = browserCleanup(primary, [
    { name: "authority", release: () => { events.push("authority"); throw new Error("authority free failed"); } },
    { name: "runtime", release: () => events.push("runtime") },
    { name: "host", release: () => events.push("host") },
  ]);
  assert.deepEqual(events, ["authority", "runtime", "host"]);
  assert.equal(outcome.error, "Error: browser scenario failed");
  assert.equal(outcome.cleanupError.code, "BROWSER_PROOF_CLEANUP_FAILED");
  assert.deepEqual(outcome.cleanupError.resources, ["authority"]);
});
'''
if text.count(old_cleanup) != 1:
    raise SystemExit("cleanup contract anchor changed")
text = text.replace(old_cleanup, new_cleanup, 1)

start = text.find("async function executeScenario(fault, mutate = (source) => source) {")
end = text.find('test("driver startup propagates the actual asynchronous child error"', start)
if start < 0 or end < 0:
    raise SystemExit("serialized harness range anchors changed")
replacement = r'''async function executeScenario(fault, mutate = (source) => source) {
  const events = [], handles = [], elements = [], updates = [];
  let compileCount = 0;
  let nextEpoch = 100n;

  const typedError = (code, operation, cause) => Object.assign(new Error(code), {
    code, operation, ...(cause === undefined ? {} : { cause }),
  });
  const makeAuthority = ({ owner, identity, epoch, revision, sequence }) => {
    const value = {
      __wbg_ptr: 1,
      owner, identity, epoch, revision, sequence,
      publishedRevision: revision,
      sinkSequence: sequence,
      sinkBindingEpoch: epoch,
      presentationRoot: 71,
      presentationOccurrence: 61,
      terminalOccurrence: 61,
      opacity: 0.5,
      appearanceSurround: "average",
      rendererProvenance: "unverified",
      sourceRgb: () => new Uint8Array([64, 64, 64]),
      caseCount: () => 1,
      caseIndex: (index) => index === 0 ? 0 : undefined,
      caseComposite: (index) => index === 0 ? new Uint8Array([160, 160, 160]) : new Uint8Array(),
      free() { this.__wbg_ptr = 0; events.push("authority-handle"); },
    };
    handles.push(value);
    return value;
  };
  const makeUpdate = (state, authority) => {
    let current = authority;
    const value = {
      __wbg_ptr: 1,
      state,
      authorityCount: () => current === undefined ? 0 : 1,
      takeAuthority(index) {
        if (index !== 0 || current === undefined) {
          throw typedError("attached_authority_unavailable", "takeAttachedAuthority");
        }
        const taken = current;
        current = undefined;
        return taken;
      },
      free() { this.__wbg_ptr = 0; events.push("update-handle"); },
    };
    handles.push(value);
    return value;
  };
  const makeCompiled = (owner, identity) => {
    const compiled = {
      __wbg_ptr: 1,
      attach(_streamId, _bindings, host) {
        const epoch = ++nextEpoch;
        let revision = 0n;
        let sequence = 0n;
        const runtime = {
          __wbg_ptr: 1,
          updateObserved(nextRevision, ids, surfaces) {
            updates.push({ kind: "observed", revision: nextRevision, ids: Array.from(ids), surfaces: Array.from(surfaces) });
            if (ids.length !== 1 || surfaces.length !== 3 || nextRevision <= revision) {
              throw typedError("attached_update", "updateAttachedObserved");
            }
            const intent = {
              kind: "set-all", revision: nextRevision, bindingEpoch: epoch,
              expectedSequence: sequence, desiredSequence: sequence + 1n,
              patch: [{ output: 91, sinkOutput: 3, source: new Uint8Array([64, 64, 64]), opacity: 0.5 }],
            };
            try { host.tryInstall(intent); }
            catch (cause) { throw typedError("attached_host", "updateAttachedObserved", cause); }
            revision = nextRevision;
            sequence += 1n;
            return makeUpdate("ready", makeAuthority({ owner, identity, epoch, revision, sequence }));
          },
          updateUnknown(nextRevision, reasonId) {
            updates.push({ kind: "unknown", revision: nextRevision, reasonId });
            if (nextRevision <= revision) throw typedError("attached_update", "updateAttachedUnknown");
            const intent = {
              kind: "revoke-all", revision: nextRevision, bindingEpoch: epoch,
              expectedSequence: sequence, desiredSequence: sequence + 1n,
            };
            try { host.tryInstall(intent); }
            catch (cause) { throw typedError("attached_host", "updateAttachedUnknown", cause); }
            revision = nextRevision;
            sequence += 1n;
            return makeUpdate("stale");
          },
          validateAuthority(authority) {
            if (authority.identity !== identity) {
              throw typedError("attached_program_identity_mismatch", "validateAttachedAuthority");
            }
            if (authority.owner !== owner) {
              throw typedError("attached_foreign_owner_generation", "validateAttachedAuthority");
            }
            if (authority.epoch !== epoch) {
              throw typedError("attached_foreign_binding_epoch", "validateAttachedAuthority");
            }
            if (authority.revision !== revision || authority.sequence !== sequence) {
              throw typedError("attached_published_revision_mismatch", "validateAttachedAuthority");
            }
          },
          free() { this.__wbg_ptr = 0; events.push("runtime-handle"); },
        };
        handles.push(runtime);
        return runtime;
      },
      free() { this.__wbg_ptr = 0; events.push("compiled-handle"); },
    };
    handles.push(compiled);
    return compiled;
  };

  const api = {
    init: async () => {},
    isProgramError: (error) => error instanceof Error
      && typeof error.code === "string" && error.code.startsWith("attached_")
      && typeof error.operation === "string",
    compileAttachedProgramWire() {
      compileCount += 1;
      if (compileCount === 4) {
        throw typedError("attached_root_consumed_downstream", "compileAttachedProgramWire");
      }
      if (compileCount === 1) return makeCompiled(1, "identity-a");
      if (compileCount === 2) return makeCompiled(2, "identity-a");
      return makeCompiled(3, "identity-b");
    },
  };

  const makeElement = (connected = false) => {
    let cssText = "";
    const element = {
      isConnected: connected,
      style: {
        get cssText() { return cssText; },
        set cssText(value) { cssText = String(value); },
      },
      getAttribute(name) { return name === "style" ? cssText : null; },
      remove() { this.isConnected = false; events.push("host"); },
    };
    elements.push(element);
    return element;
  };
  const proofElement = makeElement(true);
  const document = {
    getElementById(id) { return id === "proof" ? proofElement : null; },
    createElement() { return makeElement(false); },
  };
  const wire = await import("../program-wire/abi-v1.js");
  const load = async (url) => url.endsWith("/index.js") ? api : wire;
  const source = mutate(browserScenario("https://proof.invalid", fault)).replaceAll("await import(", "await load(");
  const execute = new Function("load", "document", "getComputedStyle", "fetch", source);
  const envelope = await new Promise((done) => execute(
    load,
    document,
    (element) => {
      const match = element.style.cssText.match(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)\)/u);
      return { color: match ? `rgba(${match[1]}, ${match[2]}, ${match[3]}, ${match[4]})` : "rgba(0, 0, 0, 0)" };
    },
    () => undefined,
    done,
  ));
  assert.equal(envelope.error, undefined, "proof errors must not collide with WebDriver value.error");
  return { report: envelope.proof, events, handles, elements, updates };
}

function assertScenario({ report, handles, elements }, after, cleanup) {
  const order = ["host", "runtime", "authority"];
  const expected = order.slice(0, order.indexOf(after) + 1);
  assert.deepEqual(report.acquired, expected);
  assert.deepEqual(report.released, expected.toReversed());
  assert.ok(handles.every((handle) => handle.__wbg_ptr === 0));
  assert.ok(elements.filter((element) => element.isConnected).length === 0);
  assert.ok(expected.every((name) => report.readback[name] === true));
  assert.equal(report.error, `Error: fault after ${after}`);
  assert.equal(report.code, "BROWSER_PROOF_INJECTED");
  assert.equal(report.operation, after);
  if (cleanup) {
    assert.equal(report.cleanupError.code, "BROWSER_PROOF_CLEANUP_FAILED");
    assert.deepEqual(report.cleanupError.resources, [cleanup]);
    assert.equal(report.cleanupError.errors[0].code, "BROWSER_PROOF_INJECTED_CLEANUP");
  } else assert.equal(report.cleanupError, undefined);
}

for (const after of ["host", "runtime", "authority"]) {
  test(`serialized actual browser acquisition path cleans up after ${after}`, async () => {
    for (const cleanup of [undefined, after]) {
      assertScenario(await executeScenario({ after, cleanup }), after, cleanup);
    }
  });
}

test("serialized normal browser path proves attached authority, rollback and invalidation", async () => {
  const { report, handles, elements, updates } = await executeScenario();
  assert.doesNotThrow(() => verifyBrowserConsumer(report));
  assert.ok(handles.every((handle) => handle.__wbg_ptr === 0));
  assert.ok(elements.every((element) => !element.isConnected));
  assert.deepEqual(updates, [
    { kind: "observed", revision: 1n, ids: [1], surfaces: [255, 255, 255] },
    { kind: "observed", revision: 2n, ids: [1], surfaces: [255, 255, 255] },
    { kind: "observed", revision: 2n, ids: [1], surfaces: [255, 255, 255] },
    { kind: "unknown", revision: 3n, reasonId: 77 },
  ]);
});

for (const [name, before, after] of [
  ["error narrowing loss", "rejected: api.isProgramError(error)", "rejected: false"],
  ["independent composite constant", "const independentComposite = source.map((channel) => Math.round(255 + authority1.opacity * (channel - 255)));", "const independentComposite = [0, 0, 0];"],
  ["source substituted for final composite", "const composite = Array.from(authority1.caseComposite(0));", "const composite = [...source];"],
  ["CSS RGB drift", "rgb(${r} ${g} ${b}", "rgb(${r + 1} ${g} ${b}"],
  ["host rejection bypass", "if (state.rejectNext) {", "if (false && state.rejectNext) {"],
  ["published sequence freeze", "state.sequence = intent.desiredSequence;", "state.sequence = intent.expectedSequence;"],
  ["old-authority stale check loss", "const oldAfterRetry = capture(() => runtime.validateAuthority(authority1));", "const oldAfterRetry = { rejected: false };"],
  ["stale revoke loss", 'publish(intent, [], "");', "publish(intent, state.patch, state.cssText);"],
  ["renderer provenance overclaim", "rendererProvenance: authority1.rendererProvenance,", 'rendererProvenance: "observed",'],
  ["foreign identity check loss", "const changedIdentity = capture(() => changedIdentityRuntime.validateAuthority(authority1));", "const changedIdentity = { rejected: false };"],
]) {
  test(`production attached oracle kills ${name} sabotage`, async () => {
    const { report } = await executeScenario(undefined, (source) => {
      assert.ok(source.includes(before), "sabotage must reach the serialized execution path");
      return source.replace(before, after);
    });
    assert.throws(() => verifyBrowserConsumer(report), /browser consumer result drifted/u);
  });
}

for (const [name, before, after] of [
  ["missing authority release", "freeHandles(authorityHandles);", "void authorityHandles;"],
  ["wrong order", "const outcome = browserCleanup(primary, resources.toReversed());", "const outcome = browserCleanup(primary, resources);"],
  ["primary masking", "browserCleanup(primary, resources.toReversed())", "browserCleanup(undefined, resources.toReversed())"],
]) {
  test(`actual attached browser path oracle kills ${name} sabotage`, async () => {
    const execution = await executeScenario({ after: "authority", cleanup: "authority" }, (source) => {
      assert.ok(source.includes(before), "sabotage must reach the serialized execution path");
      return source.replace(before, after);
    });
    const detected = (() => {
      try {
        assertScenario(execution, "authority", "authority");
        return false;
      } catch {
        return true;
      }
    })();
    assert.equal(detected, true);
  });
}

'''
text = text[:start] + replacement + text[end:]
path.write_text(text)
