// F-03 — atomic publication.
//
// ИНВАРИАНТ, КОТОРЫЙ ЗДЕСЬ ПРОВЕРЯЕТСЯ: если публикация снимка отказала
// посередине, предыдущий ЖИВОЙ снимок обязан остаться ПОБИТОВО неизменным.
// Доказательство — сравнение полного текста `style.cssText` (и полного набора
// пар свойство/значение) до и после отказавшей операции.
//
// Тесты, помеченные `{ todo: ... }`, — RED-воспроизведение находки на текущем
// origin/main. Они запускаются и падают; `todo` не даёт им сломать суммарный
// прогон, пока находка не закрыта.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { initSync } from "../pkg/labcolors.js";
import { applyTheme } from "../apply-theme.js";
import { watchTheme } from "../watch-theme.js";
import { adaptTheme } from "../adapt-theme.js";

initSync({
  module: new WebAssembly.Module(readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url))),
});

// Элемент с настоящей сериализацией `cssText`: порядок вставки + `name: value;`,
// как у CSSStyleDeclaration для custom properties. `armThrowOnWrite` подменяет
// host-CSSOM, который бросает на N-м обращении (remove и set считаются вместе,
// как их видит хост).
function cssomElement(initial = []) {
  const props = new Map(initial);
  const mutations = [];
  let writeCounter = 0;
  let throwOnWrite = 0;
  const guard = (kind, name) => {
    writeCounter++;
    mutations.push([kind, name, writeCounter]);
    if (throwOnWrite !== 0 && writeCounter === throwOnWrite) {
      throw new Error(`cssom ${kind} failed on write #${writeCounter}`);
    }
  };
  const style = {
    get length() {
      return props.size;
    },
    get cssText() {
      return [...props].map(([name, value]) => `${name}: ${value};`).join(" ");
    },
    item: (index) => [...props.keys()][index] ?? null,
    getPropertyValue: (name) => props.get(name) ?? "",
    setProperty(name, value) {
      guard("setProperty", name);
      props.set(name, value);
    },
    removeProperty(name) {
      guard("removeProperty", name);
      props.delete(name);
    },
  };
  return {
    props,
    style,
    mutations,
    armThrowOnWrite(n) {
      throwOnWrite = n;
      writeCounter = 0;
    },
    disarm() {
      throwOnWrite = 0;
      writeCounter = 0;
    },
    resetLog() {
      mutations.length = 0;
    },
    pairs: () => [...props],
  };
}

const snapshotOf = (element) => ({
  cssText: element.style.cssText,
  pairs: element.pairs(),
});

const roleSet = (vars) => ({
  vars,
  roles: Object.fromEntries(
    Object.entries(vars).map(([cssVar, hex]) => [
      cssVar.replace("--lab-", ""),
      { kind: "color", cssVar, hex, lc: 100 },
    ]),
  ),
});

const OLD = {
  "--lab-a": "#111111",
  "--lab-b": "#222222",
  "--lab-c": "#333333",
  "--lab-d": "#444444",
};
const NEW = {
  "--lab-a": "#AAAAAA",
  "--lab-b": "#BBBBBB",
  "--lab-c": "#CCCCCC",
  "--lab-d": "#DDDDDD",
};
const OLD_CSS = "--lab-a: #111111; --lab-b: #222222; --lab-c: #333333; --lab-d: #444444;";
const NEW_CSS = "--lab-a: #AAAAAA; --lab-b: #BBBBBB; --lab-c: #CCCCCC; --lab-d: #DDDDDD;";

// ───────────────────────── СЦЕНАРИЙ 1 — исключение на N-й записи ─────────────

test(
  "F-03/1 applyTheme: отказ CSSOM на 7-й записи обязан оставить прежний снимок побитово",
  { todo: "F-03: writeVars (snapshot.js:207-223) не снимает предыдущие пары и не восстанавливает их" },
  () => {
    const element = cssomElement();
    applyTheme(element, roleSet(OLD));
    const before = snapshotOf(element);
    assert.equal(before.cssText, OLD_CSS, "фикстура: первый снимок обязан лечь целиком");

    // Обращения 1..4 — removeProperty стухших пар, 5..8 — setProperty новых.
    // Бросаем на 7-м: два новых значения записаны, два ещё нет.
    element.armThrowOnWrite(7);
    assert.throws(
      () => applyTheme(element, roleSet(NEW)),
      /cssom setProperty failed on write #7/u,
      "фикстура: отказ CSSOM обязан выйти наружу",
    );
    element.disarm();

    const after = snapshotOf(element);
    assert.equal(
      after.cssText,
      before.cssText,
      "отказ публикации обязан оставить предыдущий живой снимок побитово неизменным",
    );
    assert.deepEqual(after.pairs, before.pairs);
  },
);

