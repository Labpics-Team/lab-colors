import { constants } from "node:fs";
import { lstat, open, opendir, realpath } from "node:fs/promises";
import { createRequire } from "node:module";
import { basename, dirname, posix, relative, resolve, sep } from "node:path";

const packageRequire = createRequire(new URL("../packages/colors/package.json", import.meta.url));

const SNIPPET_PATH_PATTERN = /^snippets\/labcolors-wasm-[0-9a-f]{16}\/inline0\.js$/u;
export const MAX_SNIPPET_DIRECTORIES = 4;
export const MAX_SNIPPET_FILES = 8;
export const MAX_SNIPPET_BYTES = 16 * 1024;
export const MAX_TOTAL_SNIPPET_BYTES = 32 * 1024;

const DEFAULT_IO = Object.freeze({ lstat, open, opendir, realpath });

function containedPath(parent, candidate, label) {
  const path = relative(parent, candidate);
  if (path === "" || path === ".." || path.startsWith(`..${sep}`) || resolve(parent, path) !== candidate) {
    throw new Error(`${label} resolves outside its canonical parent`);
  }
}

function sameFile(left, right) {
  return (
    left.dev === right.dev &&
    left.ino === right.ino &&
    left.nlink === 1n &&
    right.nlink === 1n &&
    left.size === right.size &&
    left.mtimeNs === right.mtimeNs &&
    left.ctimeNs === right.ctimeNs
  );
}

async function moduleSpecifiers(source) {
  const { init, parse } = packageRequire("es-module-lexer");
  await init;
  let imports;
  try {
    [imports] = parse(source);
  } catch (error) {
    throw new Error("generated module is not valid ECMAScript", { cause: error });
  }
  return imports.flatMap((record) => {
    if (record.d === -2 && record.n === undefined) return [];
    if (record.d !== -1 || record.n === undefined) {
      throw new Error("generated module contains a dynamic or non-literal import");
    }
    return [record.n];
  });
}

export async function runtimeSnippetPaths(runtimeSource) {
  const specifiers = await moduleSpecifiers(runtimeSource);
  if (specifiers.length === 0) return [];
  if (specifiers.length !== 1 || !specifiers[0].startsWith("./")) {
    throw new Error("generated runtime imports more than one module or a non-relative module");
  }
  const path = specifiers[0].slice(2);
  if (!SNIPPET_PATH_PATTERN.test(path)) {
    throw new Error(`generated runtime has unsupported relative module specifier: ${specifiers[0]}`);
  }
  return [path];
}

async function canonicalDirectory(path, parent, label, io) {
  const metadata = await io.lstat(path, { bigint: true });
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error(`${label} is not a canonical directory`);
  }
  const canonical = await io.realpath(path);
  containedPath(parent, canonical, label);
  if (resolve(canonical) !== resolve(path)) {
    throw new Error(`${label} canonical path differs from its lexical path`);
  }
  return canonical;
}

