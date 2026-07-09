/**
 * Юнит-тесты гейта docs-drift (`node --test scripts/docs-drift.test.mjs`).
 *
 * Чистые функции гейта проверяются на синтетических строках (позитив +
 * негатив на каждую проверку), сборщик фактов — на реальном дереве репо
 * (детерминированные инварианты: имена крейтов, субпути, законы имён).
 */

import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  ROOT,
  collectInventory,
  crateName,
  fileStem,
  lawForFile,
  workspaceCrates,
  workspaceMembers,
} from './naming-inventory.mjs';
import {
  auditRepo,
  crateMentionErrors,
  deviationsSection,
  extractCrateMentions,
  extractInventoryTable,
  extractPyMentions,
  extractSubpathMentions,
  fileLawErrors,
  hasDocRole,
  inventoryTableErrors,
  memberResolutionErrors,
  pyScriptErrors,
  subpathLawErrors,
  subpathMentionErrors,
} from './check-docs-drift.mjs';

/* ---------------- инвентарь: закон имён ---------------- */

test('fileStem срезает одинарные и двойные расширения', () => {
  assert.equal(fileStem('adapt-theme.d.ts'), 'adapt-theme');
  assert.equal(fileStem('smoke.consumer.ts'), 'smoke');
  assert.equal(fileStem('golden_ref.py'), 'golden_ref');
  assert.equal(fileStem('whitepaper.md'), 'whitepaper');
});

test('lawForFile: по-доменные законы', () => {
  // Rust/Python — snake_case: закон, не отступление.
  assert.ok(lawForFile('crates/x/src/accent_balance.rs'));
  assert.ok(lawForFile('scripts/jhk_golden_ref.py'));
  assert.ok(!lawForFile('crates/x/src/accentBalance.rs'));
  // Swift — PascalCase.
  assert.ok(lawForFile('bindings/swift/Tests/ConformanceTests.swift'));
  assert.ok(!lawForFile('bindings/swift/conformance_tests.swift'));
  // Маркдаун — kebab | КАПС-канон | нумерованный ADR.
  assert.ok(lawForFile('docs/empirical-residue.md'));
  assert.ok(lawForFile('docs/NAMING.md'));
  assert.ok(lawForFile('docs/decisions/0001-config-boundary.md'));
  assert.ok(!lawForFile('docs/Empirical_Residue.md'));
  // Остальное — kebab; имена тулинга — вне юрисдикции.
  assert.ok(lawForFile('packages/colors/apply-theme.js'));
  assert.ok(lawForFile('crates/labcolors-core/Cargo.toml'));
  assert.ok(lawForFile('bindings/swift/Package.swift'));
  assert.ok(!lawForFile('packages/colors/bench/AFTER.txt'));
  assert.ok(!lawForFile('crates/x/tests/data/labui.config.prod.json'));
});

test('workspaceMembers разворачивает глоб crates/* по ФС', () => {
  const members = workspaceMembers(ROOT);
  assert.ok(members.includes('crates/labcolors-core'));
  assert.ok(members.includes('experiments/psychophysics'));
  assert.equal(members.length, new Set(members).size, 'без дублей');
});

test('workspaceCrates читает имена пакетов из Cargo.toml', () => {
  const crates = workspaceCrates(ROOT);
  assert.ok(crates.includes('labcolors-core'));
  assert.ok(crates.every((c) => c.startsWith('labcolors-')));
  assert.equal(crateName('crates/labcolors-wasm', ROOT), 'labcolors-wasm');
});

/* ---------------- гейт: роль и таблица ---------------- */

test('hasDocRole: роль в первых 5 строках, ниже — не считается', () => {
  assert.ok(hasDocRole('# X\n\n> Роль: канон именования.\n'));
  assert.ok(!hasDocRole('# X\n\n\n\n\n\n> Роль: канон.\n'));
});

test('extractInventoryTable парсит строки «| метка | N |»', () => {
  const rows = extractInventoryTable('| крейтов | 5 |\n| прочее | не число |\n');
  assert.equal(rows.get('крейтов'), 5);
  assert.equal(rows.size, 1);
});

test('inventoryTableErrors: совпадение — ноль ошибок, дрейф и пропуск — ловятся', () => {
  const inv = {
    members: ['a', 'b'],
    crates: ['x'],
    subpaths: ['.'],
    pyScripts: [],
    docsMd: ['d.md'],
    vectors: [],
    nonLaw: [],
  };
  const good = new Map([
    ['членов workspace', 2],
    ['крейтов', 1],
    ['экспорт-субпутей', 1],
    ['python-скриптов', 0],
    ['маркдаун-доков', 1],
    ['векторов', 0],
    ['вне закона имён', 0],
  ]);
  assert.deepEqual(inventoryTableErrors(good, inv), []);
  const drift = new Map(good);
  drift.set('крейтов', 7);
  assert.match(inventoryTableErrors(drift, inv)[0], /= 7, факт ФС = 1/);
  const missing = new Map(good);
  missing.delete('векторов');
  assert.match(inventoryTableErrors(missing, inv)[0], /нет строки «векторов»/);
});