test(
  "F-03/1 applyTheme: отказ CSSOM не должен отдавать роль ambient CSS",
  {
    todo:
      "F-03: снятые, но не переписанные `--lab-*` исчезают из inline-стиля — ровно тот " +
      "неявный ambient-fallback, ради запрета которого admitSnapshot отклоняет Unreachable",
  },
  () => {
    const element = cssomElement();
    applyTheme(element, roleSet(OLD));
    element.armThrowOnWrite(7);
    assert.throws(() => applyTheme(element, roleSet(NEW)), /write #7/u);
    element.disarm();

    // Не «устарели», а ОТСУТСТВУЮТ: каскад отдаст этим переменным внешнее значение.
    assert.equal(element.props.has("--lab-c"), true, "--lab-c не должен исчезнуть из снимка");
    assert.equal(element.props.has("--lab-d"), true, "--lab-d не должен исчезнуть из снимка");
  },
);

test(
  "F-03/1 watchTheme: отказ CSSOM в refresh обязан оставить прежний снимок побитово",
  {
    todo:
      "F-03: тот же writeVars; контроллер помечает dirty и чинит на СЛЕДУЮЩЕЙ операции, " +
      "но физическое состояние между ними гибридное",
  },
  () => {
    let background = "#FFFFFF";
    const colors = { resolveTheme: (bg) => roleSet(bg === "#FFFFFF" ? OLD : NEW) };
    const element = cssomElement();
    const ctrl = watchTheme(element, {
      colors,
      theme: "light",
      background: () => background,
      observe: false,
    });
    const before = snapshotOf(element);
    assert.equal(before.cssText, OLD_CSS, "фикстура: стартовый commit обязан лечь целиком");

    background = "#000000";
    element.armThrowOnWrite(7);
    assert.throws(() => ctrl.refresh(), /cssom setProperty failed on write #7/u);
    element.disarm();

    assert.equal(
      element.style.cssText,
      before.cssText,
      "отказ публикации обязан оставить предыдущий живой снимок побитово неизменным",
    );
  },
);

test(
  "F-03/1 watchTheme: после отказа CSSOM и stop() гибрид остаётся навсегда",
  {
    todo:
      "F-03: обещанный self-heal («следующий refresh() повторит полный снимок») " +
      "недостижим после stop(); терминальный путь не чинит частичную запись",
  },
  () => {
    let background = "#FFFFFF";
    const colors = { resolveTheme: (bg) => roleSet(bg === "#FFFFFF" ? OLD : NEW) };
    const element = cssomElement();
    const ctrl = watchTheme(element, {
      colors,
      theme: "light",
      background: () => background,
      observe: false,
    });

    background = "#000000";
    element.armThrowOnWrite(7);
    assert.throws(() => ctrl.refresh(), /write #7/u);
    element.disarm();
    ctrl.stop(); // владелец уходит; больше никто не перепишет снимок

    assert.ok(
      element.style.cssText === OLD_CSS || element.style.cssText === NEW_CSS,
      `после stop() снимок обязан быть цельным, получено: '${element.style.cssText}'`,
    );
  },
);

// ───────────────── СЦЕНАРИЙ 1b — второй writer: diff-путь adaptTheme ─────────

// adapt-theme.js:771 пишет в DOM МИМО `writeVars`. Кадр ease — самостоятельная
// публикация: отказ посередине оставляет часть ролей на цвете кадра N, часть —
// на цвете кадра N-1, то есть гибридный ПОКРАШЕННЫЙ кадр.
const ADAPT_VARS = ["--lab-p", "--lab-q", "--lab-r", "--lab-s"];

const adaptResult = (hex) => ({
  vars: Object.fromEntries(ADAPT_VARS.map((cssVar) => [cssVar, hex])),
  roles: Object.fromEntries(
    ADAPT_VARS.map((cssVar) => [
      cssVar.replace("--lab-", ""),
      { kind: "color", cssVar, hex, lc: 100 },
    ]),
  ),
});

function adaptHarness(element) {
  let now = 1000;
  let resolved = adaptResult("#000000");
  let lc = ADAPT_VARS.map(() => 100);
  let bg = "#FFFFFF";
  const colors = {
    resolveCount: 0,
    resolveTheme() {
      colors.resolveCount++;
      return resolved;
    },
    recheckContrast() {
      return lc.flatMap((value) => [value, 10]);
    },
  };
  const ctrl = adaptTheme(element, {
    colors,
    theme: "light",
    background: () => bg,
    now: () => now,
    win: {},
    sustainMs: 120,
    dwellMs: 250,
    easeMs: 1000,
  });
  return {
    ctrl,
    colors,
    setNow: (value) => {
      now = value;
    },
    setBg: (value) => {
      bg = value;
    },
    setLc: (values) => {
      lc = values;
    },
    setResolve: (value) => {
      resolved = value;
    },
  };
}

test(
  "F-03/1b adaptTheme diff-кадр: отказ на N-й записи обязан оставить предыдущий кадр побитово",
  { todo: "F-03: второй writer (adapt-theme.js:771) пишет посвойственно, без capture/restore" },
  () => {
    const element = cssomElement();
    const h = adaptHarness(element);
    assert.equal(element.pairs().length, 4, "фикстура: стартовый снимок обязан лечь целиком");

    // Устойчивый провал контраста → пересчёт → ease в полёте.
    h.setLc(ADAPT_VARS.map(() => 10));
    h.setBg("#202020");
    h.ctrl.tick(); // взводит breachSince
    h.setResolve(adaptResult("#F0F0F0"));
    h.setNow(1300); // за sustainMs и за dwellMs
    h.setBg("#202021");
    h.ctrl.tick(); // пересчёт + adopt: diff-база обнулена, ease пошёл
    assert.equal(h.colors.resolveCount, 2, "фикстура: устойчивый провал обязан вызвать пересчёт");
    h.setLc(ADAPT_VARS.map(() => 100)); // провал снят: дальше только кадры ease

    h.setNow(1400);
    h.setBg("#202022");
    h.ctrl.tick(); // первый кадр после adopt: полная запись, diff-база встаёт
    const before = snapshotOf(element);
    assert.equal(before.pairs.length, 4, "фикстура: кадр ease обязан покрывать все роли");
    const framePaint = new Set(before.pairs.map(([, value]) => value));
    assert.equal(framePaint.size, 1, "фикстура: до отказа все роли на одном цвете кадра");

    h.setNow(1500);
    h.setBg("#202023");
    element.resetLog();
    element.armThrowOnWrite(2); // второй setProperty diff-кадра
    assert.throws(() => h.ctrl.tick(), /cssom setProperty failed on write #2/u);
    element.disarm();
    assert.deepEqual(
      element.mutations.map(([kind]) => kind),
      ["setProperty", "setProperty"],
      "фикстура: кадр обязан идти DIFF-путём (adapt-theme.js:771), без remove-фазы",
    );

    assert.equal(
      element.style.cssText,
      before.cssText,
      "отказ кадра обязан оставить предыдущий покрашенный кадр побитово неизменным",
    );
  },
);

// ───────── СЦЕНАРИИ 2-4 — reentrancy / cancel / dispose посреди записи ───────
// Здесь ожидается ЗЕЛЁНОЕ: сериализация через commitDepth не даёт новой
// операции разорвать уже начатую публикацию.

test("F-03/2 reentrant setTheme из обработчика записи не разрывает публикацию", () => {
  const element = cssomElement();
  let background = "#FFFFFF";
  const colors = { resolveTheme: (bg) => roleSet(bg === "#FFFFFF" ? OLD : NEW) };
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: () => background,
    observe: false,
  });

  let reentered = false;
  const rawSet = element.style.setProperty.bind(element.style);
  element.style.setProperty = (name, value) => {
    rawSet(name, value);
    if (!reentered && name === "--lab-b") {
      reentered = true;
      ctrl.setTheme("light"); // reentrant посреди commit
    }
  };

  background = "#000000";
  ctrl.refresh();
  assert.equal(reentered, true, "фикстура: reentrant-вызов обязан состояться во время записи");

  const pairs = element.pairs();
  assert.equal(pairs.length, 4, "снимок обязан покрывать весь набор ключей");
  const allOld = pairs.every(([name, value]) => value === OLD[name]);
  const allNew = pairs.every(([name, value]) => value === NEW[name]);
  assert.ok(allOld || allNew, `гибридный снимок: ${element.style.cssText}`);
});

test("F-03/3+4 stop() из обработчика записи не обрывает уже начатую публикацию", () => {
  const element = cssomElement();
  let background = "#FFFFFF";
  const colors = { resolveTheme: (bg) => roleSet(bg === "#FFFFFF" ? OLD : NEW) };
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: () => background,
    observe: false,
  });

  let stopped = false;
  const rawSet = element.style.setProperty.bind(element.style);
  element.style.setProperty = (name, value) => {
    rawSet(name, value);
    if (!stopped && name === "--lab-b") {
      stopped = true;
      ctrl.stop(); // dispose посреди commit
    }
  };

  background = "#000000";
  ctrl.refresh();
  assert.equal(stopped, true, "фикстура: stop() обязан быть вызван во время записи");
  assert.equal(
    element.style.cssText,
    NEW_CSS,
    "dispose посреди commit не должен оставлять усечённый снимок",
  );
});

