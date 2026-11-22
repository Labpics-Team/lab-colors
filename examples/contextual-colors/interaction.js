const sameDefinition = (left, right) => left?.blue === right.blue
  && left.warm === right.warm && left.opacity === right.opacity;

/** One pending control state; asynchronous compilation never publishes an obsolete definition. */
export function createColorInteraction({ compile, attach, onOutcome, onError, requestFrame, cancelFrame }) {
  let program, view, definition, pending, frame = 0, running = false, disposed = false;

  function queue() {
    if (!disposed && pending && !running && !frame) frame = requestFrame(flush);
  }

  async function flush() {
    frame = 0;
    if (disposed || running || !pending) return;
    const requested = pending;
    pending = undefined;
    running = true;
    let candidate, firstView;
    try {
      if (!sameDefinition(definition, requested)) {
        candidate = await compile(requested);
        if (disposed) return;
        // A changed surface can use the compiled definition; changed sources cannot.
        const latest = pending ?? requested;
        if (!sameDefinition(requested, latest)) return;
        pending = undefined;
        if (!view) {
          firstView = attach(candidate);
          const outcome = firstView.set({ canvas: latest.canvas });
          view = firstView;
          firstView = undefined;
          program = candidate;
          candidate = undefined;
          definition = latest;
          onOutcome(outcome);
        } else {
          const outcome = view.replace(candidate, { canvas: latest.canvas });
          if (outcome.state === "committed") {
            const previous = program;
            view = outcome.view;
            program = candidate;
            candidate = undefined;
            definition = latest;
            previous.dispose();
          }
          onOutcome(outcome);
        }
      } else {
        onOutcome(view.set({ canvas: requested.canvas }));
      }
    } catch (error) {
      if (!disposed && (!pending || sameDefinition(requested, pending))) onError(error);
    } finally {
      firstView?.dispose();
      candidate?.dispose();
      running = false;
      queue();
    }
  }

  return {
    get current() { return view?.current ?? null; },
    schedule(state) {
      if (disposed) return;
      pending = Object.freeze({ ...state });
      queue();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      pending = undefined;
      cancelFrame(frame);
      frame = 0;
      view?.dispose();
      program?.dispose();
    },
  };
}
