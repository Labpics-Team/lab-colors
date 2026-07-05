// Сквозные инварианты ВЕРХА цепочки, как их видит БРАУЗЕР: живой движок
// (resolveTheme) → эмитированная oklch-строка в `vars` → ШТАТНЫЙ парсер
// потребителя (parseCssColor) → перепроверка легальности (recheckContrast).
//
// Почему здесь, а не в Rust: `vars[--lab-*]` — это ровно та строка, которую
// прочитает браузер, а `parseCssColor` / `compositeOver` / `toHex` — тот самый
// код пакета, что реконструирует цвет на странице (его же использует
// effectiveBackground). Так тест меряет ПОТЕРИ НА СЕРИАЛИЗАЦИИ ВЫХОДА, а не
// внутри солвера, и без параллельной копии физики контраста.
//
// Что уже закрыто в другом месте (НЕ дублируем):
// - core `oklch.rs::round_trip_is_byte_exact_*` — emit↔parse байт-точны на КУБЕ;
// - `oklch-parse.test.mjs` — parseCssColor декодит эмиссию на 16 фикстурах;
// - core `property_invariants.rs::every_floored_role_clears_its_wcag_floor…` —
//   пол на СОБСТВЕННОМ hex солвера (не на репарснутой строке);
// - wasm `wasm_parity.rs` — граница == нативный резолв, роль-в-роль.
// Дыра: КОМПОЗИЦИЯ этих доказательств на ЖИВОМ корпусе ролей — «цвет, который
// браузер соберёт из emitted vars, всё ещё проходит свой пол/таргет» — как
// единая цепочка, а не транзитивность двух изолированных проверок. Для
// полупрозрачных ролей это единственное место, где проверяется, что
// эмитированные тинт+альфа, скомпозиченные браузером, дают ОБЕЩАННЫЙ композит.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { initSync, LabColors } from "../pkg/labcolors.js";
import { parseCssColor, compositeOver, toHex } from "../effective-bg.js";

// Инициализация wasm в node: pkg собран под `--target web` (fetch по URL), а в
// node грузим байты напрямую. Оборачиваем в WebAssembly.Module и передаём
// объектом `{ module }` — штатная форма initSync без deprecated-варнинга.
initSync({
  module: new WebAssembly.Module(readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url))),
});

/// Максимальная поканальная дельта двух `#RRGGBB` в LSB (8-бит ступенях).
function channelDelta(hexA, hexB) {
  const a = parseCssColor(hexA);
  const b = parseCssColor(hexB);
  return Math.max(Math.abs(a[0] - b[0]), Math.abs(a[1] - b[1]), Math.abs(a[2] - b[2]));
}

// Замороженный SSOT-паспорт labui — тот же, что читает wasm-parity. Единый вход
// и для движка, и для декларации алиасов ниже, чтобы стороны не разошлись.
const PASSPORT_PATH = new URL(
  "../../../crates/labcolors-wasm/tests/data/labui.config.json",
  import.meta.url,
);
const PASSPORT = readFileSync(PASSPORT_PATH, "utf8");
const PASSPORT_OBJ = JSON.parse(PASSPORT);

const THEMES = ["light", "dark", "light-ic", "dark-ic"];
// Корпус фонов: концы яркостной оси, near-края, канонические подложки labui.
const BACKGROUNDS = ["#FFFFFF", "#000000", "#101012", "#808080", "#F2F2F7", "#1C1C1E"];

function engine() {
  const e = new LabColors();
  e.loadConfig(PASSPORT);
  return e;
}

// [r,g,b] из parseCssColor-результата (отбрасываем α).
const rgb = (parsed) => [parsed[0], parsed[1], parsed[2]];

// ─────────────────────────────────────────────────────────────────────────────
// ЛЕГАЛЬНОСТЬ НАСКВОЗЬ — сплошные роли
// ─────────────────────────────────────────────────────────────────────────────