async function generatedSnippetFiles(packageDirectory, io) {
  const lexicalPackage = resolve(packageDirectory);
  // Родитель может быть системным alias (/var на macOS), но сам пакет — не ссылкой.
  const packagePath = resolve(await io.realpath(dirname(lexicalPackage)), basename(lexicalPackage));
  const packageMetadata = await io.lstat(packagePath, { bigint: true });
  if (!packageMetadata.isDirectory() || packageMetadata.isSymbolicLink()) {
    throw new Error("package directory is not canonical");
  }
  const canonicalPackage = await io.realpath(packagePath);
  if (resolve(canonicalPackage) !== packagePath) {
    throw new Error("package directory canonical path differs from its lexical path");
  }
  const pkgPath = resolve(canonicalPackage, "pkg");
  const canonicalPkg = await canonicalDirectory(pkgPath, canonicalPackage, "pkg", io);
  const snippetsPath = resolve(pkgPath, "snippets");
  let snippetsMetadata;
  try {
    snippetsMetadata = await io.lstat(snippetsPath);
  } catch (error) {
    if (error?.code === "ENOENT") return { files: [], snippetsPath };
    throw error;
  }
  if (!snippetsMetadata.isDirectory() || snippetsMetadata.isSymbolicLink()) {
    throw new Error("snippets root is not a canonical directory");
  }
  const canonicalSnippets = await io.realpath(snippetsPath);
  containedPath(canonicalPkg, canonicalSnippets, "snippets root");

  const files = [];
  let directories = 0;
  let totalBytes = 0n;
  const snippets = await io.opendir(snippetsPath);
  for await (const directoryEntry of snippets) {
    directories += 1;
    if (directories > MAX_SNIPPET_DIRECTORIES) {
      throw new Error(`generated snippet directory count exceeds ${MAX_SNIPPET_DIRECTORIES}`);
    }
    if (!directoryEntry.isDirectory() || !/^labcolors-wasm-[0-9a-f]{16}$/u.test(directoryEntry.name)) {
      throw new Error(`unexpected generated snippet entry: snippets/${directoryEntry.name}`);
    }
    const directory = resolve(snippetsPath, directoryEntry.name);
    await canonicalDirectory(directory, canonicalSnippets, `snippet directory ${directoryEntry.name}`, io);
    const children = await io.opendir(directory);
    for await (const fileEntry of children) {
      if (files.length >= MAX_SNIPPET_FILES) {
        throw new Error(`generated snippet file count exceeds ${MAX_SNIPPET_FILES}`);
      }
      const relativePath = posix.join("snippets", directoryEntry.name, fileEntry.name);
      if (!fileEntry.isFile() || !SNIPPET_PATH_PATTERN.test(relativePath)) {
        throw new Error(`unexpected generated snippet entry: ${relativePath}`);
      }
      const path = resolve(directory, fileEntry.name);
      const metadata = await io.lstat(path, { bigint: true });
      if (!metadata.isFile() || metadata.isSymbolicLink()) {
        throw new Error(`generated snippet is not a regular file: ${relativePath}`);
      }
      const canonicalFile = await io.realpath(path);
      containedPath(await io.realpath(directory), canonicalFile, relativePath);
      if (metadata.size === 0n || metadata.size > BigInt(MAX_SNIPPET_BYTES)) {
        throw new Error(`generated snippet has invalid byte size: ${relativePath}`);
      }
      totalBytes += metadata.size;
      if (totalBytes > BigInt(MAX_TOTAL_SNIPPET_BYTES)) {
        throw new Error(`generated snippet bytes exceed ${MAX_TOTAL_SNIPPET_BYTES}`);
      }
      files.push({ relativePath, path, metadata, canonicalFile });
    }
  }
  return { files, snippetsPath };
}

async function readStableSnippet(file, directory, io) {
  const noFollow = typeof constants.O_NOFOLLOW === "number" ? constants.O_NOFOLLOW : 0;
  let handle;
  try {
    handle = await io.open(file.path, constants.O_RDONLY | noFollow);
    const opened = await handle.stat({ bigint: true });
    if (!opened.isFile() || !sameFile(opened, file.metadata)) {
      throw new Error(`generated snippet identity changed before read: ${file.relativePath}`);
    }
    const bytes = Buffer.alloc(MAX_SNIPPET_BYTES + 1);
    const { bytesRead } = await handle.read(bytes, 0, bytes.length, 0);
    if (BigInt(bytesRead) !== opened.size || bytesRead > MAX_SNIPPET_BYTES) {
      throw new Error(`generated snippet size changed during read: ${file.relativePath}`);
    }
    const [after, canonicalAfter, canonicalDirectoryPath] = await Promise.all([
      io.lstat(file.path, { bigint: true }),
      io.realpath(file.path),
      io.realpath(directory),
    ]);
    containedPath(canonicalDirectoryPath, canonicalAfter, file.relativePath);
    if (
      resolve(canonicalAfter) !== resolve(file.path) ||
      !after.isFile() ||
      after.isSymbolicLink() ||
      !sameFile(opened, after) ||
      canonicalAfter !== file.canonicalFile
    ) {
      throw new Error(`generated snippet identity changed during read: ${file.relativePath}`);
    }
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(0, bytesRead));
  } finally {
    if (handle !== undefined) await handle.close();
  }
}

export async function retainImportedRuntimeSnippets(packageDirectory, runtimeSource, io = DEFAULT_IO) {
  const imported = await runtimeSnippetPaths(runtimeSource);
  const { files, snippetsPath } = await generatedSnippetFiles(packageDirectory, io);
  const importedPath = imported[0];
  const importedFile = files.find(({ relativePath }) => relativePath === importedPath);
  if (
    files.length !== imported.length ||
    (importedPath !== undefined && importedFile === undefined)
  ) {
    throw new Error("generated snippet inventory does not exactly match runtime imports");
  }

  if (importedFile !== undefined) {
    const directory = resolve(snippetsPath, importedPath.split("/")[1]);
    const snippetSource = await readStableSnippet(importedFile, directory, io);
    if ((await moduleSpecifiers(snippetSource)).length !== 0) {
      throw new Error("generated runtime snippet must not import or re-export another module");
    }
  }

  return imported.map((path) => `pkg/${path}`);
}
