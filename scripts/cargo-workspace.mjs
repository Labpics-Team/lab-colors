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
export function workspacePackageTable(cargoSource) {
  assertNoMultilineStrings(cargoSource);
  const header = WORKSPACE_PACKAGE_HEADER.exec(cargoSource);
  if (header) {
    const remainder = cargoSource.slice(header.index + header[0].length);
    const nextTable = remainder.search(ANY_TABLE_HEADER);
    return nextTable < 0 ? remainder : remainder.slice(0, nextTable);
  }
  throw new Error("workspace.package table is absent");
}

/** Читает release-версию только из `[workspace.package]`.
 * Инвариант: одноимённые ключи последующих таблиц не влияют на результат. */
export function workspaceVersion(cargoSource) {
  const version = VERSION_ENTRY.exec(workspacePackageTable(cargoSource))?.[1];
  if (version) return version;
  throw new Error("[workspace.package].version is absent");
}
