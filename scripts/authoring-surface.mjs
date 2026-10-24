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

