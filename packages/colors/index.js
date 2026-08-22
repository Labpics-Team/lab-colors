// Terminal C7c public entry for @labpics/colors.
//
// One runtime root: canonical Program wire -> ProgramRuntime -> ProgramSnapshot.
// Retired recipe engines and browser helper
// roots are intentionally not exported.

import initWasm, { initSync as initWasmSync } from "./pkg/labcolors.js";

let initState = "idle";
let initFlight;

export {
  compileProgramWire,
  evaluateWcag22,
  numericalCapabilityManifest,
  ProgramRuntime,
  ProgramSnapshot,
} from "./pkg/labcolors.js";

// wasm-bindgen returns every raw export from its loaders. The public facade
// deliberately erases that value: initialization is an effect, not a second
// uncurated ABI beside the typed package surface.
export function init(input) {
  if (initState === "ready") return Promise.resolve();
  if (initState === "async") return initFlight;
  if (initState === "starting") {
    throw new Error("Lab Colors: initialization input admission is in progress");
  }
  if (initState === "sync") {
    throw new Error("Lab Colors: synchronous initialization is in progress");
  }

  let resolveFlight;
  let rejectFlight;
  initFlight = new Promise((resolve, reject) => {
    resolveFlight = resolve;
    rejectFlight = reject;
  });
  const flight = initFlight;
  initState = "starting";

  let pending;
  try {
    pending = initWasm(input);
  } catch (error) {
    initState = "idle";
    initFlight = undefined;
    rejectFlight(error);
    return flight;
  }
  initState = "async";
  Promise.resolve(pending).then(
    () => {
      initState = "ready";
      resolveFlight();
    },
    (error) => {
      initState = "idle";
      initFlight = undefined;
      rejectFlight(error);
    },
  );
  return flight;
}

export function initSync(input) {
  if (initState === "ready") return;
  if (initState === "async") throw new Error("Lab Colors: asynchronous initialization is in progress");
  if (initState === "starting") throw new Error("Lab Colors: initialization input admission is in progress");
  if (initState === "sync") throw new Error("Lab Colors: synchronous initialization is in progress");
  initState = "sync";
  try {
    initWasmSync(input);
    initState = "ready";
  } catch (error) {
    initState = "idle";
    throw error;
  }
}

export default init;
