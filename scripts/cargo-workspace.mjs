const WORKSPACE_PACKAGE_HEADER =
  /^[ \t]*\[workspace\.package\][ \t]*(?:#.*)?\r?$/mu;
const ANY_TABLE_HEADER =
  /^[ \t]*(?:\[[^\[\]\r\n]+\]|\[\[[^\[\]\r\n]+\]\])[ \t]*(?:#.*)?\r?$/mu;
const VERSION_ENTRY = /^[ \t]*version[ \t]*=[ \t]*"([^"\r\n]+)"[ \t]*(?:#.*)?\r?$/mu;

function assertNoMultilineStrings(source) {
  let state = "code";
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    if (state === "comment") {
      if (char === "\n") state = "code";
      continue;
    }
    if (state === "basic") {
      if (char === "\\") index += 1;
      else if (char === '"') state = "code";
      else if (char === "\n") state = "code";
      continue;
    }
    if (state === "literal") {
      if (char === "'") state = "code";
      else if (char === "\n") state = "code";
      continue;
    }
    if (char === "#") {
      state = "comment";
    } else if (char === '"') {
      if (source.startsWith('"""', index)) {
        throw new Error("multiline TOML strings are unsupported by the workspace parser");
      }
      state = "basic";
    } else if (char === "'") {
      if (source.startsWith("'''", index)) {
        throw new Error("multiline TOML strings are unsupported by the workspace parser");
      }
      state = "literal";
    }
  }
}

/** Изолирует workspace metadata от последующих TOML-таблиц.
 * Инвариант: возвращённый диапазон не содержит другую таблицу. */
function packageTable(cargoSource, headerPattern, label) {
  assertNoMultilineStrings(cargoSource);
  const header = headerPattern.exec(cargoSource);
  if (header) {
    const remainder = cargoSource.slice(header.index + header[0].length);
    const nextTable = remainder.search(ANY_TABLE_HEADER);
    return nextTable < 0 ? remainder : remainder.slice(0, nextTable);
  }
  throw new Error(`${label} table is absent`);
}

export function workspacePackageTable(cargoSource) {
  return packageTable(cargoSource, WORKSPACE_PACKAGE_HEADER, "workspace.package");
}

export function packageLicense(cargoSource) {
  const table = packageTable(cargoSource, /^[ \t]*\[package\][ \t]*(?:#.*)?\r?$/mu, "package");
  const entries = [...table.matchAll(/^[ \t]*license[ \t]*=[ \t]*"([^"\r\n]+)"[ \t]*(?:#.*)?\r?$/gmu)];
  if (entries.length !== 1) throw new Error("[package].license requires one literal SPDX expression");
  return entries[0][1];
}

/** Читает release-версию только из `[workspace.package]`.
 * Инвариант: одноимённые ключи последующих таблиц не влияют на результат. */
export function workspaceVersion(cargoSource) {
  const version = VERSION_ENTRY.exec(workspacePackageTable(cargoSource))?.[1];
  if (version) return version;
  throw new Error("[workspace.package].version is absent");
}
