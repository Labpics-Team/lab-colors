import { colors, type ColorProgram, type ColorView, type ColorUse, type ColorRole, type ColorAdaptiveRole, type ColorFixedRole, type ColorFixedUsePolicy, type ColorAttachOptions, type ColorCleanlinessAssessment } from "@labpics/colors";

async function contextualProgram(element: HTMLElement): Promise<void> {
  const c = colors(), canvas = c.surface("canvas");
  const accent: ColorAdaptiveRole = c.role("accent", "#0067F4");
  const neutral = c.role("label", "#242424", { domain: "neutral", identity: { lightness: [0, 1] } });
  const fill = accent.on(canvas, { separation: [0.04, 0.4] });
  const label = neutral.on(fill, { contrast: "wcag22-text" });
  const pane = accent.on(canvas, { opacity: 0.8, tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  const fixed: ColorFixedRole = c.fixed("paper", "#f4eadb");
  const fixedPolicy: ColorFixedUsePolicy = { opacity: 0.8, contrast: "wcag22-ui", separation: [0.02, 2] };
  const fixedUse = fixed.on(canvas, fixedPolicy);
  c.separate(fixedUse, pane, [0.08, 0.65]);
  // @ts-expect-error the author supplies both bounds of the distance.
  c.separate(fixedUse, pane);
  // @ts-expect-error relations compare applied colors.
  c.separate(accent, fixedUse, [0, 1]);
  fixedUse.role.on(canvas, { opacity: 0 });
  // @ts-expect-error a fixed source cannot use a tone adaptation curve.
  fixed.on(canvas, { tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  // @ts-expect-error a fixed use retains its fixed role's policy contract.
  fixedUse.role.on(canvas, { tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  const generalRole: ColorRole = fixed;
  const generalUse: ColorUse = generalRole.on(canvas, fixedPolicy);
  // @ts-expect-error annotating a fixed role as a general role does not enable adaptation.
  generalRole.on(canvas, { tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  // @ts-expect-error a general use may belong to a fixed role and cannot promise tone adaptation.
  generalUse.role.on(canvas, { tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  const placeRole = (role: ColorRole) => role.on(canvas, { contrast: "wcag22-ui", opacity: 0.8 });
  placeRole(accent); placeRole(fixed);
  pane.role.on(canvas, { tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  const program: ColorProgram<"fill" | "label" | "pane"> = await c.compile({ fill, label, pane });
  const frame = program.resolve({ canvas: "#e6ddd0" });
  if (frame.state === "ready" || frame.state === "modeled") {
    const rgb: readonly [number, number, number] = frame.values.fill.rgb;
    const completed: "exact" | "unfinished" = frame.optimality;
    const quality: ColorCleanlinessAssessment = frame.cleanliness;
    for (const selection of frame.reference) {
      const use: ColorUse = selection.use;
      const complete: boolean | undefined = selection.capacity?.complete;
      const contrast: number | undefined = selection.cases[0]?.actualContrast;
      void use; void complete; void contrast;
      // @ts-expect-error public evidence uses authored handles, not physical wire IDs.
      selection.targetId;
    }
    void quality;
    void rgb; void completed;
    // @ts-expect-error only declared aliases exist on a ready headless result.
    frame.values.missing;
  } else {
    for (const item of frame.diagnostics) {
      if (item.kind === "relation" && item.requirement === "visible-pair-separation") {
        const left: ColorUse = item.left.use;
        const right: readonly string[] = item.right.outputs;
        const range: readonly [number, number] = item.range;
        void left; void right; void range;
      }
    }
    // @ts-expect-error unavailable results have no current values.
    frame.values;
  }
  const attachmentOptions: ColorAttachOptions = { nonce: "application-supplied-nonce" };
  const modeled = program.resolve({ canvas: { kind: "affine-srgb8", from: "#ffffff", to: [221, 221, 221] } });
  if (modeled.state === "modeled") {
    const complete: "complete" = modeled.coverage.model;
    const rendering: "unknown" = modeled.coverage.rendering;
    const background: string = modeled.contextCss.canvas;
    const descriptor = modeled.context.canvas;
    const witness: readonly [number, number] | undefined = modeled.field.witnesses[0]?.coordinate;
    void complete; void rendering; void background; void descriptor; void witness;
    // @ts-expect-error a modeled frame is not an attach-ready frame.
    const ready: Extract<typeof frame, { state: "ready" }> = modeled;
    void ready;
  }
  // @ts-expect-error interpolation spaces beyond opaque encoded-sRGB are not implemented.
  program.resolve({ canvas: { kind: "affine-oklab", from: "#ffffff", to: "#000000" } });
  const view: ColorView<"fill" | "label" | "pane"> = program.attach(element, { canvas: "#283548" }, attachmentOptions);
  const preview = program.preview(element, undefined, attachmentOptions);
  const previewUpdate = preview.set({ canvas: { kind: "affine-srgb8", from: "#ffffff", to: "#dddddd" } });
  if (previewUpdate.state === "committed" && previewUpdate.frame.state === "modeled") {
    const coverage: "unknown" = previewUpdate.frame.coverage.rendering;
    void coverage;
  }
  const readyOnly: "ready" | undefined = view.current?.state;
  void readyOnly;
  // @ts-expect-error a CSP nonce is an explicit string, not a coerced value.
  program.attach(element, { canvas: "#283548" }, { nonce: 123 });
  view.set({ canvas: "#61766a" });
  const next = await c.compile({ primary: fill });
  const previewReplacement = preview.replace(next, { canvas: "#ffffff" });
  if (previewReplacement.state === "committed" && previewReplacement.view.current?.state === "modeled") {
    previewReplacement.view.current.values.primary;
    // @ts-expect-error a replaced preview uses the new program's aliases.
    previewReplacement.view.current.values.fill;
  }
  const fieldReplacement = view.replace(next, { canvas: { kind: "affine-srgb8", from: "#ffffff", to: "#dddddd" } });
  if (fieldReplacement.state === "rejected" && fieldReplacement.proposed.state === "modeled") {
    fieldReplacement.proposed.values.primary;
    // @ts-expect-error rejected replacement values use the proposed program's names.
    fieldReplacement.proposed.values.fill;
  }
  const replacement = view.replace(next, { canvas: "#e6ddd0" });
  if (replacement.state === "committed") {
    const current: ColorView<"primary"> = replacement.view;
    current.dispose();
  }
  view.dispose();
  next.dispose();
  program.dispose();

  // @ts-expect-error a role is not a physical local use.
  c.compile({ accent });
  // @ts-expect-error a role is not a background occurrence.
  accent.on(neutral);
  // @ts-expect-error tone follows an explicit input, not a resolved paint.
  accent.on(canvas, { tone: { from: fill, curve: [[0, 0.2], [1, 0.95]] } });
  // @ts-expect-error custom scalars are not a universal quality score.
  accent.on(canvas, { cleanliness: 0.8 });
  colors({ cleanliness: "nominal-srgb8-v2" });
  // @ts-expect-error an unimplemented universal profile cannot silently compile.
  colors({ cleanliness: "universal-cleanliness" });
  // @ts-expect-error role handles cannot be fabricated.
  const fakeRole: ColorRole = { id: "fake", anchor: [0, 0, 0], on: () => fill };
  // @ts-expect-error use handles cannot be fabricated.
  const fakeUse: ColorUse = { role: accent, against: canvas };
  void fakeRole; void fakeUse;
}
void contextualProgram;

async function authoredHierarchy(): Promise<void> {
  const c = colors({ reference: { canvas: "#ffffff" } });
  const canvas = c.surface("canvas");
  const heading = c.role("heading", "#444444").on(canvas, { contrast: "wcag22-text" });
  const description = c.role("description", "#666666").on(canvas, { contrast: "wcag22-text", opacity: 0.9 });
  c.preserveTonalHierarchy([heading, description]);
  // @ts-expect-error relations connect physical applications, not source roles.
  c.preserveTonalHierarchy([heading.role, description.role]);
  const program = await c.compile({ heading, description });
  program.resolve({ canvas: "#000000" });
  program.dispose();
}
void authoredHierarchy;
