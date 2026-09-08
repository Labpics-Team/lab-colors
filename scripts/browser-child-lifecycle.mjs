function fail(message) {
  throw new Error(`Program browser proof: ${message}`);
}

export function observeChildErrors(child) {
  let operationalError;
  let consumedOperationalError;
  let exited = child.exitCode !== null || child.signalCode !== null;
  let closed = false;
  let resolveFailure;
  let resolveExit;
  let resolveClose;
  const failure = new Promise((accept) => { resolveFailure = accept; });
  const exit = new Promise((accept) => { resolveExit = accept; });
  const close = new Promise((accept) => { resolveClose = accept; });
  const onError = (error) => {
    if (operationalError === undefined) {
      operationalError = error;
      resolveFailure(error);
    }
  };
  const onExit = () => {
    exited = true;
    resolveExit();
  };
  const onClose = () => {
    exited = true;
    closed = true;
    resolveExit();
    resolveClose();
  };
  child.on("error", onError);
  child.on("exit", onExit);
  child.on("close", onClose);
  return {
    failure,
    exit,
    close,
    consumeOperationalError(error) {
      if (operationalError === error) consumedOperationalError = error;
    },
    get cleanupOperationalError() {
      return operationalError === consumedOperationalError ? undefined : operationalError;
    },
    get operationalError() { return operationalError; },
    get exited() { return exited; },
    get closed() { return closed; },
    release: () => {
      child.removeListener("error", onError);
      child.removeListener("exit", onExit);
      child.removeListener("close", onClose);
    },
  };
}

async function settlesWithin(promise, timeoutMs) {
  let timer;
  const timeout = new Promise((accept) => { timer = setTimeout(() => accept(false), timeoutMs); });
  try {
    return await Promise.race([promise.then(() => true), timeout]);
  } finally {
    clearTimeout(timer);
  }
}

async function signalAndObserveClose(child, errors, signal, timeoutMs) {
  let signalFailure;
  try {
    if (!child.kill(signal)) {
      signalFailure = new Error(`Program browser proof: ChromeDriver rejected ${signal}`);
    }
  } catch (error) {
    signalFailure = error;
  }
  if (errors.closed || await settlesWithin(errors.close, timeoutMs)) return { closed: true };
  return { closed: false, signalFailure };
}

async function terminateObservedChild(child, errors, timeoutMs) {
  if (errors.closed) return;
  if (errors.exited) {
    if (await settlesWithin(errors.close, timeoutMs)) return;
    throw new Error("Program browser proof: ChromeDriver exited without closing its process resources");
  }

  const term = await signalAndObserveClose(child, errors, "SIGTERM", timeoutMs);
  if (term.closed) return;

  const kill = await signalAndObserveClose(child, errors, "SIGKILL", timeoutMs);
  if (kill.closed) return;
  if (kill.signalFailure !== undefined) throw kill.signalFailure;
  throw new Error("Program browser proof: ChromeDriver survived forced termination");
}

export async function terminateChild(child, timeoutMs = 5_000) {
  const errors = observeChildErrors(child);
  try {
    await terminateObservedChild(child, errors, timeoutMs);
  } finally {
    errors.release();
  }
}

export function releaseChild(child, errors, timeoutMs = 5_000) {
  if (errors.releasePromise !== undefined) return errors.releasePromise;
  errors.releasePromise = (async () => {
    try {
      await terminateObservedChild(child, errors, timeoutMs);
    } catch (cleanupError) {
      const operationalError = errors.cleanupOperationalError;
      if (operationalError === undefined || operationalError === cleanupError) throw cleanupError;
      throw new AggregateError(
        [operationalError, cleanupError],
        "ChromeDriver failed before and during termination",
        { cause: operationalError },
      );
    } finally {
      errors.release();
    }
    const operationalError = errors.cleanupOperationalError;
    if (operationalError !== undefined) throw operationalError;
  })();
  return errors.releasePromise;
}

export async function waitForDriver(signal, port, child, errors) {
  const polling = new AbortController();
  const pollingSignal = AbortSignal.any([signal, polling.signal]);
  const terminal = errors.exit.then(() => {
    const outcome = child.exitCode ?? child.signalCode ?? "unknown";
    return new Error(`Program browser proof: ChromeDriver exited prematurely with code ${outcome}`);
  });
  const readiness = (async () => {
    while (!pollingSignal.aborted) {
      if (errors.exited) {
        const outcome = child.exitCode ?? child.signalCode ?? "unknown";
        fail(`ChromeDriver exited prematurely with code ${outcome}`);
      }
      try {
        const response = await fetch(`http://127.0.0.1:${port}/status`, { signal: pollingSignal });
        if (response.ok && (await response.json())?.value?.ready === true) return;
      } catch (error) {
        if (pollingSignal.aborted) throw error;
      }
      await new Promise((accept) => setTimeout(accept, 50));
    }
    throw pollingSignal.reason;
  })();
  try {
    const outcome = await Promise.race([
      readiness.then(() => new Promise((accept) => setImmediate(accept))),
      errors.failure,
      terminal,
    ]);
    if (outcome instanceof Error) {
      errors.consumeOperationalError(outcome);
      throw outcome;
    }
  } finally {
    polling.abort();
    await readiness.catch(() => {});
  }
}
