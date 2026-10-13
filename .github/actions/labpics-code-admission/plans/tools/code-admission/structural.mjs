// Базовый run не учитывает inline suppressions. scan может только доказать
// разрешённый соседний случай; его отсутствие сохраняет исходную находку.
export function runStructuralRule(rule, paths, scan, config) {
  const common = [
    "--config", config, "--threads", "2",
    ...["hidden", "dot", "exclude", "global", "parent", "vcs"].flatMap(kind => ["--no-ignore", kind]),
    "--json=compact", "--color", "never",
  ];
  const matches = scan(["run", ...common, "--pattern", rule.pattern, "--lang", rule.language, ...paths]);
  if (!Array.isArray(matches)) throw new TypeError("ast-grep JSON must be an array");
  if (!rule.allow || matches.length === 0) return matches;
  const allowed = scan(["scan", ...common, "--inline-rules", JSON.stringify({
    id: rule.id, language: rule.language, severity: "error", message: rule.message, rule: rule.allow,
  }), ...paths]);
  if (!Array.isArray(allowed)) throw new TypeError("ast-grep allowance JSON must be an array");
  const locations = new Set(allowed.map(location));
  return matches.filter(match => !locations.has(location(match)));
}

function location(match) {
  const start = match?.range?.byteOffset?.start, end = match?.range?.byteOffset?.end;
  if (typeof match?.file !== "string" || !match.file ||
      !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end <= start) {
    throw new TypeError("Structural allowance requires an exact source range");
  }
  return JSON.stringify([match.file, start, end, match.text]);
}
