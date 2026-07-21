// Сквозные инварианты ВЕРХА цепочки, как их видит БРАУЗЕР: живой движок
// (resolveTheme) → эмитированная oklch-строка в `vars` → ШТАТНЫЙ парсер
// потребителя (parseCssColor) → перепроверка легальности (recheckContrast).
//
// Почему здесь, а не в Rust: `vars[--lab-*]` — это ровно та строка, которую
// прочитает браузер, а `parseCssColor` и скрытый exact point bridge — тот самый
// путь пакета, что реконструирует цвет на странице (его же использует
// effectiveBackground). Так тест меряет ПОТЕРИ НА СЕРИАЛИЗАЦИИ ВЫХОДА, а не
// внутри солвера, и без параллельной копии физики контраста.
//
// Что уже закрыто в другом месте (НЕ дублируем):
// - core `oklch.rs::round_trip_is_byte_exact_*` — emit↔parse байт-точны на
//   решётке шага 5 и полном сером ramp (не объявлен полный куб);
// - `oklch-parse.test.mjs` — parseCssColor декодит эмиссию на 16 фикстурах;
// - core `property_invariants.rs::every_floored_role_clears_its_wcag_floor…` —
//   пол на СОБСТВЕННОМ hex солвера (не на репарснутой строке);
// - wasm `wasm_parity.rs` — JS-граница == core-оракул внутри того же wasm runtime.
// Дыра: КОМПОЗИЦИЯ этих доказательств на ЖИВОМ корпусе ролей — «цвет, который
// браузер соберёт из emitted vars, всё ещё проходит свой пол/таргет» — как
// единая цепочка, а не транзитивность двух изолированных проверок. Для
// полупрозрачных ролей это единственное место, где проверяется, что
// эмитированные тинт+альфа, скомпозиченные браузером, дают ОБЕЩАННЫЙ композит.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  initSync,
  LabColors,
  __over,
} from "../pkg/labcolors.js";
import { applyTheme } from "../apply-theme.js";
import { parseCssColor, toHex } from "../effective-bg.js";

// Инициализация wasm в node: pkg собран под `--target web` (fetch по URL), а в
// node грузим байты напрямую. Оборачиваем в WebAssembly.Module и передаём
// объектом `{ module }` — штатная форма initSync без deprecated-варнинга.
initSync({
  module: new WebAssembly.Module(readFileSync(new URL("../pkg/labcolors_bg.wasm", import.meta.url))),
});

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

// C8d packed recheck boundary: recheckContrast takes a packed `0x00RRGGBB`
// background word, a `Uint32Array` of foregrounds, and a numeric theme handle.
const pk = (hex) => Number.parseInt(hex.replace(/^#/u, ""), 16) >>> 0;
const recheck1 = (e, bg, fgHex, theme) =>
  e.recheckContrast(pk(bg), Uint32Array.of(pk(fgHex)), e.themeHandle(theme));

// [r,g,b] из parseCssColor-результата (отбрасываем α).
const rgb = (parsed) => [parsed[0], parsed[1], parsed[2]];
const packRgb24 = (parsed) =>
  ((Math.round(parsed[0]) << 16) |
    (Math.round(parsed[1]) << 8) |
    Math.round(parsed[2])) >>> 0;
const hexFromRgb24 = (packed) =>
  `#${packed.toString(16).padStart(6, "0").toUpperCase()}`;

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
        const flat = recheck1(e, bg, paintedHex, theme);
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
// ЛЕГАЛЬНОСТЬ НАСКВОЗЬ — полупрозрачные роли (encoded-sRGB8 reference)
// ─────────────────────────────────────────────────────────────────────────────

// Численный контракт здесь — encoded-sRGB8 source-over reference, реализованный
// штатным consumer-кодом. Это проверка единства engine↔package, а не заявление,
// что любой renderer без явно заданного color-management профиля совпадёт с ним.
test("translucent serialization fidelity: emitted tint+alpha, reference composite and reported metrics agree exactly", () => {
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
        // Вычисленная α — часть сертификата композита: строка обязана вернуть
        // тот же binary64, а не близкое округлённое значение.
        assert.ok(
          Object.is(alpha, role.alpha),
          `${theme}/${bg}/${key}: reparsed alpha ${alpha} != reported ${role.alpha}`,
        );

        // Reference-композит из эмитированных значений обязан совпасть с
        // сертификатом побайтно: допуск скрыл бы другой цвет и другие метрики.
        const packed = __over(
          packRgb24(parsed),
          alpha,
          packRgb24(bgParsed),
        );
        assert.notEqual(packed, 0xFFFFFFFF, "admitted emitted layer must compose");
        const compHex = hexFromRgb24(packed);
        assert.equal(
          compHex,
          role.compositeHex,
          `${theme}/${bg}/${key}: reference composite ${compHex} != promised ${role.compositeHex}`,
        );

        // Самосогласованность движка: перепроверка ЕГО ЖЕ compositeHex
        // воспроизводит отданные метрики композита (не зависит от находки выше).
        const flat = recheck1(e, bg, role.compositeHex, theme);
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
        assert.equal(
          toHex(rgb(parseCssColor(res.vars[role.cssVar]))),
          role.haloHex,
          `${theme}/${bg}/${key}: halo css must reconstruct haloHex`,
        );

        // Сателлит -core: валидная oklch-строка.
        const core = res.vars[`${role.cssVar}-core`];
        assert.ok(core && parseCssColor(core), `${theme}/${bg}/${key}: -core must be valid oklch`);
        assert.equal(
          toHex(rgb(parseCssColor(core))),
          role.coreHex,
          `${theme}/${bg}/${key}: core css must reconstruct coreHex`,
        );

        // Сателлит -alpha — буквальный SSOT, не повторное округление числа.
        const alphaVar = res.vars[`${role.cssVar}-alpha`];
        const a = Number(alphaVar);
        assert.ok(
          Number.isFinite(a) && a > 0 && a <= 1,
          `${theme}/${bg}/${key}: -alpha must be a number in (0,1]: ${alphaVar}`,
        );
        assert.equal(alphaVar, role.alphaCss, `${theme}/${bg}/${key}: alpha var != alphaCss`);
        assert.ok(Object.is(a, role.alpha), `${theme}/${bg}/${key}: alpha lost binary64 bits`);

        assert.equal(role.constraintLayer, "halo");
        assert.equal(role.compositeProfile, "encoded-srgb8-screen-v1");
        assert.equal(role.compositeGuarantee, "bit-exact");
        assert.equal(role.layerRecipeProfile, "cam16-jprime-oklab-cusp-v1");
        assert.equal(role.appearanceDiagnosticProfile, "cam16-ucs-jprime-li2017-v1");
        assert.equal(role.selectionDiagnosticProfile, "cam16-ucs-jprime-li2017-v1");
        assert.equal(role.decisionProfile, "legacy-platform-dependent-v1");
        assert.deepEqual(role.decisionGuarantee, {
          kind: "legacy-platform-dependent-v1",
        });
        assert.ok(["legacy-reached", "legacy-unreachable"].includes(role.targetStatus));
        if (role.targetStatus === "legacy-reached") {
          assert.ok(role.haloAchievedDj >= role.targetDj);
        } else {
          assert.ok(role.haloAchievedDj < role.targetDj);
        }

        // Оба поля — point-reference замеры ИЗОЛИРОВАННЫХ слоёв. Полный
        // spatial stack без геометрии здесь намеренно не реконструируется.
        const [br, bgc, bb] = parseCssColor(bg);
        const screenHex = (layerCss) => {
          const [lr, lg, lb] = parseCssColor(layerCss);
          // Тот же конечный byte-reference, что у ядра: нормализация
          // byte/255 перед обратным умножением сдвигает точные half-tie.
          const channel = (background, layer) =>
            background + a * layer * (255 - background) / 255;
          return toHex([channel(br, lr), channel(bgc, lg), channel(bb, lb)]);
        };
        assert.equal(screenHex(res.vars[role.cssVar]), role.haloCompositeHex);
        assert.equal(screenHex(core), role.coreCompositeHex);
      }
    }
  }
  assert.ok(glowChecked > 0, "no glow roles exercised — passport should carry glows");
});