test("legality survives serialization: each solid role's emitted var reparses to its exact colour and still clears its floor", () => {
  const e = engine();
  let flooredChecked = 0;
  let solidsChecked = 0;

  for (const theme of THEMES) {
    for (const bg of BACKGROUNDS) {
      const res = e.resolveTheme(bg, theme);
      for (const [key, role] of Object.entries(res.roles)) {
        if (role.kind !== "color") continue;
        solidsChecked++;

        const emitted = res.vars[role.cssVar];
        assert.ok(emitted, `${theme}/${bg}/${key}: reachable colour must emit a var`);

        // Браузер реконструирует цвет из emitted строки штатным парсером.
        const parsed = parseCssColor(emitted);
        assert.ok(parsed, `${theme}/${bg}/${key}: emitted var must parse: ${emitted}`);
        const paintedHex = toHex(rgb(parsed));

        // Серия­лизация без потерь: реконструкция === то, что движок отдал как hex.
        assert.equal(
          paintedHex,
          role.hex,
          `${theme}/${bg}/${key}: reparsed var must equal role hex (serialization drift)`,
        );

        // Контраст РЕПАРСНУТОГО цвета, замеренный тем же движком, воспроизводит
        // обещанный — и, для floored-ролей, всё ещё держит пол.
        const flat = e.recheckContrast(bg, [paintedHex], theme);
        const [lc, wcag] = [flat[0], flat[1]];
        assert.ok(
          Math.abs(wcag - role.wcagRatio) < 1e-9,
          `${theme}/${bg}/${key}: recheck WCAG ${wcag} != reported ${role.wcagRatio}`,
        );
        assert.ok(
          Math.abs(lc - role.lc) < 1e-9,
          `${theme}/${bg}/${key}: recheck Lc ${lc} != reported ${role.lc}`,
        );
        if (role.legalFloor != null) {
          flooredChecked++;
          assert.ok(
            wcag >= role.legalFloor - 1e-9,
            `${theme}/${bg}/${key}: reparsed colour fell below legal floor ${role.legalFloor} (got ${wcag})`,
          );
        }
      }
    }
  }

  // Не вакуумно: свип обязан реально прогнать сплошные и floored-роли.
  assert.ok(solidsChecked > 0, "no solid roles exercised — sweep is vacuous");
  assert.ok(flooredChecked > 0, "no floored solid roles exercised — floor claim is vacuous");
});

// ─────────────────────────────────────────────────────────────────────────────
// ЛЕГАЛЬНОСТЬ НАСКВОЗЬ — полупрозрачные роли (композит, как его соберёт браузер)
// ─────────────────────────────────────────────────────────────────────────────

// НАХОДКА (репро ниже, зафиксирована в PR; причина заземлена на исходники
// labcolors-core): для 3 из 1699 полупрозрачных сэмплов корпуса (все — тинт
// #C0B2FA при низкой α над чистым чёрным в dark-ic) композит, СОБРАННЫЙ
// браузером из эмитированной строки, расходится с движковым `compositeHex`
// ровно на 1 LSB (#17161F vs #17161E и т.п.).
// ПРИЧИНА — РАЗНОЕ ПРОСТРАНСТВО АРИФМЕТИКИ, не потеря точности тинта: тинт
// КВАНТУЕТСЯ в 8 бит В ОБОИХ путях (semantic.rs:1977 `quantise_encoded` ДО
// `composite_over_encoded`), α одна. Но движок композитит в нормализованном
// encoded-[0,1] (`α·(byte/255)`), затем hex_from_srgb_encoded делает `·255·round`
// (srgb.rs:196); браузер (`compositeOver`) — в 0–255 (`α·byte`). На α·byte ровно
// = 30.5 (тинт-канал 250, α=0.122): движок `0.122·(250/255)·255 = 30.4999… →
// round → 30` (#…1E), браузер `250·0.122 = 30.5 → round → 31` (#…1F). Округление
// точной половины расходится, потому что ÷255·255 стягивает 30.5 к 30.4999.
// Следствие: обещанный `compositeHex`/Lc/WCAG считаны в другом пространстве,
// чем то, что реально отрендерит браузер (суб-JND, но реальный выходной gap).
// Живёт в эмиссии labcolors-core — не чиним здесь (зона солвера), фиксируем
// границей. Возможный фикс ядра: композитить в том же (байтовом) пространстве,
// что и браузер, — тогда обещанное == отрендеренное. Инвариант ниже строгий на
// том, что ЭМИССИЯ СТРОКИ (тинт/α) ничего не теряет побайтно, и держит композит
// в истинной границе ≤1 LSB — дрейф ≥2 LSB или потеря тинта/α упадёт RED.
const COMPOSITE_QUANT_LSB = 1;
// Известная верхняя граница числа расхождений на текущем корпусе (THEMES×
// BACKGROUNDS×паспорт). Пин: разрастание gap (в другие темы/фоны/роли) поднимет
// число выше и упадёт RED — характеризация защищает и КОЛИЧЕСТВО, и ЛОКАЦИЮ.
const KNOWN_COMPOSITE_DIVERGENCES = 3;

