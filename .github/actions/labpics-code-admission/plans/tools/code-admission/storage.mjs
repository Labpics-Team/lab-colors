import { createHash } from 'node:crypto';
import { relative, sep } from 'node:path';

const UNKNOWN = 3, FALSE = 1, TRUE = 2;
const digest = text => createHash('sha256').update(text).digest('hex');
const values = mask => [false, true].filter(value => mask & (value ? TRUE : FALSE));
const combine = (left, right, operation) => values(left).flatMap(a => values(right).map(b => operation(a, b)))
  .reduce((mask, value) => mask | (value ? TRUE : FALSE), 0);

// Неизвестное условие может сработать. Разрешение требует явного ограничения
// эффекта, а не удачного примера события или значения пользовательского input.
export function mayRetainOnSuccess(condition) {
  if (condition === undefined) return true;
  let text = String(condition).trim();
  if (text.startsWith('${{') && text.endsWith('}}')) text = text.slice(3, -2).trim();
  function evaluate(value) {
    value = value.trim();
    let depth = 0, quote = false;
    const splits = { '||': [], '&&': [] };
    for (let index = 0; index < value.length; index++) {
      const char = value[index];
      if (char === "'") {
        if (quote && value[index + 1] === "'") { index++; continue; }
        quote = !quote;
      }
      if (quote) continue;
      if (char === '(') depth++;
      if (char === ')') depth--;
      if (depth < 0) return UNKNOWN;
      const operator = value.slice(index, index + 2);
      if (depth === 0 && Object.hasOwn(splits, operator)) { splits[operator].push(index); index++; }
    }
    if (quote || depth !== 0) return UNKNOWN;
    for (const operator of ['||', '&&']) {
      if (splits[operator].length) {
        const index = splits[operator].at(-1);
        return combine(evaluate(value.slice(0, index)), evaluate(value.slice(index + 2)),
          operator === '||' ? ((a, b) => a || b) : ((a, b) => a && b));
      }
    }
    if (value.startsWith('(') && value.endsWith(')')) return evaluate(value.slice(1, -1));
    if (value.startsWith('!') && !value.startsWith('!=')) {
      return values(evaluate(value.slice(1))).reduce((mask, item) => mask | (item ? FALSE : TRUE), 0);
    }
    if (/^(false|failure\s*\(\s*\)|cancelled\s*\(\s*\))$/i.test(value)) return FALSE;
    if (/^(true|success\s*\(\s*\)|always\s*\(\s*\))$/i.test(value)) return TRUE;
    if (/^github\.event_name\s*==\s*'(workflow_dispatch|release)'$/i.test(value)
      || /^'(workflow_dispatch|release)'\s*==\s*github\.event_name$/i.test(value)
      || /^github\.ref_type\s*==\s*'tag'$/i.test(value)
      || /^'tag'\s*==\s*github\.ref_type$/i.test(value)
      || /^startsWith\s*\(\s*github\.ref\s*,\s*'refs\/tags\/'\s*\)$/i.test(value)) return FALSE;
    return UNKNOWN;
  }
  return Boolean(evaluate(text) & TRUE);
}

function scalar(text) {
  text = text.trim();
  if (/^[>|][-+]?\s*(?:#.*)?\n/.test(text)) {
    return text.slice(text.indexOf('\n') + 1).split('\n').map(line => line.trim()).join(' ').trim();
  }
  if (text.startsWith("'")) {
    const quoted = /^'((?:[^']|'')*)'\s*(?:#.*)?$/u.exec(text);
    return quoted ? quoted[1].replaceAll("''", "'") : null;
  }
  if (text.startsWith('"')) {
    const quoted = /^("(?:\\.|[^"\\])*")\s*(?:#.*)?$/u.exec(text);
    try { return quoted ? JSON.parse(quoted[1]) : null; } catch { return null; }
  }
  if (/^[&*!]/.test(text) || text.includes('\n')) return null;
  return text.replace(/\s+#.*$/, '').trim();
}
function pair(match) {
  const found = /^("[^"\n]+"|'[^'\n]+'|[^:\s]+)\s*:\s*([\s\S]*)$/.exec(match.text);
  if (!found) return null;
  return { ...match, key: scalar(found[1]), value: scalar(found[2]) };
}
function range(match) {
  const value = match.range?.byteOffset;
  if (!value || !Number.isSafeInteger(value.start) || !Number.isSafeInteger(value.end) || value.end <= value.start) {
    throw new TypeError('YAML parser omitted a source range');
  }
  return value;
}
const contains = (outer, inner) => outer.start <= inner.start && outer.end >= inner.end;
const width = item => range(item).end - range(item).start;

export const isStorageSource = path => /^\.github\/workflows\/[^/]+\.ya?ml$/.test(path)
  || /(?:^|\/)action\.ya?ml$/.test(path);

