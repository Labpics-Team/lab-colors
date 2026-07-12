#!/usr/bin/env node
/**
 * docs-drift guard для lab-colors (порт эталона labui#150 / lab-icons#53 /
 * lab-motion scripts/check-docs-drift.mjs на Rust-workspace).
 *
 * Доки vs реальность — сверяет утверждения docs/NAMING.md с фактами ФС
 * (факты собирает scripts/naming-inventory.mjs, единый источник):
 *
 *   1. docs/NAMING.md существует и объявляет роль в первых 5 строках.
 *   2. Инвентарная таблица «сверяется гейтом»: числа в доке == факты ФС
 *      (члены workspace, крейты, субпути, py-скрипты, доки, векторы,
 *      файлы вне закона имён).
 *   3. Крейты двусторонне: каждый крейт workspace упомянут в NAMING.md;
 *      каждое упоминание `labcolors-*` существует в crates/ (фантомы запрещены).
 *   4. Члены workspace из Cargo.toml разрешаются в ФС (Cargo.toml на месте).
 *   5. Субпути двусторонне: каждый экспорт-субпуть @labpics/colors упомянут
 *      в NAMING.md; каждое упоминание `./x` существует в exports.
 *   6. Закон субпутей: kebab-домен `./[a-z0-9-]+`; иное — только через
 *      «Известные отступления» (служебный `./package.json` разрешён законом).
 *   7. Python-скрипты двусторонне: scripts/*.py ↔ упоминания `*.py` в доке.
 *   8. Файлы вне закона имён перечислены в «Известных отступлениях»,
 *      исчезнувшие из ФС записи обязаны уйти из доки.
 *
 * Функции чистые и экспортируются — поведение проверяемо юнитами
 * (scripts/docs-drift.test.mjs, `node --test scripts/docs-drift.test.mjs`).
 */

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ROOT, collectInventory } from './naming-inventory.mjs';

/** Субпути, законные вне kebab-домена: корень и служебный стандарт npm. */
export const SUBPATH_LAW_ALLOW = new Set(['.', './package.json']);
const SUBPATH_KEBAB = /^\.\/[a-z0-9-]+$/;

/* ------------------------------------------------------------------ *
 * 1. Роль дока                                                        *
 * ------------------------------------------------------------------ */

export function hasDocRole(text) {
  const head = text.split('\n').slice(0, 5).join('\n');
  return /(справка|ADR|канон|гайд|отчёт|роль:)/i.test(head);
}

/* ------------------------------------------------------------------ *
 * 2. Инвентарная таблица                                              *
 * ------------------------------------------------------------------ */

/** Строки `| метка | N |` из таблицы под заголовком «Инвентарь». */
export function extractInventoryTable(text) {
  const rows = new Map();
  for (const m of text.matchAll(/^\|\s*([^|]+?)\s*\|\s*(\d+)\s*\|\s*$/gm)) {
    rows.set(m[1], Number(m[2]));
  }
  return rows;
}

/** Метрика доки → факт ФС; ключ ищется подстрокой в метке строки. */
export function inventoryTableErrors(rows, inv) {
  const expected = [
    ['членов workspace', inv.members.length],
    ['крейтов', inv.crates.length],
    ['экспорт-субпутей', inv.subpaths.length],
    ['python-скриптов', inv.pyScripts.length],
    ['маркдаун-доков', inv.docsMd.length],
    ['векторов', inv.vectors.length],
    ['вне закона имён', inv.nonLaw.length],
  ];
  const errs = [];
  for (const [key, fact] of expected) {
    const row = [...rows.entries()].find(([label]) => label.includes(key));
    if (!row) {
      errs.push(`NAMING.md: в инвентарной таблице нет строки «${key}»`);
    } else if (row[1] !== fact) {
      errs.push(`NAMING.md: «${row[0]}» = ${row[1]}, факт ФС = ${fact}`);
    }
  }
  return errs;
}

/* ------------------------------------------------------------------ *
 * 3–4. Крейты и члены workspace                                       *
 * ------------------------------------------------------------------ */

/** Бэктик-упоминания крейтов `labcolors-*` в доке. */
export function extractCrateMentions(text) {
  return [...text.matchAll(/`(labcolors-[a-z0-9-]+)`/g)].map((m) => m[1]);
}

export function crateMentionErrors(text, crates) {
  const real = new Set(crates);
  const errs = [];
  for (const c of new Set(extractCrateMentions(text))) {
    if (!real.has(c)) {
      errs.push(`NAMING.md упоминает несуществующий крейт \`${c}\` — фантом`);
    }
  }
  for (const c of crates) {
    if (!text.includes(`\`${c}\``)) {
      errs.push(`крейт ${c} не упомянут в NAMING.md`);
    }
  }
  return errs;
}

