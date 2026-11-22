import { declareInterface } from "./declaration.js";

const byId = (id) => document.getElementById(id);
const preview = byId("preview"), status = byId("status"), diagnostics = byId("diagnostics");
const controls = { surface: byId("surface"), blue: byId("blue"), warm: byId("warm"), opacity: byId("opacity") };
let program, view, authoredRevision = 0, pendingFrame = 0;


function settings() {
  return { blue: controls.blue.value, warm: controls.warm.value, opacity: Number(controls.opacity.value) / 100 };
}

function setStatus(message, state) {
  status.textContent = message;
  status.parentElement.dataset.state = state;
}

function show(outcome) {
  if (outcome.state === "rejected") {
    const frame = outcome.proposed;
    const retained = view?.current ? "Сохранён предыдущий интерфейс." : "Новый интерфейс пока не опубликован.";
    const messages = {
      conflict: `Изменение не применено: связанные условия несовместимы. ${retained}`,
      unresolved: `Поиск ещё не нашёл допустимый вариант. ${retained}`,
      unknown: `Для изменения не хватает данных поверхности. ${retained}`,
      stale: `Контекст успел измениться. ${retained}`,
    };
    setStatus(messages[frame.state], "rejected");
    diagnostics.hidden = false;
    diagnostics.textContent = JSON.stringify(frame, (key, value) => typeof value === "bigint" ? String(value) : key === "use" ? undefined : value, 2);
    return;
  }
  const { frame } = outcome;
  preview.setAttribute("aria-busy", "false");
  setStatus("Цвета и поверхность приняты вместе.", "ready");
  byId("revision").textContent = String(frame.revision);
  byId("completion").textContent = frame.optimality === "exact"
    ? "Обязательные условия выполнены; поиск предпочтительного результата завершён."
    : "Обязательные условия выполнены. Поиск ещё более предпочтительного результата не завершён.";
  const rows = Object.entries(frame.values).map(([name, value]) => {
    const row = document.createElement("tr");
    for (const text of [name, value.rgb.join(", "), String(value.opacity)]) {
      const cell = document.createElement("td");
      cell.textContent = text;
      row.append(cell);
    }
    return row;
  });
  byId("outputs").replaceChildren(...rows);
  diagnostics.hidden = true;
  diagnostics.textContent = "";
  for (const button of document.querySelectorAll("[data-surface]")) {
    button.setAttribute("aria-pressed", String(button.dataset.surface === controls.surface.value));
  }
}

function report(error) {
  setStatus("Не удалось применить изменение. Подробности доступны в текущем результате.", "rejected");
  diagnostics.hidden = false;
  diagnostics.textContent = `${error.code ?? error.name}: ${error.message}`;
}

function applySurface() {
  pendingFrame = 0;
  if (!view) return;
  try { show(view.set({ canvas: controls.surface.value })); }
  catch (error) { report(error); }
}

function scheduleSurface() {
  if (!pendingFrame) pendingFrame = requestAnimationFrame(applySurface);
}

async function applyAuthoredRevision() {
  const generation = ++authoredRevision;
  byId("opacity-value").value = `${controls.opacity.value}%`;
  const next = await declareInterface(settings());
  if (generation !== authoredRevision) { next.dispose(); return; }
  try {
    if (!view) {
      view = next.attach(preview);
      const outcome = view.set({ canvas: controls.surface.value });
      program = next;
      show(outcome);
      return;
    }
    const outcome = view.replace(next, { canvas: controls.surface.value });
    if (outcome.state === "committed") {
      view = outcome.view;
      program.dispose();
      program = next;
    } else next.dispose();
    show(outcome);
  } catch (error) { next.dispose(); throw error; }
}

for (const button of document.querySelectorAll("[data-surface]")) {
  button.addEventListener("click", () => { controls.surface.value = button.dataset.surface; scheduleSurface(); });
}
document.querySelector("form").addEventListener("submit", (event) => event.preventDefault());
controls.surface.addEventListener("input", scheduleSurface);
for (const control of [controls.blue, controls.warm, controls.opacity]) {
  control.addEventListener("input", () => applyAuthoredRevision().catch(report));
}
byId("open-project").addEventListener("click", (event) => {
  const expanded = event.currentTarget.getAttribute("aria-expanded") === "true";
  event.currentTarget.setAttribute("aria-expanded", String(!expanded));
  byId("project-detail").hidden = expanded;
});
window.addEventListener("pagehide", () => {
  authoredRevision += 1;
  cancelAnimationFrame(pendingFrame);
  view?.dispose();
  program?.dispose();
});
await applyAuthoredRevision().catch(report);
