#!/usr/bin/env node
/**
 * naming-inventory — фактический нейминг-инвентарь lab-colors.
 *
 * Роль: справка-скрипт и библиотека фактов ФС. Единственный источник фактов
 * для docs/NAMING.md и scripts/check-docs-drift.mjs (гейт импортирует отсюда,
 * чтобы дока и гейт сверялись с ОДНОЙ реальностью, а не с двумя копиями).
 * Портировано с эталона экосистемы (labui#150, lab-icons#53, lab-motion).
 *
 * Отличие от Node-монорепо эталона: lab-colors — Rust-workspace, закон имён
 * по-доменный (snake_case — закон Rust/PEP 8, не отступление):
 *   1. крейты workspace: crates/* c Cargo.toml, имена labcolors-<роль>;
 *   2. члены workspace из Cargo.toml (глоб crates/* разворачивается по ФС);
 *   3. экспорт-субпути packages/colors/package.json — публичный API npm;
 *   4. python-скрипты scripts/*.py (golden-эталоны);
 *   5. доки docs/**\/*.md, векторы conformance/vectors/*.json;
 *   6. файлы вне по-доменного закона имён (см. lawForFile).
 *
 * CLI: `node scripts/naming-inventory.mjs` — сводка + полный JSON.
 */

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = join(__dirname, '..');

/** Генерируемое/чужое вне публичного package-контракта — не инвентарь. */
const SKIP_DIRS = new Set([
  'node_modules',
  'target',
  '.git',
  'dist',
  '.build',
  'coverage',
]);

/** Директории, по которым бежит скан закона имён. */
export const SCAN_TOPS = [
  'crates',
  'scripts',
  'docs',
  'packages',
  'conformance',
  'bindings',
  'reference',
  'experiments',
];

/** Имена, продиктованные тулингом, — вне юрисдикции закона. */
const TOOL_FIXED = new Set([
  'Cargo.toml',
  'Cargo.lock',
  'Package.swift',
  'LICENSE',
]);

/** Рекурсивный список файлов (POSIX-пути относительно base, дотфайлы мимо). */
export function walk(dir, out = [], base = dir) {
  if (!existsSync(dir)) return out;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.name.startsWith('.')) continue;
    if (e.isDirectory()) {
      if (!SKIP_DIRS.has(e.name)) walk(join(dir, e.name), out, base);
    } else {
      out.push(relative(base, join(dir, e.name)).replaceAll('\\', '/'));
    }
  }
  return out;
}

/* ------------------------------------------------------------------ *
 * Workspace: члены и крейты                                           *
 * ------------------------------------------------------------------ */

/**
 * Члены workspace из корневого Cargo.toml.
 * Глоб `crates/*` разворачивается по ФС (как это делает cargo);
 * членом считается директория с Cargo.toml.
 */
export function workspaceMembers(root = ROOT) {
  const toml = readFileSync(join(root, 'Cargo.toml'), 'utf8');
  const m = toml.match(/^members\s*=\s*\[([^\]]*)\]/m);
  if (!m) return [];
  const entries = [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
  const members = [];
  for (const e of entries) {
    if (e.endsWith('/*')) {
      const parent = e.slice(0, -2);
      const dir = join(root, parent);
      if (!existsSync(dir)) continue;
      for (const d of readdirSync(dir, { withFileTypes: true })) {
        if (d.isDirectory() && existsSync(join(dir, d.name, 'Cargo.toml'))) {
          members.push(`${parent}/${d.name}`);
        }
      }
    } else {
      members.push(e);
    }
  }
  return members.sort();
}

/** Имя пакета из Cargo.toml члена (`name = "…"` в [package]). */
export function crateName(memberPath, root = ROOT) {
  const toml = readFileSync(join(root, memberPath, 'Cargo.toml'), 'utf8');
  const m = toml.match(/^name\s*=\s*"([^"]+)"/m);
  return m ? m[1] : null;
}

/** Крейты семейства labcolors-*: члены под crates/. */
export function workspaceCrates(root = ROOT) {
  return workspaceMembers(root)
    .filter((m) => m.startsWith('crates/'))
    .map((m) => crateName(m, root))
    .filter(Boolean)
    .sort();
}

/* ------------------------------------------------------------------ *
 * npm-пакет @labpics/colors                                           *
 * ------------------------------------------------------------------ */

/** Экспорт-субпути packages/colors/package.json: ['.', './apply-theme', …]. */
export function exportSubpaths(root = ROOT) {
  const pkg = JSON.parse(
    readFileSync(join(root, 'packages', 'colors', 'package.json'), 'utf8'),
  );
  return Object.keys(pkg.exports ?? {}).sort();
}

/* ------------------------------------------------------------------ *
 * Скрипты, доки, векторы                                              *
 * ------------------------------------------------------------------ */