// ───────── СЦЕНАРИЙ 5 — поздний результат после отмены ───────────────────────

test("F-03/5 поздний refresh после stop() не пишет в DOM", async () => {
  const element = cssomElement();
  let background = "#FFFFFF";
  const colors = { resolveTheme: (bg) => roleSet(bg === "#FFFFFF" ? OLD : NEW) };
  let observerCallback = null;
  const win = {
    MutationObserver: function (fn) {
      observerCallback = fn;
      return { observe() {}, disconnect() {} };
    },
    document: { documentElement: {} },
  };
  const ctrl = watchTheme(element, {
    colors,
    theme: "light",
    background: () => background,
    win,
  });
  const before = snapshotOf(element);
  assert.equal(before.cssText, OLD_CSS, "фикстура: стартовый commit обязан лечь целиком");

  background = "#000000";
  observerCallback(); // ставит refresh в микрозадачу
  ctrl.stop(); // отменяем ДО того, как микрозадача выполнится
  await Promise.resolve();
  await Promise.resolve();

  assert.equal(element.style.cssText, before.cssText, "поздний результат не должен писать в DOM");
});

// ───────── ЧУВСТВИТЕЛЬНОСТЬ ОРАКУЛА (anti-vacuity) ──────────────────────────
//
// Оракул обязан быть выполнимым: тот же фикстурный CSSOM, тот же отказ на 7-й
// записи, но writer сначала снимает предыдущие пары и восстанавливает их при
// исключении. Если этот тест зелёный, а F-03/1 красный, то причина падения
// F-03/1 — именно отсутствие capture/restore в `writeVars`, а не бросающая
// фикстура и не соседний инвариант.

