import { colors, type ColorProgram, type ColorView, type ColorUse, type ColorRole, type ColorFixedRole, type ColorFixedUsePolicy, type ColorAttachOptions, type ColorCleanlinessAssessment } from "@labpics/colors";

async function contextualProgram(element: HTMLElement): Promise<void> {
  const c = colors(), canvas = c.surface("canvas");
  const accent = c.role("accent", "#0067F4");
  const neutral = c.role("label", "#242424", { domain: "neutral", identity: { lightness: [0, 1] } });
  const fill = accent.on(canvas, { separation: [0.04, 0.4] });
  const label = neutral.on(fill, { contrast: "wcag22-text" });
  const pane = accent.on(canvas, { opacity: 0.8, tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  const fixed: ColorFixedRole = c.fixed("paper", "#f4eadb");
  const fixedPolicy: ColorFixedUsePolicy = { opacity: 0.8, contrast: "wcag22-ui", separation: [0.02, 2] };
  const fixedUse = fixed.on(canvas, fixedPolicy);
  fixedUse.role.on(canvas, { opacity: 0 });
  // @ts-expect-error a fixed source cannot use a tone adaptation curve.
  fixed.on(canvas, { tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  // @ts-expect-error a fixed use retains its fixed role's policy contract.
  fixedUse.role.on(canvas, { tone: { from: canvas, curve: [[0, 0.2], [1, 0.95]] } });
  const program: ColorProgram<"fill" | "label" | "pane"> = await c.compile({ fill, label, pane });
  const frame = program.resolve({ canvas: "#e6ddd0" });
  if (frame.state === "ready") {
    const rgb: readonly [number, number, number] = frame.values.fill.rgb;
    const completed: "exact" | "unfinished" = frame.optimality;
    const quality: ColorCleanlinessAssessment = frame.cleanliness;
    void quality;
    void rgb; void completed;
    // @ts-expect-error only declared aliases exist on a ready headless result.
    frame.values.missing;
  } else {
    frame.diagnostics;
    // @ts-expect-error unavailable results have no current values.
    frame.values;
  }
  const attachmentOptions: ColorAttachOptions = { nonce: "application-supplied-nonce" };
  const view: ColorView<"fill" | "label" | "pane"> = program.attach(element, { canvas: "#283548" }, attachmentOptions);
  // @ts-expect-error a CSP nonce is an explicit string, not a coerced value.
  program.attach(element, { canvas: "#283548" }, { nonce: 123 });
  view.set({ canvas: "#61766a" });
  const next = await c.compile({ primary: fill });
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