test("translucent serialization fidelity: emitted tint+alpha round-trip exactly; browser composite matches the promise within the ≤1-LSB quantization bound", () => {
  const e = engine();
  let translucentChecked = 0;

  for (const theme of THEMES) {
    for (const bg of BACKGROUNDS) {
      const res = e.resolveTheme(bg, theme);
      const bgParsed = parseCssColor(bg);
      assert.ok(bgParsed, `${theme}/${bg}: background must parse`);

      for (const [key, role] of Object.entries(res.roles)) {
        if (role.kind !== "translucent") continue;
        translucentChecked++;

        const emitted = res.vars[role.cssVar];
        const parsed = parseCssColor(emitted); // [tr,tg,tb, alpha]
        assert.ok(parsed, `${theme}/${bg}/${key}: translucent var must parse: ${emitted}`);
        const [, , , alpha] = parsed;
        assert.ok(
          Number.isFinite(alpha) && alpha > 0 && alpha <= 1,
          `${theme}/${bg}/${key}: emitted alpha out of (0,1]: ${alpha}`,
        );

        // HARD: тинт эмиссии реконструируется в отданный движком tintHex
        // побайтово — эмитированная СТРОКА не теряет тинт.
        assert.equal(
          toHex(rgb(parsed)),
          role.tintHex,
          `${theme}/${bg}/${key}: reparsed tint != reported tintHex (serialization loss)`,
        );
        // HARD: α реконструируется в пределах точности эмиссии (4 знака).
        assert.ok(
          Math.abs(alpha - role.alpha) < 5e-5,
          `${theme}/${bg}/${key}: reparsed alpha ${alpha} drifted from ${role.alpha}`,
        );

        // Композит, собранный браузером из ЭМИТИРОВАННОГО тинта, — в истинной
        // границе ≤1 LSB от обещанного (см. находку выше). Дрейф ≥2 = регрессия.
        const compHex = toHex(compositeOver(parsed, [bgParsed[0], bgParsed[1], bgParsed[2], 1]));
        assert.ok(
          channelDelta(compHex, role.compositeHex) <= COMPOSITE_QUANT_LSB,
          `${theme}/${bg}/${key}: composite ${compHex} drifts >${COMPOSITE_QUANT_LSB} LSB from promised ${role.compositeHex}`,
        );

        // Самосогласованность движка: перепроверка ЕГО ЖЕ compositeHex
        // воспроизводит отданные метрики композита (не зависит от находки выше).
        const flat = e.recheckContrast(bg, [role.compositeHex], theme);
        assert.ok(
          Math.abs(flat[1] - role.compositeWcag) < 1e-9,
          `${theme}/${bg}/${key}: recheck(compositeHex) WCAG ${flat[1]} != reported ${role.compositeWcag}`,
        );
        assert.ok(
          Math.abs(flat[0] - role.compositeLc) < 1e-9,
          `${theme}/${bg}/${key}: recheck(compositeHex) Lc ${flat[0]} != reported ${role.compositeLc}`,
        );
      }
    }
  }
  assert.ok(translucentChecked > 0, "no translucent roles exercised — sweep is vacuous");
});

