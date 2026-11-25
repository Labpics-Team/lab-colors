const NONCE = "Y29sb3JzLWF1dGhvcmluZy1wcm9vZg==";
const CHECKS = {
  ownership: ["independent-modules", "independent-roots", "independent-dispose",
    "cascade-rollback", "cascade-recovery", "important-rollback", "important-recovery"],
  csp: ["csp-refusal"],
  nonce: ["nonce-acceptance", "field-preview", "preview-solid-update"],
};

/** The same installed tarball is served under two complete module URL trees. */
export function authoringBrowserCases(browserCleanup) {
  return Object.keys(CHECKS).map((mode) => ({
    name: mode,
    fixture: {
      html: '<!doctype html><link rel="stylesheet" href="/browser-fixture.css"><body></body>',
      stylesheet: '[data-authoring-proof]{color:var(--ink);background:var(--canvas)}',
      packageAlias: "/independent-copy",
      ...(mode === "ownership" ? {} : {
        contentSecurityPolicy: "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; "
          + "connect-src 'self'; style-src 'self'"
          + (mode === "nonce" ? ` 'nonce-${NONCE}'` : ""),
      }),
    },
    scenario(origin) {
      return `const done=arguments[arguments.length-1];
        const browserCleanup=${browserCleanup.toString()};
        (${browserAuthoringConsumer.toString()})(${JSON.stringify(origin)},${JSON.stringify(mode)},${JSON.stringify(NONCE)},browserCleanup)
          .then(proof=>done({proof}),error=>done({proof:browserCleanup(error,[])}));`;
    },
  }));
}