/* ---------------- гейт: крейты и члены ---------------- */

test('crateMentionErrors: фантом и пропуск — двусторонняя сверка', () => {
  assert.deepEqual(crateMentionErrors('канон: `labcolors-core`.', ['labcolors-core']), []);
  assert.match(
    crateMentionErrors('см. `labcolors-ghost`.', ['labcolors-core'])[0],
    /фантом/,
  );
  assert.match(crateMentionErrors('пусто.', ['labcolors-core'])[0], /не упомянут/);
  assert.deepEqual(extractCrateMentions('`labcolors-ffi` и labcolors-wasm без бэктиков'), [
    'labcolors-ffi',
  ]);
});

test('memberResolutionErrors: несуществующий член ловится', () => {
  assert.deepEqual(memberResolutionErrors(['crates/labcolors-core'], ROOT), []);
  assert.match(
    memberResolutionErrors(['crates/no-such-crate'], ROOT)[0],
    /не разрешается в ФС/,
  );
});

/* ---------------- гейт: субпути ---------------- */

test('subpathMentionErrors: фантом и неупомянутый субпуть', () => {
  const subpaths = ['.', './apply-theme'];
  assert.deepEqual(subpathMentionErrors('есть `./apply-theme`.', subpaths), []);
  assert.match(subpathMentionErrors('есть `./ghost`.', subpaths)[0], /фантом/);
  assert.match(subpathMentionErrors('пусто.', subpaths)[0], /не упомянут/);
  assert.deepEqual(extractSubpathMentions('`./a-b` и `./pkg/x_bg.wasm`'), [
    './a-b',
    './pkg/x_bg.wasm',
  ]);
});

test('subpathLawErrors: kebab и package.json законны, артефакт — только через отступления', () => {
  const subpaths = ['.', './package.json', './apply-theme', './pkg/labcolors_bg.wasm'];
  assert.deepEqual(
    subpathLawErrors(subpaths, 'зафиксировано: `./pkg/labcolors_bg.wasm` — артефакт.'),
    [],
  );
  assert.match(subpathLawErrors(subpaths, 'пусто')[0], /вне kebab-закона/);
});

/* ---------------- гейт: python и отступления ---------------- */

test('pyScriptErrors: двусторонняя сверка скриптов', () => {
  assert.deepEqual(pyScriptErrors('`golden_ref.py`', ['golden_ref.py']), []);
  assert.match(pyScriptErrors('`ghost_ref.py`', ['golden_ref.py'])[0], /фантом/);
  assert.match(pyScriptErrors('пусто', ['golden_ref.py'])[0], /не упомянут/);
  assert.deepEqual(extractPyMentions('`a_b.py` и c.py без бэктиков'), ['a_b.py']);
});

test('deviationsSection выделяет раздел до следующего ##', () => {
  const text = '# T\n\n## Известные отступления\n\n- `x/y.txt` — причина.\n\n## Дальше\n';
  assert.match(deviationsSection(text), /x\/y\.txt/);
  assert.ok(!deviationsSection(text).includes('Дальше'));
  assert.equal(deviationsSection('# T\nбез раздела\n'), '');
});

test('fileLawErrors: незафиксированный файл и осиротевшая запись', () => {
  const inv = { nonLaw: ['packages/colors/bench/AFTER.txt'], subpaths: [] };
  assert.deepEqual(
    fileLawErrors('- `packages/colors/bench/AFTER.txt` — слепок.', inv),
    [],
  );
  assert.match(fileLawErrors('', inv)[0], /не зафиксирован/);
  assert.match(
    fileLawErrors('- `packages/colors/bench/GONE.txt` — нет такого.', inv)[1] ??
      fileLawErrors('- `packages/colors/bench/GONE.txt` — нет такого.', inv)[0],
    /больше нет/,
  );
});

/* ---------------- интеграция: живой репо зелёный ---------------- */

test('auditRepo на текущем дереве: ноль ошибок (дока == реальность)', () => {
  const { errors } = auditRepo(ROOT);
  assert.deepEqual(errors, []);
});

test('collectInventory детерминирован (двойной прогон байт-в-байт)', () => {
  assert.deepEqual(collectInventory(ROOT), collectInventory(ROOT));
});
