// Байт-идентичность ПОЛНОЙ JS-проекции `resolveTheme`.
//
// `resolve-projection.golden.json` снят на ДО-оптимизационной проекции
// (по-полевое построение объекта через Reflect.set) генератором
// `gen-resolve-projection-golden.mjs`. Существующий
// `wasm-boundary-parity.test.mjs` фиксирует только `vars`; этот тест — лок
// всего результата: каждое поле каждой роли (kind/hex/lc/wcagRatio/флаги/css…),
// `vars`, ПОРЯДОК ключей на всех уровнях (через строковое равенство
// JSON.stringify) и битовые паттерны каждого f64 (FNV-1a по Float64-байтам —
// ловит и -0, которую stringify печатает как "0").
//
// Назначение: перф-оптимизация проекции (задача #54) может менять КАК строится
// объект, но не ЧТО в нём — этот тест делает инвариант проверяемым.
//
// Требует собранный `pkg/` (CI: `npm test` после `wasm-pack build`); без него
// скипается с внятным сообщением, как соседний wasm-boundary-parity.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = resolve(here, "../pkg/labcolors_bg.wasm");
const gluePath = resolve(here, "../pkg/labcolors.js");
const goldenPath = resolve(here, "resolve-projection.golden.json");

const haveWasm = existsSync(wasmPath) && existsSync(gluePath);

/** FNV-1a по little-endian байтам Float64 каждого числового листа — та же
 *  функция, что в генераторе (продублирована сознательно: тест не должен
 *  импортировать модуль с побочным эффектом записи golden). */
function f64fp(value) {
  let h = 0x811c9dc5;
  const buf = new DataView(new ArrayBuffer(8));
  const mix = (b) => {
    h ^= b;
    h = Math.imul(h, 0x01000193) >>> 0;
  };
  const walk = (v) => {
    if (typeof v === "number") {
      buf.setFloat64(0, v, true);
      for (let i = 0; i < 8; i++) mix(buf.getUint8(i));
    } else if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === "object") for (const k of Object.keys(v)) walk(v[k]);
  };
  walk(value);
  return (h >>> 0).toString(16).padStart(8, "0");
}

test("полная проекция resolveTheme байт-идентична до-оптимизационному golden", async (t) => {
  if (!haveWasm) {
    t.skip("pkg/ not built — run `npm run build` first (CI builds before `npm test`)");
    return;
  }
  const { initSync, LabColors } = await import(pathToFileURL(gluePath).href);
  initSync({ module: readFileSync(wasmPath) });
  const CONFIG = readFileSync(
    resolve(here, "../../../crates/labcolors-wasm/tests/data/labui.config.json"),
    "utf8",
  );
  const golden = JSON.parse(readFileSync(goldenPath, "utf8"));

  const engine = new LabColors();
  engine.loadConfig(CONFIG);

  for (const c of golden.cases) {
    // Дважды: второй вызов — гарантированный cache-hit того же ключа. Проекция
    // обязана быть идентичной на обоих путях (и hit, и первый резолв).
    for (const pass of ["first", "cache-hit"]) {
      const got = engine.resolveTheme(c.bg, c.theme);
      assert.equal(
        JSON.stringify(got),
        c.json,
        `${c.theme} ${c.bg} (${pass}): значения/порядок ключей разошлись с golden`,
      );
      assert.equal(
        f64fp(got),
        c.f64fp,
        `${c.theme} ${c.bg} (${pass}): битовые паттерны f64 разошлись с golden`,
      );
      // Свежий граф на каждый вызов — часть контракта (потребитель вправе
      // мутировать результат); мемоизация не имеет права вернуть тот же объект.
      const again = engine.resolveTheme(c.bg, c.theme);
      assert.notEqual(again, got, `${c.theme} ${c.bg}: повторный вызов вернул тот же объект`);
      assert.notEqual(again.roles, got.roles, `${c.theme} ${c.bg}: roles разделяют объект`);
      assert.notEqual(again.vars, got.vars, `${c.theme} ${c.bg}: vars разделяют объект`);
    }
  }
});
