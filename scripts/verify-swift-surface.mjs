import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "../packages/colors/node_modules/typescript/lib/typescript.js";

const camel = (name) => name.replace(/_([a-z0-9])/gu, (_, letter) => letter.toUpperCase());
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

function compiledSurface(graph) {
  if (graph?.module?.name !== "LabColors" || !Array.isArray(graph.symbols) || !Array.isArray(graph.relationships)) {
    fail("expected the compiled LabColors symbol graph");
  }
  const classes = new Map(), methods = new Map(), parents = new Map();
  for (const symbol of graph.symbols) {
    if (!["public", "open"].includes(symbol.accessLevel)) continue;
    const path = symbol.pathComponents;
    if (!Array.isArray(path) || path.length === 0) fail("public symbol has no qualified name");
    if (symbol.kind?.identifier === "swift.class") classes.set(symbol.identifier.precise, path.at(-1));
    if (!["swift.func", "swift.method"].includes(symbol.kind?.identifier)) continue;
    const signature = symbol.functionSignature;
    if (!signature || typeof signature !== "object" || Array.isArray(signature)) {
      fail("compiled operation has no signature: " + path.join("."));
    }
    // SymbolKit omits an empty parameter list. Missing non-empty signatures
    // still fail: the compiler's qualified name must describe the same arity.
    const parameters = signature.parameters === undefined ? [] : signature.parameters;
    if (!Array.isArray(parameters) ||
        (signature.parameters === undefined && !path.at(-1).endsWith("()"))) {
      fail("compiled operation has an invalid parameter list: " + path.join("."));
    }
    const owner = path.length === 1 ? "fn" : path.at(-2);
    const method = path.at(-1).split("(", 1)[0];
    const key = owner + "::" + method;
    if (!methods.has(key)) methods.set(key, new Set());
    methods.get(key).add(parameters.length);
  }
  for (const relation of graph.relationships) {
    if (relation.kind !== "inheritsFrom") continue;
    const source = classes.get(relation.source), target = classes.get(relation.target);
    if (source && target) {
      if (parents.has(source) && parents.get(source) !== target) fail("ambiguous inheritance: " + source);
      parents.set(source, target);
    }
  }
  function has(owner, method, arity, visited = new Set()) {
    if (visited.has(owner)) fail("cyclic compiled inheritance");
    visited.add(owner);
    if (methods.get(owner + "::" + method)?.has(arity)) return true;
    return parents.has(owner) && has(parents.get(owner), method, arity, visited);
  }
  return { has };
}

export function verifySwiftSurface(expected, documents, graph) {
  if (expected?.schemaVersion !== 1 || !expected.operations || Array.isArray(expected.operations) ||
      expected.operations["fn::compile_program_wire"] !== 2 || expected.operations["ProgramRuntime::update_observed"] !== 4) {
    fail("native operation inventory is absent or incomplete");
  }
  const required = new Map();
  for (const [key, arity] of Object.entries(expected.operations)) {
    if (!/^(?:fn|[A-Za-z][A-Za-z0-9]*)::[a-z][a-z0-9_]*$/u.test(key) || !Number.isSafeInteger(arity) || arity < 0) {
      fail("invalid native operation descriptor");
    }
    const [owner, name] = key.split("::");
    const translated = owner + "::" + camel(name);
    if (required.has(translated)) fail("ambiguous Swift operation name: " + translated);
    required.set(translated, arity);
  }
  for (const [key, arity] of authoringSurface(documents)) required.set(key, arity);
  const compiled = compiledSurface(graph), missing = [];
  for (const [key, arity] of required) {
    const [owner, method] = key.split("::");
    if (!compiled.has(owner, method, arity)) missing.push(key + "/" + arity);
  }
  if (missing.length) fail("compiled Swift API is missing operations: " + missing.join(", "));
  return required.size;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [inventoryPath, ...graphs] = process.argv.slice(2);
  if (!inventoryPath || graphs.length === 0) fail("usage: verify-swift-surface.mjs INVENTORY SYMBOL_GRAPH...");
  const root = fileURLToPath(new URL("..", import.meta.url));
  const documents = Object.fromEntries(await Promise.all(["abi-v1.d.ts", "abi-v2.d.ts"].map(async (name) =>
    [name, await readFile(resolve(root, "packages/colors/program-wire", name), "utf8")])));
  const inventory = JSON.parse(await readFile(inventoryPath, "utf8"));
  for (const path of graphs) {
    const count = verifySwiftSurface(inventory, documents, JSON.parse(await readFile(path, "utf8")));
    console.log(`Swift compiled surface: ${count} operations, ${path}`);
  }
}
