import { copyFileSync, cpSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = fileURLToPath(new URL("../../", import.meta.url));

// Состав фасада задаёт package.files: тесты не хранят второй список модулей.
// Подменяется только generated binding; исходные модули остаются byte-exact.
export function copyRuntimeFixture(destination, { generatedBinding } = {}) {
  const manifest = readFileSync(join(packageRoot, "package.json"));
  mkdirSync(destination, { recursive: true });
  writeFileSync(join(destination, "package.json"), manifest);
  for (const path of JSON.parse(manifest).files) {
    if (path.startsWith("pkg/") || !/\.m?js$/u.test(path)) continue;
    const target = join(destination, path);
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(join(packageRoot, path), target);
  }
  const generated = join(destination, "pkg");
  if (generatedBinding === undefined) {
    cpSync(join(packageRoot, "pkg"), generated, { recursive: true });
  } else {
    mkdirSync(generated, { recursive: true });
    writeFileSync(join(generated, "labcolors.js"), generatedBinding);
  }
}
