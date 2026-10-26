import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { authoringSurface, loadAuthoringDocuments } from "./authoring-surface.mjs";
const path = process.argv[2];
if (!path) throw new Error("usage: emit-authoring-surface.mjs OUTPUT");
const documents = await loadAuthoringDocuments(fileURLToPath(new URL("../packages/colors", import.meta.url)));
await writeFile(path, JSON.stringify({schemaVersion: 1, operations: Object.fromEntries(authoringSurface(documents))}));
