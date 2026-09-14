#!/usr/bin/env python3
from pathlib import Path

path = Path("scripts/verify-package-release.mjs")
text = path.read_text(encoding="utf-8")
anchor = "// Execute the same packed-package runtime smoke under the caller's Node binary."
if text.count(anchor) != 1:
    raise SystemExit("clean consumer insertion anchor changed")
if "async function verifyCleanConsumer(" in text:
    raise SystemExit("verifyCleanConsumer already exists")

block = r'''async function verifyCleanConsumer(
  tarballBytes,
  packageJson,
  typescriptCompilers,
  expectedBuildMetadata,
  expectedNumericalArtifacts,
) {
  const consumer = await mkdtemp(join(tmpdir(), "labcolors-release-consumer-"));
  try {
    const tarballPath = resolve(consumer, "verified-package.tgz");
    await writeFile(tarballPath, tarballBytes, { flag: "wx", mode: 0o600 });
    await writeFile(
      join(consumer, "package.json"),
      `${JSON.stringify({ private: true, type: "module" }, null, 2)}\n`,
    );

    for (const compiler of typescriptCompilers) {
      const localTypescript = await readJson(
        resolve(
          PACKAGE_DIR,
          "node_modules",
          compiler.packageDirectory,
          "package.json",
        ),
      );
      if (localTypescript.version !== compiler.version) {
        fail(
          `installed ${compiler.role} TypeScript ${localTypescript.version} ` +
            `differs from lockfile ${compiler.version}`,
        );
      }
    }

    npm(
      [
        "install",
        "--offline",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--no-package-lock",
        "--save=false",
        tarballPath,
      ],
      consumer,
    );

    const installed = resolve(consumer, "node_modules", ...packageJson.name.split("/"));
    const installedPackage = await readJson(resolve(installed, "package.json"));
    if (installedPackage.name !== packageJson.name || installedPackage.version !== packageJson.version) {
      fail(
        `clean install resolved ${installedPackage.name}@${installedPackage.version}, ` +
          `expected ${packageJson.name}@${packageJson.version}`,
      );
    }

    await validateNumericalEvidenceArtifacts(
      installed,
      expectedNumericalArtifacts,
      "clean-installed package",
    );

    const expectedWasm = new Map(
      expectedBuildMetadata.wasm.map((artifact) => [artifact.role, artifact]),
    );
    for (const [role, artifactPath] of [["runtime", "pkg/labcolors_bg.wasm"]]) {
      const expected = expectedWasm.get(role);
      const installedWasm = await readFile(resolve(installed, artifactPath));
      if (
        expected?.path !== artifactPath ||
        expected.bytes !== installedWasm.length ||
        expected.sha256 !== sha256(installedWasm)
      ) {
        fail(`clean-installed ${role} WASM differs from the packed release input`);
      }
    }
    const installedBuildMetadata = await readJson(resolve(installed, "build-metadata.json"));
    if (!isDeepStrictEqual(installedBuildMetadata, expectedBuildMetadata)) {
      fail("clean-installed build metadata differs from the verified release inputs");
    }
    const runtimePath = resolve(consumer, "runtime-smoke.mjs");
    const typesPath = resolve(consumer, "smoke.ts");
    await writeFile(runtimePath, runtimeSmokeSource());
    await writeFile(typesPath, typeSmokeSource());

    command(process.execPath, [runtimePath], consumer);
    for (const compiler of typescriptCompilers) {
      command(
        process.execPath,
        [
          resolve(
            PACKAGE_DIR,
            "node_modules",
            compiler.packageDirectory,
            "lib",
            "tsc.js",
          ),
          "--noEmit",
          "--strict",
          "--skipLibCheck",
          "false",
          "--target",
          "ES2022",
          "--lib",
          "ES2022,DOM",
          "--module",
          "NodeNext",
          "--moduleResolution",
          "NodeNext",
          "--typeRoots",
          resolve(consumer, "node_modules", "@types"),
          typesPath,
        ],
        consumer,
      );
    }

  } finally {
    await rm(consumer, { recursive: true, force: true });
  }
}

'''
path.write_text(text.replace(anchor, block + anchor, 1), encoding="utf-8")