/** Python-скрипты scripts/*.py (golden-эталоны). */
export function pythonScripts(root = ROOT) {
  return walk(join(root, 'scripts'))
    .filter((f) => f.endsWith('.py'))
    .sort();
}

/** Все маркдаун-доки docs/**\/*.md (рекурсивно). */
export function docsMarkdown(root = ROOT) {
  return walk(join(root, 'docs'))
    .filter((f) => f.endsWith('.md'))
    .sort();
}

/** Векторные файлы conformance/vectors/*.json (включая manifest). */
export function conformanceVectors(root = ROOT) {
  return walk(join(root, 'conformance', 'vectors'))
    .filter((f) => f.endsWith('.json'))
    .sort();
}

/* ------------------------------------------------------------------ *
 * По-доменный закон имён                                              *
 * ------------------------------------------------------------------ */

const SNAKE = /^[a-z0-9_]+$/;
const KEBAB = /^[a-z0-9]+(-[a-z0-9]+)*$/;
const PASCAL = /^[A-Z][A-Za-z0-9]*$/;
// .md: kebab-стем | КАПС-канон (README, NAMING) | нумерованный ADR 0001-…
const MD = /^([a-z0-9]+(-[a-z0-9]+)*|[A-Z]+(-[A-Z]+)*|\d{4}-[a-z0-9]+(-[a-z0-9]+)*)$/;

/** Стем файла: имя без расширения (двойные вроде `.d.ts` тоже срезаются). */
export function fileStem(base) {
  return base.replace(/\.[^.]+(\.[^.]+)?$/, '');
}

/**
 * Закон для файла по домену. true — имя законно.
 * snake_case для .rs/.py — ЗАКОН (rustc/PEP 8), не отступление;
 * PascalCase для .swift — конвенция Swift; остальное — kebab-case.
 */
export function lawForFile(path) {
  const base = path.split('/').pop();
  if (TOOL_FIXED.has(base)) return true;
  const stem = fileStem(base);
  if (base.endsWith('.rs') || base.endsWith('.py')) return SNAKE.test(stem);
  if (base.endsWith('.swift')) return PASCAL.test(stem);
  if (base.endsWith('.md')) return MD.test(stem);
  return KEBAB.test(stem);
}

/** Файлы SCAN_TOPS вне по-доменного закона имён. */
export function nonLawFiles(root = ROOT) {
  const packageRoot = join(root, 'packages', 'colors');
  const packageJson = JSON.parse(readFileSync(join(packageRoot, 'package.json'), 'utf8'));
  const generatedPackageFiles = new Set();
  for (const declaredPath of packageJson.files ?? []) {
    const packagePath = declaredPath.replace(/^\.\//, '').replace(/\/+$/, '');
    if (packagePath === 'pkg' || packagePath === 'compiler') {
      for (const file of walk(join(packageRoot, packagePath))) {
        generatedPackageFiles.add(`packages/colors/${packagePath}/${file}`);
      }
    } else if (/^(?:pkg|compiler)\//.test(packagePath)) {
      generatedPackageFiles.add(`packages/colors/${packagePath}`);
    }
  }
  const bad = new Set();
  for (const top of SCAN_TOPS) {
    for (const f of walk(join(root, top))) {
      const repositoryPath = `${top}/${f}`;
      // Declared generated outputs are package-contract facts, not source-tree
      // facts. An undeclared file beside them still passes through the law.
      if (generatedPackageFiles.has(repositoryPath)) continue;
      if (!lawForFile(f)) bad.add(repositoryPath);
    }
  }
  for (const path of generatedPackageFiles) {
    if (!lawForFile(path)) bad.add(path);
  }
  return [...bad].sort();
}

/* ------------------------------------------------------------------ */

/** Полный инвентарь одним объектом (факты для доки и гейта). */
export function collectInventory(root = ROOT) {
  return {
    members: workspaceMembers(root),
    crates: workspaceCrates(root),
    subpaths: exportSubpaths(root),
    pyScripts: pythonScripts(root),
    docsMd: docsMarkdown(root),
    vectors: conformanceVectors(root),
    nonLaw: nonLawFiles(root),
  };
}

const isCLI = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isCLI) {
  const inv = collectInventory(ROOT);
  console.info(
    `naming-inventory: членов workspace ${inv.members.length}, ` +
      `крейтов ${inv.crates.length}, субпутей ${inv.subpaths.length}, ` +
      `py-скриптов ${inv.pyScripts.length}, доков ${inv.docsMd.length}, ` +
      `векторов ${inv.vectors.length}, вне закона имён ${inv.nonLaw.length}`,
  );
  console.info(JSON.stringify(inv, null, 2));
}