export function storageFindings(root, paths, scan, config) {
  if (!paths.length) return [];
  const common = ['--config', config, '--threads', '2', '--json=compact', '--color', 'never',
    ...['hidden', 'dot', 'exclude', 'global', 'parent', 'vcs'].flatMap(kind => ['--no-ignore', kind])];
  const read = kind => {
    const items = scan(['run', ...common, '--lang', 'yaml', '--kind', kind, ...paths]);
    if (!Array.isArray(items)) throw new TypeError('YAML parser returned an incomplete result');
    for (const item of items) range(item);
    return items;
  };
  const mappings = [...read('block_mapping'), ...read('flow_mapping')];
  const pairs = [...read('block_mapping_pair'), ...read('flow_pair')].map(pair).filter(Boolean);
  const findings = [];
  const failure = (item, message, context = []) => {
    const path = relative(root, item.file).split(sep).join('/');
    if (!path || path.startsWith('../')) throw new TypeError('YAML source escaped the snapshot');
    findings.push({ fingerprint: `storage:${path}:${digest(JSON.stringify([message, item.text, ...context]))}`, source: 'policy',
      rule: 'lab-ci-retained-output', path, line: item.range.start.line + 1, message });
  };
  for (const item of read('ERROR')) failure(item, 'Неоднозначный YAML не определяет разрешённый путь хранения CI.');
  const parents = item => mappings.filter(parent => parent.file === item.file && contains(range(parent), range(item)))
    .sort((a, b) => width(a) - width(b));
  const fields = mapping => pairs.filter(item => item.file === mapping.file && parents(item)[0] === mapping);
  const childMapping = item => mappings.filter(mapping => mapping.file === item.file && contains(range(item), range(mapping)))
    .sort((a, b) => width(b) - width(a))[0];
  const actionPairs = pairs.filter(item => item.key === 'uses');
  for (const item of actionPairs.filter(item => item.value === null)) {
    failure(item, 'Исполняемое действие задаётся явно; ссылка или расширение YAML скрывает эффект хранения.');
  }
  const effects = actionPairs.filter(item => /^(actions\/upload-artifact|docker\/build-push-action|docker\/bake-action)(?:@|$)/i.test(item.value ?? ''));
  // YAML aliases inside step sequences can hide the action itself or its
  // storage inputs. Aliases in unrelated job/root data do not change the step.
  const stepFields = pairs.filter(item => item.key === 'steps');
  for (const alias of read('alias')) {
    if (stepFields.some(item => item.file === alias.file && contains(range(item), range(alias)))) {
      failure(alias, 'Шаг CI с YAML alias скрывает исполняемое действие или его настройки хранения.');
    }
  }
  for (const use of effects) {
    const ancestors = parents(use);
    if (!ancestors.length) { failure(use, 'Хранение CI требует явного задания и условия.'); continue; }
    for (const mapping of ancestors) {
      const names = fields(mapping).map(item => item.key);
      if (names.length !== new Set(names).size) failure(mapping, 'Повторный ключ YAML делает условия хранения неоднозначными.');
    }
    const step = ancestors[0];
    const affectingFields = ancestors.flatMap(mapping =>
      fields(mapping).filter(item => item.key === 'if' || item.key === 'env'))
      .map(item => item.text);
    const effectFailure = (item, message) =>
      failure(item, message, [step.text, ...affectingFields]);
    if (/^docker\//i.test(use.value)) {
      const scopes = ancestors.map(mapping => fields(mapping).find(item => item.key === 'env'))
        .filter(Boolean).map(item => childMapping(item));
      if (scopes.some(mapping => !mapping)) effectFailure(ancestors[0], 'Окружение producer должно быть явным YAML mapping.');
      for (const mapping of scopes.filter(Boolean)) {
        const names = fields(mapping).map(item => item.key);
        if (names.length !== new Set(names).size) failure(mapping, 'Повторный env делает хранение неоднозначным.');
      }
      const configuration = scopes.filter(Boolean).flatMap(mapping => fields(mapping))
        .find(item => item.key === 'DOCKER_BUILD_RECORD_UPLOAD');
      if (String(configuration?.value ?? '').toLowerCase() !== 'false') {
        effectFailure(ancestors[0], 'Образ и summary сохраняются; автоматическая копия Docker build record должна быть отключена через DOCKER_BUILD_RECORD_UPLOAD=false.');
      }
      continue;
    }
    const conditions = ancestors.flatMap(mapping => fields(mapping).filter(item => item.key === 'if'));
    const withPair = fields(step).find(item => item.key === 'with');
    const withMapping = withPair && childMapping(withPair);
    if (withMapping) {
      const names = fields(withMapping).map(item => item.key);
      if (names.length !== new Set(names).size) failure(withMapping, 'Параметры хранения не допускают повторных ключей YAML.');
    }
    const retention = withMapping && fields(withMapping).find(item => item.key === 'retention-days');
    if (conditions.every(item => item.value === null || mayRetainOnSuccess(item.value))) {
      effectFailure(step, 'Успешный обычный CI хранит результат в log/summary и передаёт малые значения через outputs. Архив нужен только для отказа, явного экспорта или выпуска версии.');
    } else if (retention?.value !== '1') {
      effectFailure(step, 'Actions хранит временный результат один день; долговечный результат принадлежит выпуску или исследованию.');
    }
  }
  return findings;
}