// Характеризация НАХОДКИ: пин ТЕКУЩЕГО поведения по трём осям — ВЕЛИЧИНА (≤1 LSB),
// КОЛИЧЕСТВО (≤ известного) и ЛОКАЦИЯ (только near-black dark-ic). Тест НЕ
// узаконивает баг:
//   • починят движок (композит в байтовом пространстве) → расхождения исчезнут,
//     `divergent.length > 0` упадёт → форс-ревью;
//   • разрастётся gap (другие темы/фоны/роли или >1 LSB) → величина/количество/
//     локация превысят пин → RED.
// Без пина количества и локации регрессия, размазавшая тот же ≤1-LSB зазор на
// сотню сэмплов или в light-темы, прошла бы зелёной — находка тихо сгнила бы.
test("characterization: the composite quantization gap is real, bounded to ≤1 LSB, count-capped, and confined to near-black dark-ic (pins current behaviour)", () => {
  const e = engine();
  let maxObservedDelta = 0;
  const divergent = [];
  for (const theme of THEMES) {
    for (const bg of BACKGROUNDS) {
      const res = e.resolveTheme(bg, theme);
      const bgP = parseCssColor(bg);
      for (const [key, role] of Object.entries(res.roles)) {
        if (role.kind !== "translucent") continue;
        const parsed = parseCssColor(res.vars[role.cssVar]);
        const compHex = toHex(compositeOver(parsed, [bgP[0], bgP[1], bgP[2], 1]));
        const d = channelDelta(compHex, role.compositeHex);
        maxObservedDelta = Math.max(maxObservedDelta, d);
        if (d > 0) divergent.push(`${theme}/${bg}/${key}`);
      }
    }
  }
  // ВЕЛИЧИНА: никогда больше 1 LSB.
  assert.ok(maxObservedDelta <= 1, `composite quantization gap exceeded 1 LSB: ${maxObservedDelta}`);
  // СУЩЕСТВОВАНИЕ: расхождение реально сегодня (иначе движок, вероятно, починили).
  assert.ok(
    divergent.length > 0,
    "composite gap no longer reproduces — engine may compose in byte space now; revisit the PR finding",
  );
  // КОЛИЧЕСТВО: не больше известной верхней границы — разрастание gap → RED.
  assert.ok(
    divergent.length <= KNOWN_COMPOSITE_DIVERGENCES,
    `composite gap spread: ${divergent.length} divergences > known ${KNOWN_COMPOSITE_DIVERGENCES} — ${divergent.join(", ")}`,
  );
  // ЛОКАЦИЯ: все расхождения — только near-black (#000000) в dark-ic. Утечка в
  // другую тему/фон = смена природы находки → форс-ревью.
  for (const loc of divergent) {
    assert.ok(
      loc.startsWith("dark-ic/#000000/"),
      `composite gap escaped near-black dark-ic: ${loc}`,
    );
  }
});

// ─────────────────────────────────────────────────────────────────────────────
// GLOW — сателлиты эмиссии
// ─────────────────────────────────────────────────────────────────────────────

test("glow roles emit halo primary + -core/-alpha satellites, all well-formed", () => {
  const e = engine();
  let glowChecked = 0;

  for (const theme of THEMES) {
    for (const bg of BACKGROUNDS) {
      const res = e.resolveTheme(bg, theme);
      for (const [key, role] of Object.entries(res.roles)) {
        if (role.kind !== "glow") continue;
        glowChecked++;

        // Основная переменная несёт halo (единая oklch-форма) == role.css.
        assert.equal(
          res.vars[role.cssVar],
          role.css,
          `${theme}/${bg}/${key}: primary var must mirror halo css`,
        );
        assert.ok(parseCssColor(res.vars[role.cssVar]), `${theme}/${bg}/${key}: halo must parse`);

        // Сателлит -core: валидная oklch-строка.
        const core = res.vars[`${role.cssVar}-core`];
        assert.ok(core && parseCssColor(core), `${theme}/${bg}/${key}: -core must be valid oklch`);

        // Сателлит -alpha: конечное число в (0,1].
        const alphaVar = res.vars[`${role.cssVar}-alpha`];
        const a = Number(alphaVar);
        assert.ok(
          Number.isFinite(a) && a > 0 && a <= 1,
          `${theme}/${bg}/${key}: -alpha must be a number in (0,1]: ${alphaVar}`,
        );
      }
    }
  }
  assert.ok(glowChecked > 0, "no glow roles exercised — passport should carry glows");
});

