import { readFile, writeFile } from "node:fs/promises";
import { authoringSurface } from "./authoring-surface.mjs";
const path = process.argv[2];
if (!path) throw new Error("usage: emit-authoring-surface.mjs OUTPUT");
const documents = Object.fromEntries(await Promise.all(["abi-v1.d.ts", "abi-v2.d.ts"].map(async name =>
  [name, await readFile(new URL("../packages/colors/program-wire/" + name, import.meta.url), "utf8")])));
await writeFile(path, JSON.stringify({schemaVersion: 1, operations: Object.fromEntries(authoringSurface(documents))}));
