const WORKSPACE_PACKAGE_HEADER =
  /^[ \t]*\[workspace\.package\][ \t]*(?:#.*)?\r?$/mu;
const ANY_TABLE_HEADER =
  /^[ \t]*(?:\[[^\[\]\r\n]+\]|\[\[[^\[\]\r\n]+\]\])[ \t]*(?:#.*)?\r?$/mu;
const VERSION_ENTRY = /^[ \t]*version[ \t]*=[ \t]*"([^"\r\n]+)"[ \t]*(?:#.*)?\r?$/mu;

/** Читает root version, не пересекая границу TOML-таблицы `[workspace.package]`. */
export function workspaceVersion(cargoSource) {
  const header = WORKSPACE_PACKAGE_HEADER.exec(cargoSource);
  if (header) {
    const remainder = cargoSource.slice(header.index + header[0].length);
    const nextTable = remainder.search(ANY_TABLE_HEADER);
    const workspacePackage = nextTable < 0 ? remainder : remainder.slice(0, nextTable);
    const version = VERSION_ENTRY.exec(workspacePackage)?.[1];
    if (version) return version;
  }
  throw new Error("workspace core version is absent");
}
