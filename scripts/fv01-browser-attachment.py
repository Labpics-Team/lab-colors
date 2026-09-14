from pathlib import Path

path = Path("scripts/test-program-runtime-browser.mjs")
text = path.read_text()
old_order = '''const RESOURCE_ORDER = [
  "temp-install",
  "browser",
  "browser-session",
  "server",
  "runtime",
  "snapshot",
  "host",
];
'''
new_order = '''const RESOURCE_ORDER = [
  "temp-install",
  "browser",
  "browser-session",
  "server",
  "host",
  "runtime",
  "authority",
];
'''
if text.count(old_order) != 1:
    raise SystemExit("resource order anchor changed")
text = text.replace(old_order, new_order, 1)

start = text.find("async function browserConsumer(origin, fault) {")
end = text.find("\nexport async function runBrowserProof", start)
if start < 0 or end < 0:
    raise SystemExit("browser consumer anchors changed")
consumer = r'''async function browserConsumer(origin, fault) {
  const resources = [];
  const evidence = { acquired: [], released: [], readback: {} };
  const runtimeHandles = [];
  const authorityHandles = [];
  let primary, result, element;

  const freeHandles = (handles) => {
    let firstError;
    for (const handle of handles.toReversed()) {
      try {
        if (handle?.__wbg_ptr !== 0) handle.free();
      } catch (error) {
        firstError ??= error;
      }
    }
    if (firstError !== undefined) throw firstError;
  };

  try {
    const api = await import(`${origin}/index.js`);
    const wire = await import(`${origin}/program-wire/abi-v1.js`);
    await api.init({ module_or_path: fetch(`${origin}/pkg/labcolors_bg.wasm`) });

    element = document.getElementById("proof");
    if (!element) throw new Error("proof element is absent");

    const hostFailure = (code, message) => Object.assign(new Error(message), { code });
    const makeHost = (target) => {
      const state = {
        bindingEpoch: null,
        sequence: 0n,
        revision: null,
        patch: [],
        cssText: "",
        rejectNext: false,
        disposed: false,
      };
      const normalizePatch = (patch) => {
        if (!Array.isArray(patch) || patch.length !== 1) {
          throw hostFailure("HOST_PATCH_SCOPE_MISMATCH", "host patch must cover the exact one-output scope");
        }
        const seen = new Set();
        return patch.map((entry) => {
          if (entry?.output !== 91 || entry?.sinkOutput !== 3 || seen.has(entry.sinkOutput)) {
            throw hostFailure("HOST_PATCH_SCOPE_MISMATCH", "host patch scope or output identity drifted");
          }
          seen.add(entry.sinkOutput);
          if (!(entry.source instanceof Uint8Array) || entry.source.length !== 3
            || !Number.isFinite(entry.opacity) || entry.opacity < 0 || entry.opacity > 1) {
            throw hostFailure("HOST_PATCH_INVALID", "host patch payload is invalid");
          }
          return {
            output: entry.output,
            sinkOutput: entry.sinkOutput,
            source: Array.from(entry.source),
            opacity: entry.opacity,
          };
        });
      };
      const samePatch = (left, right) => JSON.stringify(left) === JSON.stringify(right);
      const checkEpoch = (intent) => {
        if (typeof intent.bindingEpoch !== "bigint") {
          throw hostFailure("HOST_STAMP_MISMATCH", "binding epoch must be bigint");
        }
        if (state.bindingEpoch !== null && intent.bindingEpoch !== state.bindingEpoch) {
          throw hostFailure("HOST_STAMP_MISMATCH", "binding epoch changed");
        }
      };
      const checkMutation = (intent) => {
        checkEpoch(intent);
        if (intent.expectedSequence !== state.sequence
          || intent.desiredSequence !== state.sequence + 1n) {
          throw hostFailure("HOST_STAMP_MISMATCH", "mutation sequence mismatch");
        }
        if (typeof intent.revision !== "bigint"
          || (state.revision !== null && intent.revision <= state.revision)) {
          throw hostFailure("HOST_REVISION_MISMATCH", "mutation revision is not monotonic");
        }
      };
      const publish = (intent, patch, cssText) => {
        if (state.rejectNext) {
          state.rejectNext = false;
          throw hostFailure("HOST_REJECTED", "injected host rejection before publication");
        }
        target.style.cssText = cssText;
        state.bindingEpoch ??= intent.bindingEpoch;
        state.sequence = intent.desiredSequence;
        state.revision = intent.revision;
        state.patch = patch;
        state.cssText = target.getAttribute("style") ?? "";
      };
      return {
        rejectNextInstall() { state.rejectNext = true; },
        snapshot() {
          return {
            bindingEpoch: state.bindingEpoch?.toString() ?? null,
            sequence: state.sequence.toString(),
            revision: state.revision?.toString() ?? null,
            patch: state.patch.map((entry) => ({ ...entry, source: [...entry.source] })),
            cssText: target.getAttribute("style") ?? "",
          };
        },
        tryInstall(intent) {
          if (!intent || typeof intent.kind !== "string") {
            throw hostFailure("HOST_INTENT_INVALID", "host intent is invalid");
          }
          if (intent.kind === "set-all") {
            checkMutation(intent);
            const patch = normalizePatch(intent.patch);
            const entry = patch[0];
            const [r, g, b] = entry.source;
            const cssText = `--consumer-color: rgb(${r} ${g} ${b} / ${entry.opacity});`;
            publish(intent, patch, cssText);
            return;
          }
          if (intent.kind === "revoke-all") {
            checkMutation(intent);
            publish(intent, [], "");
            return;
          }
          if (intent.kind === "confirm-exact") {
            checkEpoch(intent);
            const patch = normalizePatch(intent.patch);
            if (intent.publishedSequence !== state.sequence || intent.revision !== state.revision) {
              throw hostFailure("HOST_STAMP_MISMATCH", "confirmation stamp changed");
            }
            if (!samePatch(patch, state.patch)
              || (target.getAttribute("style") ?? "") !== state.cssText) {
              throw hostFailure("HOST_CONFIRMATION_MISMATCH", "confirmed host snapshot drifted");
            }
            return;
          }
          throw hostFailure("HOST_INTENT_INVALID", `unknown host intent ${intent.kind}`);
        },
        free() {
          if (state.disposed) return;
          target.style.cssText = "";
          target.remove();
          state.disposed = true;
        },
      };
    };

    const host = makeHost(element);
    resources.push({
      name: "host",
      release() {
        host.free();
        host.free();
        released(evidence, "host", fault);
      },
    });
    acquisition(evidence, "host", fault);

    const buildWire = (adaptingLuminance = 64) => new wire.ProgramWireBuilderV1()
      .source(11, [64, 64, 64]).fixedTarget(21, 11).surfaceInputPort(31).opacityInput(32, 0.5)
      .solidPaint(41, 21).opacityPaint(42, 41, 32).inputSurface(51, 31)
      .sourceOverOccurrence(61, 42, 51, adaptingLuminance, 0.2, wire.SURROUND_AVERAGE_V1)
      .presentationRoot(71, 61).presentationTarget(71, 61)
      .exactVisibleUnary(true, 81, 61, [160, 160, 160]).output(91, 42)
      .finish();
    const buildConsumedRootWire = () => new wire.ProgramWireBuilderV1()
      .source(1, [64, 64, 64]).fixedTarget(2, 1).surfaceInputPort(3).opacityInput(4, 0.5)
      .solidPaint(5, 2).opacityPaint(6, 5, 4).inputSurface(7, 3)
      .sourceOverOccurrence(8, 6, 7, 64, 0.2, wire.SURROUND_AVERAGE_V1)
      .presentationRoot(9, 8).presentationTarget(9, 8)
      .occurrenceSurface(10, 8)
      .sourceOverOccurrence(11, 6, 10, 64, 0.2, wire.SURROUND_AVERAGE_V1)
      .exactVisibleUnary(true, 12, 11, [160, 160, 160]).output(13, 6)
      .finish();
    const bindings = {
      emissionOutputs: new Uint32Array([91]),
      emissionSinkOutputs: new Uint32Array([3]),
      presentationOutputs: new Uint32Array([91]),
      presentationRoots: new Uint32Array([71]),
      presentationOccurrences: new Uint32Array([61]),
    };
    const scenarioIds = new Uint32Array([1]);
    const whiteBackdrop = new Uint8Array([255, 255, 255]);
    const bytes = buildWire();
    const compiled = api.compileAttachedProgramWire(bytes);
    runtimeHandles.push(compiled);
    const runtime = compiled.attach(1, bindings, host);
    runtimeHandles.push(runtime);
    resources.push({
      name: "runtime",
      release() {
        freeHandles(runtimeHandles);
        released(evidence, "runtime", fault);
      },
    });
    acquisition(evidence, "runtime", fault);

    const update1 = runtime.updateObserved(1n, scenarioIds, whiteBackdrop);
    authorityHandles.push(update1);
    if (update1.state !== "ready" || update1.authorityCount() !== 1) {
      throw new Error("first attached update did not publish exactly one Ready authority");
    }
    const authority1 = update1.takeAuthority(0);
    authorityHandles.push(authority1);
    resources.push({
      name: "authority",
      release() {
        freeHandles(authorityHandles);
        released(evidence, "authority", fault);
      },
    });
    acquisition(evidence, "authority", fault);

    runtime.validateAuthority(authority1);
    const source = Array.from(authority1.sourceRgb());
    const composite = Array.from(authority1.caseComposite(0));
    const independentComposite = source.map((channel) => Math.round(255 + authority1.opacity * (channel - 255)));
    const rgba = (value) => {
      const channels = value.match(/[\d.]+/g)?.map(Number);
      if (!channels || channels.length < 3) throw new Error("computed color was not RGB");
      return [channels[0], channels[1], channels[2], channels[3] ?? 1];
    };
    const computed = rgba(getComputedStyle(element).color);
    const readyHost = host.snapshot();

    const capture = (operation) => {
      try {
        operation();
        return { rejected: false };
      } catch (error) {
        return {
          rejected: api.isProgramError(error),
          code: error?.code,
          operation: error?.operation,
          causeCode: error?.cause?.code,
        };
      }
    };

    host.rejectNextInstall();
    const rejectionBefore = host.snapshot();
    const hostRejection = capture(() => {
      const unexpected = runtime.updateObserved(2n, scenarioIds, whiteBackdrop);
      authorityHandles.push(unexpected);
    });
    const rejectionAfter = host.snapshot();
    let oldAuthorityValidAfterReject = true;
    try { runtime.validateAuthority(authority1); }
    catch { oldAuthorityValidAfterReject = false; }

    const update2 = runtime.updateObserved(2n, scenarioIds, whiteBackdrop);
    authorityHandles.push(update2);
    const authority2 = update2.takeAuthority(0);
    authorityHandles.push(authority2);
    runtime.validateAuthority(authority2);
    const oldAfterRetry = capture(() => runtime.validateAuthority(authority1));

    const foreignEpochHost = makeHost(document.createElement("div"));
    const foreignEpochRuntime = compiled.attach(2, bindings, foreignEpochHost);
    runtimeHandles.push(foreignEpochRuntime);
    const foreignEpoch = capture(() => foreignEpochRuntime.validateAuthority(authority1));

    const sameBytesOwner = api.compileAttachedProgramWire(bytes);
    runtimeHandles.push(sameBytesOwner);
    const foreignOwnerRuntime = sameBytesOwner.attach(3, bindings, makeHost(document.createElement("div")));
    runtimeHandles.push(foreignOwnerRuntime);
    const foreignOwner = capture(() => foreignOwnerRuntime.validateAuthority(authority1));

    const changedIdentityOwner = api.compileAttachedProgramWire(buildWire(65));
    runtimeHandles.push(changedIdentityOwner);
    const changedIdentityRuntime = changedIdentityOwner.attach(4, bindings, makeHost(document.createElement("div")));
    runtimeHandles.push(changedIdentityRuntime);
    const changedIdentity = capture(() => changedIdentityRuntime.validateAuthority(authority1));

    const consumedRoot = capture(() => {
      const unexpected = api.compileAttachedProgramWire(buildConsumedRootWire());
      runtimeHandles.push(unexpected);
    });

    const staleUpdate = runtime.updateUnknown(3n, 77);
    authorityHandles.push(staleUpdate);
    const staleHost = host.snapshot();

    result = {
      ready: {
        state: update1.state,
        authorityCount: 1,
        source,
        composite,
        independentComposite,
        computed,
        cssText: readyHost.cssText,
        host: readyHost,
        publishedRevision: authority1.publishedRevision.toString(),
        sinkSequence: authority1.sinkSequence.toString(),
        sinkBindingEpoch: authority1.sinkBindingEpoch.toString(),
        presentationRoot: authority1.presentationRoot,
        presentationOccurrence: authority1.presentationOccurrence,
        terminalOccurrence: authority1.terminalOccurrence,
        appearanceSurround: authority1.appearanceSurround,
        rendererProvenance: authority1.rendererProvenance,
        caseCount: authority1.caseCount(),
        caseIndex: authority1.caseIndex(0),
      },
      rejection: {
        error: hostRejection,
        before: rejectionBefore,
        after: rejectionAfter,
        oldAuthorityValid: oldAuthorityValidAfterReject,
      },
      retry: {
        state: update2.state,
        newSequence: authority2.sinkSequence.toString(),
        newRevision: authority2.publishedRevision.toString(),
        oldAuthorityError: oldAfterRetry,
      },
      stale: {
        state: staleUpdate.state,
        authorityCount: staleUpdate.authorityCount(),
        host: staleHost,
      },
      foreign: { epoch: foreignEpoch, owner: foreignOwner, identity: changedIdentity },
      consumedRoot,
    };
  } catch (error) {
    primary = error;
  }

  const outcome = browserCleanup(primary, resources.toReversed());
  if (runtimeHandles.length > 0) {
    evidence.readback.runtime = runtimeHandles.every((handle) => handle.__wbg_ptr === 0);
  }
  if (authorityHandles.length > 0) {
    evidence.readback.authority = authorityHandles.every((handle) => handle.__wbg_ptr === 0);
  }
  if (element) {
    evidence.readback.host = !element.isConnected && element.style.cssText === "";
  }
  return { ...result, ...outcome, ...evidence };
}
'''
text = text[:start] + consumer + text[end:]