// Точная копия последовательности writeVars (snapshot.js:207-223) + снятие
// предыдущего состояния и его восстановление при отказе.
function writeVarsWithCapture(element, vars) {
  const entries = Object.entries(vars);
  const stale = [];
  for (let i = 0; i < element.style.length; i++) {
    const name = element.style.item(i);
    if (typeof name === "string" && name.startsWith("--lab-")) stale.push(name);
  }
  const backup = stale.map((name) => [name, element.style.getPropertyValue(name)]);
  try {
    for (const name of stale) element.style.removeProperty(name);
    for (const [name, value] of entries) element.style.setProperty(name, value);
  } catch (error) {
    for (const [name] of entries) element.style.removeProperty(name);
    for (const [name, value] of backup) element.style.setProperty(name, value);
    throw error;
  }
}

test("F-03/oracle: writer со снятием и восстановлением удовлетворяет тот же оракул", () => {
  const element = cssomElement();
  writeVarsWithCapture(element, OLD);
  const before = snapshotOf(element);
  assert.equal(before.cssText, OLD_CSS);

  element.armThrowOnWrite(7); // тот же отказ, что валит F-03/1
  assert.throws(() => writeVarsWithCapture(element, NEW), /cssom setProperty failed on write #7/u);
  element.disarm();

  const after = snapshotOf(element);
  assert.equal(after.cssText, before.cssText, "оракул выполним: снимок восстановим побитово");
  assert.deepEqual(after.pairs, before.pairs);
  assert.equal(element.props.has("--lab-c"), true);
  assert.equal(element.props.has("--lab-d"), true);
});

test("F-03/oracle: без восстановления тот же writer даёт ровно наблюдаемый гибрид", () => {
  // Контрольная группа: убираем ТОЛЬКО capture/restore. Результат обязан
  // совпасть с фактическим поведением `writeVars` — значит различие ровно одно.
  const element = cssomElement();
  writeVarsWithCapture(element, OLD);
  element.armThrowOnWrite(7);
  assert.throws(() => {
    const entries = Object.entries(NEW);
    const stale = [];
    for (let i = 0; i < element.style.length; i++) {
      const name = element.style.item(i);
      if (typeof name === "string" && name.startsWith("--lab-")) stale.push(name);
    }
    for (const name of stale) element.style.removeProperty(name);
    for (const [name, value] of entries) element.style.setProperty(name, value);
  }, /write #7/u);
  element.disarm();

  assert.equal(
    element.style.cssText,
    "--lab-a: #AAAAAA; --lab-b: #BBBBBB;",
    "контрольная группа обязана воспроизвести наблюдаемый у writeVars гибрид",
  );
});

// ───────── СЦЕНАРИЙ 6 — цель откреплена от документа ─────────────────────────

test("F-03/6 запись в откреплённую цель не создаёт нового класса отказа", () => {
  // Инлайн-стиль откреплённого элемента остаётся записываемым; пакет нигде не
  // читает `isConnected`/`ownerDocument`, поэтому открепление само по себе не
  // порождает отказ. Если host всё же бросит — это ровно сценарий 1.
  const detached = cssomElement();
  applyTheme(detached, roleSet(OLD));
  assert.equal(detached.style.cssText, OLD_CSS);
});
