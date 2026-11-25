/** Exercises the consumer's actual Canvas 2D compositor and the published authoring API. */
export async function verifyRasterRestoration(api) {
  const colors = api.colors({ reference: { base: "#a0a0a0" } });
  const base = colors.surface("base");
  const layer = colors.role("film", "#505050", { domain: "neutral", maxChange: 0.25 });
  const program = await colors.compile({ film: layer.on(base, { opacity: 0.5 }) });
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 40;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  let surround = 230;
  const draw = (frame) => {
    context.fillStyle = `rgb(${surround} ${surround} ${surround})`;
    context.fillRect(0, 0, 64, 40);
    const base = frame.context.base;
    context.fillStyle = Array.isArray(base) ? `rgb(${base.join(" ")})` : `rgb(${base.from.join(" ")})`;
    context.fillRect(5, 6, 48, 23);
    context.fillStyle = frame.values.film.css;
    context.fillRect(5, 6, 48, 23);
    return context.getImageData(0, 0, 64, 40);
  };
  const count = (a, b) => {
    let pixels = 0;
    for (let i = 0; i < a.length; i += 4) {
      if (a[i] !== b[i] || a[i + 1] !== b[i + 1] || a[i + 2] !== b[i + 2] || a[i + 3] !== b[i + 3]) pixels++;
    }
    return pixels;
  };
  let reference, limited, protectedReference, layered, layeredReference;
  try {
    reference = program.captureRaster(draw);
    const original = reference.resolve({ base: "#a0a0a0" });
    const ordinary = program.resolve({ base: "#8c8c8c" });
    const originalBytes = original.image.data;
    const ordinaryChanges = count(originalBytes, draw(ordinary).data);
    const restored = reference.resolve({ base: "#8c8c8c" });
    const error = count(originalBytes, restored.image.data);
    surround--;
    const different = reference.resolve({ base: "#8c8c8c" });
    const reverted = different.frame.values.film.rgb.join() === ordinary.values.film.rgb.join();
    const unknown = reference.resolve({ base: null });
    surround++;
    const repeated = reference.resolve({ base: "#8c8c8c" });
    const order = [original.frame, restored.frame, different.frame, unknown.proposed, repeated.frame]
      .map((frame) => frame.revision);
    if (!order.every((revision, i) => i === 0 || revision === order[i - 1] + 1n)
      || repeated.relation !== "restored") throw new Error("raster resolution lost its current revision");

    const rounded = reference.resolve({ base: "#9f9f9f" });
    if (rounded.relation !== "restored" || rounded.frame.values.film.rgb[0] !== 81
      || count(originalBytes, rounded.image.data) !== 0) throw new Error("canvas quantization lost an exact material");

    const affine = reference.resolve({ base: { kind: "affine-srgb8", from: "#a0a0a0", to: "#a0a0a0" } });
    if (affine.relation !== "preserved" || affine.frame.state !== "modeled") {
      throw new Error("the renderer bitmap must remain comparable for a modeled ready frame");
    }

    const layers = api.colors({ reference: { base: "#a0a0a0", outside: "#000000" } });
    const outside = layers.surface("outside");
    const film = layers.role("film", "#505050", { domain: "neutral", maxChange: 0.25 });
    const panel = layers.role("panel", "#000000", { domain: "neutral" });
    layered = await layers.compile({ film: film.on(layers.surface("base"), { opacity: 0.5 }),
      panel: panel.on(outside, { opacity: 0.5 }) });
    layeredReference = layered.captureRaster((frame) => {
      context.fillStyle = frame.values.panel.css;
      context.fillRect(0, 0, 64, 40);
      return draw(frame); // The opaque scene covers the panel in this renderer.
    });
    const occluded = layeredReference.resolve({ base: "#8c8c8c", outside: "#ffffff" });
    if (occluded.relation !== "restored" || count(originalBytes, occluded.image.data) !== 0) {
      throw new Error("an unreachable, covered paint blocked restoration of the actual bitmap");
    }

    const protectedSystem = api.colors({ reference: { base: "#c8c8c8" } });
    const protectedBase = protectedSystem.surface("base");
    limited = await protectedSystem.compile({ film: protectedSystem.role("film", "#000000", { domain: "neutral" })
      .on(protectedBase, { opacity: 0.75, contrast: "wcag22-ui" }) });
    protectedReference = limited.captureRaster(draw);
    const protectedResult = protectedReference.resolve({ base: "#6e6e6e" });
    const paint = limited.resolve({ base: "#6e6e6e" }).values.film.rgb;
    if (protectedResult.relation !== "unavailable" || protectedResult.frame.values.film.rgb.join() !== paint.join()) {
      throw new Error("a raster proposal bypassed the declared WCAG condition");
    }
    const result = {
      original: original.relation, restored: restored.relation, changed: different.relation,
      ordinaryChanges, restoredChanges: error, reverted, unknown: unknown.state,
      originalPaint: original.frame.values.film.rgb[0], ordinaryPaint: ordinary.values.film.rgb[0],
      restoredPaint: restored.frame.values.film.rgb[0], opacity: restored.frame.values.film.opacity,
    };
    if (result.original !== "preserved" || result.restored !== "restored" || result.changed !== "unavailable"
      || ordinaryChanges !== 48 * 23 || error !== 0 || !reverted || result.unknown !== "unavailable"
      || result.ordinaryPaint === result.restoredPaint || result.originalPaint === result.restoredPaint
      || result.opacity !== 0.5) throw new Error(JSON.stringify(result));
    return result;
  } finally {
    layeredReference?.dispose();
    layered?.dispose();
    protectedReference?.dispose();
    limited?.dispose();
    reference?.dispose();
    program.dispose();
  }
}