// ─────────────────────────────────────────────────────────────────────────────
// СЕМАНТИКА БЕЗ СИРОТ
// ─────────────────────────────────────────────────────────────────────────────

test("no orphan aliases: every declared alias resolves and mirrors its target's emission across all themes", () => {
  const e = engine();
  const declared = PASSPORT_OBJ.aliases ?? [];
  assert.ok(declared.length > 0, "passport must declare aliases for this test to bite");

  for (const theme of THEMES) {
    const res = e.resolveTheme("#FFFFFF", theme);
    for (const { alias, target } of declared) {
      // Алиас НЕ теряется молча: движок дропает алиас, чья цель не нашлась —
      // тогда ключа алиаса не будет в выводе. Его присутствие = цель существует.
      assert.ok(
        Object.prototype.hasOwnProperty.call(res.roles, alias),
        `${theme}: declared alias '${alias}' silently dropped (target '${target}' missing?)`,
      );
      assert.ok(
        Object.prototype.hasOwnProperty.call(res.roles, target),
        `${theme}: alias '${alias}' targets non-existent role '${target}'`,
      );
      // Алиас эмитит ту же переменную, что и цель (граница копирует исход цели).
      const aliasVar = res.vars[`--lab-${alias}`];
      const targetVar = res.vars[`--lab-${target}`];
      if (targetVar !== undefined) {
        assert.equal(
          aliasVar,
          targetVar,
          `${theme}: alias '${alias}' var must mirror target '${target}' var`,
        );
      }
    }
  }
});

test("no colliding vars keys: every emitted var name is unique (glow satellites never overwrite a role's primary)", () => {
  const e = engine();
  for (const theme of THEMES) {
    for (const bg of BACKGROUNDS) {
      const res = e.resolveTheme(bg, theme);
      // Ожидаемое число ключей vars: по одному primary на каждую эмитирующую
      // роль (color/translucent/glow) + 2 сателлита на каждый glow. Если
      // сателлит перезаписал чужой primary, фактическое число будет МЕНЬШЕ.
      let expected = 0;
      for (const role of Object.values(res.roles)) {
        if (role.kind === "color" || role.kind === "translucent" || role.kind === "glow") {
          expected += 1;
        }
        if (role.kind === "glow") expected += 2;
      }
      // Гард не-вакуумности: паспорт обязан эмитить роли, иначе 0==0 пройдёт
      // тривиально и коллизия осталась бы непроверенной.
      assert.ok(expected > 0, `${theme}/${bg}: no emitting roles — collision check is vacuous`);
      const actual = Object.keys(res.vars).length;
      assert.equal(
        actual,
        expected,
        `${theme}/${bg}: vars key count ${actual} != expected ${expected} — a var name collided (silent overwrite)`,
      );
    }
  }
});

test("reachable role-key set is identical across all four themes, and every reachable role appears in vars", () => {
  const e = engine();
  const bg = "#FFFFFF";

  // Один и тот же контракт-набор ключей во всех темах (тема меняет цвета, не роли).
  const keysets = THEMES.map((t) => Object.keys(e.resolveTheme(bg, t).roles).sort());
  const first = keysets[0];
  // Гард не-вакуумности: пустой набор во всех темах прошёл бы deepEqual тривиально.
  assert.ok(first.length > 0, "role-key set is empty — cross-theme equality is vacuous");
  for (let i = 1; i < keysets.length; i++) {
    assert.deepEqual(
      keysets[i],
      first,
      `theme '${THEMES[i]}' emits a different role-key set than '${THEMES[0]}'`,
    );
  }

  // Каждая достижимая роль (не none/unreachable) присутствует в vars.
  for (const theme of THEMES) {
    const res = e.resolveTheme(bg, theme);
    for (const [key, role] of Object.entries(res.roles)) {
      const reachable = ["color", "translucent", "glow"].includes(role.kind);
      const present = Object.prototype.hasOwnProperty.call(res.vars, role.cssVar);
      if (reachable) {
        assert.ok(present, `${theme}/${key}: reachable role missing from vars`);
      }
    }
  }
});
