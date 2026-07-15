/**
 * Юнит-тесты гейта docs-drift (`node --test scripts/docs-drift.test.mjs`).
 *
 * Чистые функции гейта проверяются на синтетических строках (позитив +
 * негатив на каждую проверку), сборщик фактов — на реальном дереве репо
 * (детерминированные инварианты: имена крейтов, субпути, законы имён).
 */

import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { test } from 'node:test';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import {
  ROOT,
  collectInventory,
  crateName,
  fileStem,
  lawForFile,
  nonLawFiles,
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
  claimErrorsInText,
  BARE_MATERIAL_89,
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

test('generated WASM names come from package files without hiding undeclared source', () => {
  const root = mkdtempSync(join(tmpdir(), 'labcolors-naming-generated-'));
  try {
    const generated = join(root, 'packages', 'colors', 'compiler');
    mkdirSync(generated, { recursive: true });
    writeFileSync(
      join(root, 'packages', 'colors', 'package.json'),
      JSON.stringify({
        files: [
          'compiler/labcolors_compiler.js',
          'compiler/labcolors_compiler_bg.wasm',
        ],
      }),
    );
    writeFileSync(join(generated, 'labcolors_compiler.js'), 'generated');
    writeFileSync(join(generated, 'labcolors_compiler_bg.wasm'), 'generated');
    writeFileSync(join(generated, 'hand_written_bad.js'), 'source');
    assert.deepEqual(nonLawFiles(root), [
      'packages/colors/compiler/hand_written_bad.js',
      'packages/colors/compiler/labcolors_compiler.js',
      'packages/colors/compiler/labcolors_compiler_bg.wasm',
    ]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
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

test('empirical residue: каждая живая policy-константа имеет terminal provenance class', () => {
  const text = readFileSync(join(ROOT, 'docs', 'empirical-residue.md'), 'utf8');
  const inventory = readFileSync(join(ROOT, 'docs', 'empirical-inventory.md'), 'utf8');
  const table = text
    .split('## Классификация текущих 25 policy-констант')[1]
    ?.split('\n**(b) после follow-on')[0];
  assert.ok(table, 'таблица текущей terminal-классификации должна существовать');

  const rows = table
    .split('\n')
    .filter((line) => /^\|\s*(?:\*\*)?(?:~~)?\d/.test(line))
    .filter((line) => !line.includes('~~20~~'));
  assert.equal(rows.length, 25, 'anti-vacuum: ожидаются ровно 25 живых policy-констант');

  const counts = new Map([
    ['a', 0],
    ['b', 0],
    ['c', 0],
    ['e', 0],
  ]);
  for (const row of rows) {
    const cells = row.split('|').slice(1, -1).map((cell) => cell.trim());
    const terminal = cells[3]?.match(/\(([abce])\)/)?.[1];
    assert.ok(
      terminal,
      `строка ${cells[0]} ${cells[1]} смешивает scientific status с terminal class: ${cells[3]}`,
    );
    counts.set(terminal, counts.get(terminal) + 1);
  }
  assert.deepEqual(Object.fromEntries(counts), { a: 6, b: 1, c: 7, e: 11 });

  for (const gamma of ['NEUTRAL_DEFAULT_GAMMA_LIGHT', 'NEUTRAL_DEFAULT_GAMMA_DARK']) {
    const row = rows.find((line) => line.includes(`\`${gamma}\``));
    assert.ok(row, `${gamma} должен присутствовать в таблице`);
    assert.match(row, /\*\*\(e\)\*\*/);
    assert.match(
      row,
      /НАУЧНАЯ ЗАМЕНА: OPEN/,
      `${gamma}: frozen compatibility provenance и open scientific replacement — разные поля`,
    );

    const inventoryRow = inventory
      .split('\n')
      .find((line) => line.startsWith('|') && line.includes(`\`${gamma}\``));
    assert.ok(inventoryRow, `${gamma} должен присутствовать в основном инвентаре`);
    assert.match(inventoryRow, /\*\*\(e\) DESIGN-CHOICE \/ COMPATIBILITY POLICY\*\*/);
    assert.match(
      inventoryRow,
      /НАУЧНАЯ ЗАМЕНА: OPEN/,
      `${gamma}: основной инвентарь обязан разделять terminal provenance и scientific status`,
    );
  }
});

test('issue-ссылки не превращаются в ATX headings ни в одном docs markdown', () => {
  const malformed = collectInventory(ROOT).docsMd.flatMap((path) =>
    readFileSync(join(ROOT, 'docs', path), 'utf8')
      .split('\n')
      .map((line, index) => ({ path, line, number: index + 1 }))
      .filter(({ line }) => /^#\d/.test(line)),
  );
  assert.deepEqual(malformed, []);
});

test('breaking exact-alpha/glow контракт имеет migration и не оставляет единый live profile', () => {
  const migration = readFileSync(
    join(ROOT, 'docs', 'migrations', 'exact-alpha-glow.md'),
    'utf8',
  );
  const changelog = readFileSync(join(ROOT, 'CHANGELOG.md'), 'utf8');
  const readme = readFileSync(join(ROOT, 'packages', 'colors', 'README.md'), 'utf8');
  const adr = readFileSync(
    join(ROOT, 'docs', 'decisions', '0004-finite-alpha-glow-reference.md'),
    'utf8',
  );

  for (const required of [
    '@labpics/colors` 0.10.0',
    'Rust workspace | 0.1.0 | 0.2.0',
    'decision_profile',
    'stable-v1',
    'legacy-platform-dependent-v1',
    'glow-indeterminate',
    'sound-bound-unavailable',
    'compositeProfile',
    'layerRecipeProfile',
    'appearanceDiagnosticProfile',
    'selectionDiagnosticProfile',
    'resolve_alpha_analog_hex',
    'dedicated module Worker',
    'Один и тот же модуль для',
    'Rollback',
  ]) {
    assert.ok(migration.includes(required), `migration не содержит ${required}`);
  }
  assert.match(changelog, /@labpics\/colors 0\.10\.0 \/ Rust 0\.2\.0/);
  assert.doesNotMatch(readme, /referenceProfile/);
  assert.doesNotMatch(adr, /referenceProfile/);
  assert.doesNotMatch(readme, /\bdiagnosticProfile\b/);
  assert.doesNotMatch(adr, /\bdiagnosticProfile\b/);
  assert.match(
    readme,
    /new Worker\(new URL\("\.\/color-compiler\.worker\.ts"[\s\S]*?worker\.terminate\(\)/,
  );
  assert.doesNotMatch(migration, /Promise\.all\(\[initRuntime\(\), initCompiler\(\)\]\)/);
});

test('exact-alpha migration экранирует absolute-value pipes внутри Markdown tables', () => {
  const migration = readFileSync(
    join(ROOT, 'docs', 'migrations', 'exact-alpha-glow.md'),
    'utf8',
  );
  const tableRows = migration.split('\n').filter((line) => line.startsWith('|'));
  const malformed = tableRows.filter((line) => /`[^`]*\|ΔJ′\|[^`]*`/.test(line));
  assert.deepEqual(malformed, []);
  assert.equal(
    tableRows.filter((line) => line.includes('`\\|ΔJ′\\|`')).length,
    3,
    'anti-vacuum: determinate Glow table содержит три |ΔJ′| cells',
  );
});

test('legacy muddiness API quarantined as compatibility proxy, not human verdict', () => {
  const paths = [
    ['core', 'crates/labcolors-core/src/cleanliness.rs'],
    ['wasm', 'crates/labcolors-wasm/src/lib.rs'],
    ['ffi', 'crates/labcolors-ffi/src/lib.rs'],
    ['conformance docs', 'conformance/README.md'],
    ['conformance crate', 'crates/labcolors-conformance/src/lib.rs'],
    ['npm docs', 'packages/colors/README.md'],
    ['swift docs', 'bindings/swift/README.md'],
  ];
  const surfaces = paths.map(([label, path]) => [
    label,
    readFileSync(join(ROOT, ...path.split('/')), 'utf8'),
  ]);

  for (const [label, source] of surfaces) {
    assert.match(
      source,
      /experimental compatibility proxy/i,
      `${label}: legacy API must state its non-observer status`,
    );
  }

  const publicText = surfaces.map(([, source]) => source).join('\n');
  for (const forbidden of [
    /Закон Грязи/u,
    /Muddiness Law/u,
    /0\s*[—-]\s*чистый,\s*1\s*[—-]\s*грязный/u,
    /оценка [«"]грязи[»"]/u,
  ]) {
    assert.doesNotMatch(publicText, forbidden);
  }

  const inventory = readFileSync(join(ROOT, 'docs', 'empirical-inventory.md'), 'utf8');
  assert.match(inventory, /M-01[^\n]*Indeterminate provenance/u);
  assert.match(inventory, /M-02[^\n]*universal JND Rejected/u);
  assert.match(inventory, /M-04[^\n]*Rejected provenance; Indeterminate value/u);
  assert.match(inventory, /M-05[^\n]*Rejected provenance; Indeterminate value/u);
  assert.match(inventory, /M-12[^\n]*observer claim Rejected/u);
  assert.doesNotMatch(inventory, /M-0[45][^\n]*\| cited-measured \|/u);
});

/* Клейм-гигиена (#306) */
test('голый #89 ловится, hex-цвета — нет', () => {
  assert.equal(BARE_MATERIAL_89.test('Материал (#89): контракт'), true);
  assert.equal(BARE_MATERIAL_89.test('цвет #89CFF0 и #8944AB'), false);
  assert.equal(BARE_MATERIAL_89.test('п套 #189 и #890'), false);
  const errs = claimErrorsInText('x.rs', 'см. #89.');
  assert.equal(errs.length, 1);
});

test('whole-effect, labui-material.css и platform-characterized ловятся', () => {
  assert.equal(claimErrorsInText('a.rs', 'профиль полного результата Glow').length, 1);
  // Вне Glow-контекста обычная фраза не флагается (контекстное правило).
  assert.equal(claimErrorsInText('a2.rs', 'сводка полного результата аудита').length, 0);
  assert.equal(claimErrorsInText('b.rs', 'потребляет labui-material.css').length, 1);
  assert.equal(claimErrorsInText('c.rs', 'remains platform-characterized').length, 1);
  assert.equal(claimErrorsInText('c2.rs', 'Platform-characterized: powf').length, 1);
  // Типовое имя (CamelCase, deferred) — НЕ нарушение.
  assert.equal(claimErrorsInText('d.rs', 'PlatformCharacterized непредставим').length, 0);
  assert.equal(claimErrorsInText('e.rs', 'изолированные point-слои').length, 0);
});