async function browserAuthoringConsumer(origin, mode, nonce, cleanup) {
  const resources = [];
  const evidence = { acquired: [], released: [], readback: {} };
  const result = { mode, checks: [] };
  let primary;
  const own = (name, handle, release, readback) => {
    evidence.acquired.push(name);
    resources.push({ name, release() { release(); evidence.released.push(name); }, readback });
    return handle;
  };
  const check = (name, condition, detail) => {
    if (!condition) throw new Error(`authoring ${name}: ${JSON.stringify(detail)}`);
    result.checks.push(name);
  };
  const read = (element) => {
    const computed = getComputedStyle(element);
    return { color: computed.color, canvas: computed.getPropertyValue("--canvas").trim(),
      ink: computed.getPropertyValue("--ink").trim(), scope: element.getAttribute("data-lab-colors-scope") };
  };
  const styleFor = (element) => [...document.querySelectorAll("style[data-lab-colors]")]
    .find((style) => style.getAttribute("data-lab-colors") === element.getAttribute("data-lab-colors-scope"));
  const makeProgram = async (factory, source, name) => {
    const colors = factory(), canvas = colors.surface("canvas");
    const program = await colors.compile({ ink: colors.fixed("ink", source).on(canvas) });
    return own(name, program, () => program.dispose(), () => {
      try { program.resolve({ canvas: "#ffffff" }); return false; }
      catch (error) { return error.code === "color_disposed"; }
    });
  };
  const attach = (program, element, name, options) => {
    const view = program.attach(element, { canvas: "#ffffff" }, options);
    return own(name, view, () => view.dispose(), () => view.current === null
      && !element.hasAttribute("data-lab-colors-scope") && styleFor(element) === undefined);
  };
  try {
    const host = document.createElement("div");
    document.body.appendChild(host);
    own("authoring-host", host, () => host.remove(), () => !host.isConnected);
    const first = document.createElement("section"), second = document.createElement("section");
    first.id = "authoring-first"; second.id = "authoring-second";
    first.setAttribute("data-authoring-proof", ""); second.setAttribute("data-authoring-proof", "");
    host.append(first, second);
    const api = await import(`${origin}/index.js`);
    const program = await makeProgram(api.colors, "#ff0000", "authoring-program");
    if (mode === "ownership") {
      const firstView = attach(program, first, "authoring-first-view");
      const independent = await import(`${origin}/independent-copy/index.js`);
      check("independent-modules", api.colors !== independent.colors);
      const otherProgram = await makeProgram(independent.colors, "#0000ff", "authoring-other-program");
      const secondView = attach(otherProgram, second, "authoring-second-view");
      const firstValue = read(first), secondValue = read(second);
      check("independent-roots", firstValue.color === "rgb(255, 0, 0)"
        && secondValue.color === "rgb(0, 0, 255)" && firstValue.scope !== secondValue.scope
        && firstView.current.values.ink.rgb.join() === "255,0,0"
        && secondView.current.values.ink.rgb.join() === "0,0,255", { firstValue, secondValue });
      const secondFrame = secondView.current;
      firstView.dispose();
      check("independent-dispose", firstView.current === null && read(first).scope === null
        && secondView.current === secondFrame && read(second).color === "rgb(0, 0, 255)");
      const external = document.createElement("style");
      document.head.appendChild(external);
      own("authoring-external-style", external, () => external.remove(), () => !external.isConnected);
      for (const important of [false, true]) {
        const name = important ? "important" : "cascade";
        const canvas = important ? "#ffffff" : "#000000";
        const expectedCanvas = important ? "rgb(255 255 255 / 1)" : "rgb(0 0 0 / 1)";
        const frame = secondView.current, lastUpdate = secondView.lastUpdate;
        const style = styleFor(second), text = style.textContent;
        external.textContent = `#authoring-second{--ink:rgb(0 128 0 / 1)${important ? "!important" : ""}}`;
        let rejection;
        try { secondView.set({ canvas }); } catch (error) { rejection = error; }
        check(`${name}-rollback`, read(second).color === "rgb(0, 128, 0)"
          && rejection?.code === "color_host_changed" && secondView.current === frame
          && secondView.lastUpdate === lastUpdate && style.textContent === text,
        { rejection: rejection?.code, actual: read(second) });
        external.textContent = "";
        const update = secondView.set({ canvas });
        check(`${name}-recovery`, update.state === "committed" && secondView.current !== frame
          && read(second).canvas === expectedCanvas && read(second).color === "rgb(0, 0, 255)", read(second));
      }
    } else if (mode === "csp") {
      let rejection;
      try { attach(program, first, "authoring-unexpected-view"); } catch (error) { rejection = error; }
      check("csp-refusal", rejection?.code === "color_host_changed" && read(first).scope === null
        && document.querySelectorAll("style[data-lab-colors]").length === 0,
      { rejection: rejection?.code, actual: read(first) });
    } else {
      const view = attach(program, first, "authoring-nonce-view", { nonce });
      const style = styleFor(first);
      check("nonce-acceptance", view.lastUpdate.state === "committed"
        && view.current.values.ink.rgb.join() === "255,0,0" && read(first).color === "rgb(255, 0, 0)"
        && style.nonce === nonce && style.sheet !== null && !style.sheet.disabled, read(first));
      const field = { kind: "affine-srgb8", from: "#f4eadb", to: "#d7c5a8" };
      const preview = program.preview(second, { canvas: field }, { nonce });
      own("authoring-preview", preview, () => preview.dispose(), () => preview.current === null && !styleFor(second));
      const computed = getComputedStyle(second);
      check("field-preview", preview.lastUpdate.state === "committed" && preview.current.state === "modeled"
        && preview.current.coverage.rendering === "unknown" && computed.backgroundImage.startsWith("linear-gradient(")
        && read(second).canvas === preview.current.contextCss.canvas && computed.color === "rgb(255, 0, 0)", read(second));
      const solid = preview.set({ canvas: "#ffffff" });
      check("preview-solid-update", solid.frame.state === "ready" && computed.backgroundImage === "none"
        && computed.backgroundColor === "rgb(255, 255, 255)", read(second));
    }
  } catch (error) { primary = error; }
  const outcome = cleanup(primary, resources.toReversed());
  for (const resource of resources) evidence.readback[resource.name] = resource.readback();
  evidence.readback["authoring-styles"] = document.querySelectorAll("style[data-lab-colors]").length === 0;
  return { ...result, ...outcome, ...evidence };
}

export function verifyBrowserAuthoring(result, mode) {
  const expected = CHECKS[mode];
  if (result.error || result.cleanupError || result.mode !== mode || expected === undefined
    || JSON.stringify(result.checks) !== JSON.stringify(expected)
    || JSON.stringify(result.released) !== JSON.stringify(result.acquired?.toReversed())
    || result.acquired.some((name) => result.readback?.[name] !== true)
    || result.readback?.["authoring-styles"] !== true) {
    throw new Error(`Program browser proof: authoring ${mode} failed: ${JSON.stringify(result)}`);
  }
}