test("stable glow indeterminate emits no fallback vars and clears previous legacy satellites", () => {
  const legacy = engine().resolveTheme("#101012", "dark");
  assert.ok(legacy.vars["--lab-fx-glow-brand"]);

  const stablePassport = structuredClone(PASSPORT_OBJ);
  const brandGlow = stablePassport.roles.find(({ name }) => name === "fx-glow-brand");
  assert.ok(brandGlow, "anti-vacuum: passport must contain fx-glow-brand");
  brandGlow.recipe.decision_profile = "stable-v1";
  const stableEngine = new LabColors();
  stableEngine.loadConfig(JSON.stringify(stablePassport));
  const stable = stableEngine.resolveTheme("#101012", "dark");
  const role = stable.roles["fx-glow-brand"];
  assert.equal(role.kind, "glow-indeterminate");
  assert.equal(role.numericalSiteId, "glow-target-or-maximum-v1");
  assert.equal(role.reason, "sound-bound-unavailable");
  assert.equal(role.bounds.kind, "unavailable");
  for (const key of [role.cssVar, `${role.cssVar}-core`, `${role.cssVar}-alpha`]) {
    assert.equal(stable.vars[key], undefined, `${key}: no implicit legacy fallback`);
  }

  const props = new Map();
  const element = {
    style: {
      get length() { return props.size; },
      item(index) { return [...props.keys()][index] ?? ""; },
      setProperty(key, value) { props.set(key, value); },
      removeProperty(key) { props.delete(key); },
    },
  };
  applyTheme(element, legacy);
  assert.ok(props.has("--lab-fx-glow-brand-alpha"));
  applyTheme(element, stable);
  assert.ok(!props.has("--lab-fx-glow-brand"));
  assert.ok(!props.has("--lab-fx-glow-brand-core"));
  assert.ok(!props.has("--lab-fx-glow-brand-alpha"));
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
      // Ожидаемое число ключей vars выводится из wire-kind: solid/
      // translucent несут одно значение, Glow/Material — primary и два
      // сателлита, терминальные исходы — ноль. Если сателлит перезаписал
      // чужой primary, фактическое число будет меньше.
      let expected = 0;
      for (const role of Object.values(res.roles)) {
        switch (role.kind) {
          case "color":
          case "translucent":
            expected += 1;
            break;
          case "glow":
          case "material":
            expected += 3;
            break;
          case "none":
          case "failure":
          case "glow-indeterminate":
            break;
          default:
            assert.fail(`unknown role kind: ${String(role.kind)}`);
        }
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

test("role-key set is theme-invariant and vars contain only selected outcomes", () => {
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

  // Каждый выбранный primary присутствует в vars; терминальный исход
  // не может оставить правдоподобный CSS fallback под своим именем.
  for (const theme of THEMES) {
    const res = e.resolveTheme(bg, theme);
    for (const [key, role] of Object.entries(res.roles)) {
      const selected = ["color", "translucent", "glow", "material"].includes(role.kind);
      const present = Object.prototype.hasOwnProperty.call(res.vars, role.cssVar);
      assert.equal(
        present,
        selected,
        `${theme}/${key}: primary var does not match the terminal outcome`,
      );
    }
  }
});
