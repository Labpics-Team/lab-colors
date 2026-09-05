function fail(message) {
  throw new Error(`Program browser proof: ${message}`);
}

export function observeChildErrors(child) {
  let observed;
  let notify;
  const failure = new Promise((accept) => { notify = accept; });
  const onError = (error) => {
    observed = error;
    notify(error);
  };
  child.on("error", onError);
  return {
    failure,
    get observed() { return observed; },
    release: () => child.removeListener("error", onError),
  };
}

async function signalAndWait(child, signal, timeoutMs) {
  return new Promise((accept, reject) => {
    const cleanup = () => {
      clearTimeout(timer);
      child.removeListener("error", onError);
      child.removeListener("exit", onExit);
    };
    const onError = (error) => {
      cleanup();
      reject(error);
    };
    const onExit = () => {
      cleanup();
      accept(true);
    };
    const timer = setTimeout(() => {
      cleanup();
      accept(false);
    }, timeoutMs);
    child.once("error", onError);
    child.once("exit", onExit);
    if (!child.kill(signal)) {
      cleanup();
      reject(new Error(`Program browser proof: ChromeDriver rejected ${signal}`));
    }
  });
}

export async function terminateChild(child, timeoutMs = 5_000) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  if (await signalAndWait(child, "SIGTERM", timeoutMs)) return;
  if (child.exitCode !== null || child.signalCode !== null) return;
  if (await signalAndWait(child, "SIGKILL", timeoutMs)) return;
  throw new Error("ChromeDriver survived forced termination");
}

export async function waitForDriver(signal, port, child, errors) {
  const polling = new AbortController();
  const combinedSignal = AbortSignal.any([signal, polling.signal]);
  let releaseExit = () => {};
  const exited = new Promise((accept) => {
    if (typeof child.once !== "function") return;
    const onExit = (code, exitSignal) => accept({
      kind: "failure",
      error: new Error(`Program browser proof: ChromeDriver exited prematurely with code ${code ?? exitSignal ?? "unknown"}`),
    });
    child.once("exit", onExit);
    releaseExit = () => child.removeListener("exit", onExit);
  });
  const readiness = (async () => {
    const deadline = Date.now() + 20_000;
    while (Date.now() < deadline) {
      if (child.killed || child.exitCode !== null || child.signalCode !== null) {
        const outcome = child.exitCode ?? child.signalCode ?? "unknown";
        fail(`ChromeDriver exited prematurely with code ${outcome}`);
      }
      try {
        const response = await fetch(`http://127.0.0.1:${port}/status`, { signal: combinedSignal });
        if (response.ok && (await response.json())?.value?.ready === true) return { kind: "ready" };
      } catch (error) {
        if (combinedSignal.aborted) throw error;
      }
      await new Promise((accept) => setTimeout(accept, 50));
    }
    fail("ChromeDriver did not become ready");
  })();
  try {
    const outcome = await Promise.race([
      readiness,
      errors.failure.then((error) => ({ kind: "failure", error })),
      exited,
    ]);
    if (outcome.kind === "failure") throw outcome.error;
  } finally {
    polling.abort();
    releaseExit();
    await readiness.catch(() => {});
  }
}