/** Каждый член workspace обязан разрешаться в ФС. */
export function memberResolutionErrors(members, root = ROOT) {
  return members
    .filter((m) => !existsSync(join(root, m, 'Cargo.toml')))
    .map((m) => `член workspace не разрешается в ФС: ${m}/Cargo.toml нет`);
}

/* ------------------------------------------------------------------ *
 * 5–6. Экспорт-субпути @labpics/colors                                *
 * ------------------------------------------------------------------ */

/** Бэктик-упоминания субпутей `./x` в доке. */
export function extractSubpathMentions(text) {
  return [...text.matchAll(/`(\.\/[^`\s]+)`/g)].map((m) => m[1]);
}

export function subpathMentionErrors(text, subpaths) {
  const real = new Set(subpaths);
  const errs = [];
  for (const s of new Set(extractSubpathMentions(text))) {
    if (!real.has(s)) {
      errs.push(`NAMING.md упоминает несуществующий субпуть \`${s}\` — фантом`);
    }
  }
  for (const s of subpaths) {
    if (s !== '.' && !text.includes(`\`${s}\``)) {
      errs.push(`экспорт-субпуть ${s} не упомянут в NAMING.md`);
    }
  }
  return errs;
}

/** Закон субпутей: kebab-домен, иное — через «Известные отступления». */
export function subpathLawErrors(subpaths, section) {
  const errs = [];
  for (const s of subpaths) {
    if (SUBPATH_LAW_ALLOW.has(s) || SUBPATH_KEBAB.test(s)) continue;
    if (!section.includes(`\`${s}\``)) {
      errs.push(
        `субпуть ${s} вне kebab-закона и не зафиксирован в «Известных отступлениях»`,
      );
    }
  }
  return errs;
}

/* ------------------------------------------------------------------ *
 * 7. Python-скрипты                                                   *
 * ------------------------------------------------------------------ */

export function extractPyMentions(text) {
  return [...text.matchAll(/`([a-z0-9_]+\.py)`/g)].map((m) => m[1]);
}

export function pyScriptErrors(text, pyScripts) {
  const real = new Set(pyScripts);
  const errs = [];
  for (const p of new Set(extractPyMentions(text))) {
    if (!real.has(p)) {
      errs.push(`NAMING.md упоминает несуществующий скрипт \`${p}\` — фантом`);
    }
  }
  for (const p of pyScripts) {
    if (!text.includes(`\`${p}\``)) {
      errs.push(`python-скрипт scripts/${p} не упомянут в NAMING.md`);
    }
  }
  return errs;
}

/* ------------------------------------------------------------------ *
 * 8. Известные отступления                                            *
 * ------------------------------------------------------------------ */

/** Текст раздела «Известные отступления» (до следующего `## `). */
export function deviationsSection(text) {
  const m = text.match(/^##[^\n]*Известные отступления[^\n]*\n([\s\S]*?)(?=^## |\n*$(?![\s\S]))/m);
  return m ? m[1] : '';
}

export function fileLawErrors(section, inv) {
  const errs = inv.nonLaw
    .filter((f) => !section.includes(`\`${f}\``))
    .map((f) => `файл вне закона имён не зафиксирован в «Известных отступлениях»: ${f}`);
  // Обратная сверка: записи-пути, которых больше нет среди фактов, — вон из доки.
  const facts = new Set([
    ...inv.nonLaw,
    ...inv.subpaths.filter((s) => !SUBPATH_LAW_ALLOW.has(s) && !SUBPATH_KEBAB.test(s)),
  ]);
  for (const m of section.matchAll(/`((?:\.\/|[a-z]+\/)[^`\s]+)`/g)) {
    if (!facts.has(m[1])) {
      errs.push(
        `«Известные отступления» перечисляют ${m[1]}, но такого факта больше нет — убери из доки`,
      );
    }
  }
  return errs;
}

/* ------------------------------------------------------------------ *
 * 9. Клейм-гигиена (#306): ложная provenance и завышенные claims       *
 * ------------------------------------------------------------------ */

/** Голый `#89` вне hex-цвета: реальный GitHub #89 — закрытый PR про toHex,
 *  а не owner материала. Hex-цвета (`#89CFF0`, `#8944AB`) не совпадают. */
export const BARE_MATERIAL_89 = /(^|[^0-9A-Fa-f])#89(?![0-9A-Fa-f])/;

/** Ошибки клеймов одного файла. Правила:
 *  1) голый `#89` — ложный Material-owner;
 *  2) «полного результата» рядом с point-Glow evidence — завышение до
 *     whole-effect (ядро меряет только изолированные point-слои);
 *  3) `labui-material.css` — несуществующий потребитель;
 *  4) строчное `platform-characterized` — сильнее фактического статуса
 *     (typed `PlatformCharacterized` не конструируем до attestation registry
 *     #258; текущий статус — legacy-platform-dependent). */
export function claimErrorsInText(rel, text) {
  const errs = [];
  if (BARE_MATERIAL_89.test(text)) {
    errs.push(
      `${rel}: ложная Material-ссылка «#89» (реальный #89 — PR про toHex); канон — docs/whitepaper.md §3.7`,
    );
  }
  // Контекстное правило: фраза запрещена только рядом с Glow-evidence —
  // обычное русское словосочетание в другом контексте не флагается.
  if (text.includes('полного результата') && /[Gg]low/.test(text)) {
    errs.push(
      `${rel}: point-Glow описан как «полного результата» — допустимы только изолированные point-замеры`,
    );
  }
  if (text.includes('labui-material.css')) {
    errs.push(`${rel}: несуществующий потребитель labui-material.css`);
  }
  // Дефисная форма в любой капитализации; typed CamelCase-имя
  // `PlatformCharacterized` (без дефиса) остаётся законным deferred-термином.
  if (/[Pp]latform-characterized/.test(text)) {
    errs.push(
      `${rel}: строчное «platform-characterized» сильнее статуса legacy-platform-dependent (attestation registry — #258)`,
    );
  }
  return errs;
}

const CLAIM_EXT = /\.(rs|md|mjs|js|ts)$/;
const CLAIM_SKIP = /node_modules|[\/]pkg[\/]|[\/]target[\/]|[\/]\.git[\/]|mutants\.out/;

function walkClaimFiles(dir, acc) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (CLAIM_SKIP.test(full)) continue;
    if (entry.isDirectory()) walkClaimFiles(full, acc);
    else if (CLAIM_EXT.test(entry.name)) acc.push(full);
  }
}

