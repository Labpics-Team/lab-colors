import { readFile } from "node:fs/promises";
import { posix, resolve } from "node:path";
import ts from "../packages/colors/node_modules/typescript/lib/typescript.js";
const fail = (message) => { throw new Error(message); };

export function authoringSurface(documents) {
  const result = new Map();
  for (const [name, text] of Object.entries(documents)) {
    const source = ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true);
    if (source.parseDiagnostics.length) fail("authoring declaration does not parse: " + name);
    for (const declaration of source.statements) {
      if (!ts.isClassDeclaration(declaration) || !declaration.name?.text.startsWith("ProgramWireBuilder")) continue;
      for (const method of declaration.members) {
        if (!ts.isMethodDeclaration(method) || !ts.isIdentifier(method.name)) continue;
        if (method.modifiers?.some((modifier) => [ts.SyntaxKind.PrivateKeyword, ts.SyntaxKind.ProtectedKeyword].includes(modifier.kind))) continue;
        const key = declaration.name.text + "::" + method.name.text;
        if (result.has(key)) fail("ambiguous authoring operation: " + key);
        result.set(key, method.parameters.length);
      }
    }
  }
  if (result.get("ProgramWireBuilderV1::source") !== 2 || result.get("ProgramWireBuilderV2::selectionRelease") !== 2) {
    fail("canonical authoring roots are missing");
  }
  return result;
}


export function authoringExportPaths(pkg) {
  if (!pkg?.exports || typeof pkg.exports !== "object" || Array.isArray(pkg.exports)) {
    fail("public package exports are missing");
  }
  const paths = new Set();
  for (const [name, entry] of Object.entries(pkg.exports)) {
    if (!name.startsWith("./program-wire/")) continue;
    const path = entry?.types;
    if (typeof path !== "string" || !path.startsWith("./program-wire/") || !path.endsWith(".d.ts") ||
        path.includes("\\") || path.includes("\0") || posix.normalize(path) !== path.slice(2)) {
      fail("public authoring export has no local canonical type declaration: " + name);
    }
    paths.add(path);
  }
  if (paths.size === 0) fail("public authoring exports are missing");
  return [...paths].sort();
}

export async function loadAuthoringDocuments(packageRoot) {
  const pkg = JSON.parse(await readFile(resolve(packageRoot, "package.json"), "utf8"));
  return Object.fromEntries(await Promise.all(authoringExportPaths(pkg).map(async path =>
    [path, await readFile(resolve(packageRoot, path), "utf8")])));
}
