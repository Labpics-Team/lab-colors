import { declareInterface } from "./declaration.js";
import { createColorInteraction } from "./interaction.js";

const byId = (id) => document.getElementById(id);
const preview = byId("preview"), status = byId("status"), diagnostics = byId("diagnostics");
const controls = { surface: byId("surface"), blue: byId("blue"), warm: byId("warm"), opacity: byId("opacity") };
function setStatus(message, state) {
  status.textContent = message;
  status.parentElement.dataset.state = state;
}

function show(outcome) {
  if (outcome.state === "rejected") {
    const frame = outcome.proposed;
    const retained = interaction.current ? "Сохранён предыдущий интерфейс." : "Новый интерфейс пока не опубликован.";
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
  byId("quality").textContent = frame.cleanliness.status === "not-assessed"
    ? "Cleanliness композиции ещё не оценена. Этот пример проверяет сохранение характера, связанные цвета и материал."
    : "Проверена выбранная номинальная конвенция для конечных точек. Общая Cleanliness композиции остаётся отдельной задачей.";
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

const interaction = createColorInteraction({
  compile: declareInterface,
  attach: (program) => program.attach(preview),
  onOutcome: show,
  onError: report,
  requestFrame: (callback) => requestAnimationFrame(callback),
  cancelFrame: (frame) => cancelAnimationFrame(frame),
});

function scheduleControls() {
  byId("opacity-value").value = `${controls.opacity.value}%`;
  interaction.schedule({ canvas: controls.surface.value, blue: controls.blue.value,
    warm: controls.warm.value, opacity: Number(controls.opacity.value) / 100 });
}

for (const button of document.querySelectorAll("[data-surface]")) {
  button.addEventListener("click", () => { controls.surface.value = button.dataset.surface; scheduleControls(); });
}
document.querySelector("form").addEventListener("submit", (event) => event.preventDefault());
for (const control of Object.values(controls)) control.addEventListener("input", scheduleControls);
byId("open-project").addEventListener("click", (event) => {
  const expanded = event.currentTarget.getAttribute("aria-expanded") === "true";
  event.currentTarget.setAttribute("aria-expanded", String(!expanded));
  byId("project-detail").hidden = expanded;
});
window.addEventListener("pagehide", (event) => {
  // A cached page resumes with the same DOM and its existing color owner.
  if (!event.persisted) interaction.dispose();
});
scheduleControls();