/** Скан клеймов по Rust/WASM/TS/докам (анти-вакуум #306: минимум три слоя). */
export function claimHygieneErrors(root = ROOT) {
  const errs = [];
  const files = [];
  for (const rel of ['crates', 'packages/colors', 'docs']) {
    const dir = join(root, rel);
    if (existsSync(dir)) walkClaimFiles(dir, files);
  }
  for (const rel of ['README.md', 'conformance/README.md', 'CHANGELOG.md']) {
    const f = join(root, rel);
    if (existsSync(f)) files.push(f);
  }
  for (const f of files) {
    errs.push(...claimErrorsInText(f.slice(root.length + 1), readFileSync(f, 'utf8')));
  }
  return errs;
}

/* ------------------------------------------------------------------ *
 * Аудит целиком                                                       *
 * ------------------------------------------------------------------ */

export function auditRepo(root = ROOT) {
  const errors = [];
  const namingPath = join(root, 'docs', 'NAMING.md');
  const inv = collectInventory(root);
  if (!existsSync(namingPath)) {
    return { errors: ['docs/NAMING.md отсутствует — канона нет'], inv };
  }
  const text = readFileSync(namingPath, 'utf8');

  if (!hasDocRole(text)) {
    errors.push('docs/NAMING.md: роль не объявлена в первых 5 строках');
  }
  errors.push(...inventoryTableErrors(extractInventoryTable(text), inv));
  errors.push(...crateMentionErrors(text, inv.crates));
  errors.push(...memberResolutionErrors(inv.members, root));
  errors.push(...subpathMentionErrors(text, inv.subpaths));
  const section = deviationsSection(text);
  if (!section) {
    errors.push('docs/NAMING.md: нет раздела «Известные отступления»');
  }
  errors.push(...subpathLawErrors(inv.subpaths, section));
  errors.push(...pyScriptErrors(text, inv.pyScripts));
  errors.push(...fileLawErrors(section, inv));
  errors.push(...claimHygieneErrors(root));

  return { errors, inv };
}

/* ------------------------------------------------------------------ */

const isCLI = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isCLI) {
  const { errors, inv } = auditRepo(ROOT);
  console.info(
    `check-docs-drift: факт ФС — членов workspace ${inv.members.length}, ` +
      `крейтов ${inv.crates.length}, субпутей ${inv.subpaths.length}, ` +
      `py-скриптов ${inv.pyScripts.length}, доков ${inv.docsMd.length}, ` +
      `векторов ${inv.vectors.length}, вне закона имён ${inv.nonLaw.length}`,
  );
  if (errors.length) {
    for (const e of errors) console.error(`  ✗ ${e}`);
    console.error(`check-docs-drift: FAIL (${errors.length})`);
    process.exit(1);
  }
  console.info('check-docs-drift: PASS — доки совпадают с реальностью');
}
