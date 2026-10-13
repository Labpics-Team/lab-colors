#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { fileDigest, prepareQualifiedExecutable } from "./identity.mjs";
import { runStructuralRule } from "./structural.mjs";

const MAX_OUTPUT = 8 * 1024 * 1024;
const sha256 = value => createHash("sha256").update(value).digest("hex");
const EXTENSIONS = Object.freeze({ rust: "rs", go: "go", typescript: "ts", javascript: "js" });

const argument = (name) => {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
};

const runPattern = ({ runtime, rule, path, environment, config }) => runStructuralRule(rule, [path], args => {
  const result = runtime.run(args, {
    encoding: "utf8", maxBuffer: MAX_OUTPUT, stdio: ["ignore", "pipe", "pipe"],
    env: environment, cwd: environment.HOME,
  });
  if (result.error) throw new Error(`${rule.id}: ast-grep failed to start: ${result.error.message}`);
  if (![0, 1].includes(result.status)) {
    throw new Error(`${rule.id}: ast-grep exited ${result.status}: ${String(result.stderr).trim().slice(0, 2000)}`);
  }
  try {
    return JSON.parse(result.stdout || "[]");
  } catch (error) {
    throw new Error(`${rule.id}: ast-grep returned invalid JSON: ${error.message}`);
  }
}, config);

const astGrep = argument("--ast-grep");
const rulesPath = argument("--rules");
if (!astGrep || !rulesPath) {
  console.error("usage: qualify-ast-grep.mjs --ast-grep <path> --rules <rules.json>");
  process.exit(2);
}

let root, runtime;
try {
  const binary = resolve(astGrep), rulesFile = resolve(rulesPath);
  const binaryDigest = fileDigest(binary);
  const receipts = [];
  const rulesRaw = readFileSync(rulesFile, "utf8"), rulesDigest = sha256(rulesRaw);
  const rules = JSON.parse(rulesRaw);
  if (!Array.isArray(rules) || rules.length === 0) throw new Error("rules must be a non-empty array");
  root = mkdtempSync(join(tmpdir(), "labpics-ast-qualification-"));
  runtime = prepareQualifiedExecutable(binary, binaryDigest, root, "ast-grep qualification binary");
  const environment = { PATH: process.env.PATH ?? "/usr/local/bin:/usr/bin:/bin", HOME: root, TMPDIR: root, LANG: "C.UTF-8", LC_ALL: "C.UTF-8", NO_COLOR: "1" };
  const config = join(root, "sgconfig.yml");
  writeFileSync(config, "ruleDirs: []\n");
  const ids = new Set();

  for (const rule of rules) {
    const id = String(rule?.id ?? "");
    const language = String(rule?.language ?? "").toLowerCase();
    const extension = EXTENSIONS[language];
    if (!/^lab-[a-z0-9-]+$/.test(id)) throw new Error(`invalid rule id: ${id}`);
    if (ids.has(id)) throw new Error(`duplicate rule id: ${id}`);
    if (!extension) throw new Error(`${id}: unsupported language ${language}`);
    if (typeof rule.pattern !== "string" || rule.pattern.length === 0) throw new Error(`${id}: pattern missing`);
    if (typeof rule.positive !== "string" || rule.positive.length === 0) throw new Error(`${id}: positive contrast missing`);
    if (typeof rule.negative !== "string" || rule.negative.length === 0) throw new Error(`${id}: negative contrast missing`);
    if (rule.positive === rule.negative) throw new Error(`${id}: contrasts must differ`);
    ids.add(id);

    const positivePath = join(root, `${id}.positive.${extension}`);
    const negativePath = join(root, `${id}.negative.${extension}`);
    writeFileSync(positivePath, rule.positive);
    writeFileSync(negativePath, rule.negative);

    const positive = runPattern({ runtime, rule: { ...rule, language }, path: positivePath, environment, config });
    const negative = runPattern({ runtime, rule: { ...rule, language }, path: negativePath, environment, config });
    if (!Array.isArray(positive) || positive.length === 0) {
      throw new Error(`${id}: positive contrast was not detected`);
    }
    if (!Array.isArray(negative) || negative.length !== 0) {
      throw new Error(`${id}: negative contrast was incorrectly detected`);
    }
    const controls = [];
    if (rule.controls !== undefined && !Array.isArray(rule.controls)) throw new Error(`${id}: invalid controls`);
    for (const [index, control] of (rule.controls ?? []).entries()) {
      if (!control || typeof control.name !== "string" || !control.name ||
          typeof control.source !== "string" || !control.source ||
          !Number.isSafeInteger(control.matches) || control.matches < 0) throw new Error(`${id}: invalid control`);
      const path = join(root, `${id}.control-${index}.${extension}`);
      writeFileSync(path, control.source);
      const found = runPattern({ runtime, rule: { ...rule, language }, path, environment, config });
      if (found.length !== control.matches) throw new Error(`${id}: ${control.name}: expected ${control.matches}, got ${found.length}`);
      controls.push({ name: control.name, matches: found.length });
    }
    receipts.push({ ruleId: id, positive: positive.length, negative: 0, ...(controls.length ? { controls } : {}) });
    console.error(`${id}: positive=${positive.length} negative=0`);
  }
  console.log(JSON.stringify({ schema: "labpics.ast-grep/native-qualification/v1", binaryDigest, rulesDigest, rules: receipts }));
} catch (error) {
  console.error(`ast-grep qualification failure: ${error.message}`);
  process.exitCode = 1;
} finally { if (runtime) runtime.close(); if (root) rmSync(root, { recursive: true, force: true }); }