verify_start = text.find("export function verifyBrowserConsumer(result) {")
verify_end = text.find("\nasync function main()", verify_start)
if verify_start < 0 or verify_end < 0:
    raise SystemExit("browser verifier anchors changed")
verifier = r'''export function verifyBrowserConsumer(result) {
  const equal = isDeepStrictEqual;
  const ready = result.ready;
  const rejection = result.rejection;
  const retry = result.retry;
  const stale = result.stale;
  const foreign = result.foreign;
  const epochIsNonZero = typeof ready?.sinkBindingEpoch === "string" && /^[1-9][0-9]*$/u.test(ready.sinkBindingEpoch);
  if (result.error || result.cleanupError
    || ready?.state !== "ready" || ready.authorityCount !== 1
    || !equal(ready.source, [64, 64, 64])
    || !equal(ready.composite, [160, 160, 160])
    || !equal(ready.independentComposite, [160, 160, 160])
    || equal(ready.source, ready.composite)
    || !equal(ready.computed, [64, 64, 64, 0.5])
    || typeof ready.cssText !== "string" || ready.cssText === ""
    || ready.publishedRevision !== "1" || ready.sinkSequence !== "1" || !epochIsNonZero
    || ready.host?.bindingEpoch !== ready.sinkBindingEpoch
    || ready.host?.sequence !== "1" || ready.host?.revision !== "1"
    || !equal(ready.host?.patch, [{ output: 91, sinkOutput: 3, source: [64, 64, 64], opacity: 0.5 }])
    || ready.presentationRoot !== 71 || ready.presentationOccurrence !== 61 || ready.terminalOccurrence !== 61
    || ready.appearanceSurround !== "average" || ready.rendererProvenance !== "unverified"
    || ready.caseCount !== 1 || ready.caseIndex !== 0
    || rejection?.error?.rejected !== true || rejection.error.code !== "attached_host"
    || rejection.error.operation !== "updateAttachedObserved" || rejection.error.causeCode !== "HOST_REJECTED"
    || !equal(rejection.before, rejection.after) || rejection.oldAuthorityValid !== true
    || retry?.state !== "ready" || retry.newSequence !== "2" || retry.newRevision !== "2"
    || retry.oldAuthorityError?.rejected !== true
    || retry.oldAuthorityError.code !== "attached_published_revision_mismatch"
    || retry.oldAuthorityError.operation !== "validateAttachedAuthority"
    || stale?.state !== "stale" || stale.authorityCount !== 0
    || stale.host?.sequence !== "3" || stale.host?.revision !== "3" || stale.host?.cssText !== ""
    || !equal(stale.host?.patch, [])
    || foreign?.epoch?.code !== "attached_foreign_binding_epoch"
    || foreign.epoch.operation !== "validateAttachedAuthority"
    || foreign?.owner?.code !== "attached_foreign_owner_generation"
    || foreign.owner.operation !== "validateAttachedAuthority"
    || foreign?.identity?.code !== "attached_program_identity_mismatch"
    || foreign.identity.operation !== "validateAttachedAuthority"
    || result.consumedRoot?.rejected !== true
    || result.consumedRoot.code !== "attached_root_consumed_downstream"
    || result.consumedRoot.operation !== "compileAttachedProgramWire") {
    fail(`browser consumer result drifted: ${JSON.stringify(result)}`);
  }
}
'''
text = text[:verify_start] + verifier + text[verify_end:]
path.write_text(text)
