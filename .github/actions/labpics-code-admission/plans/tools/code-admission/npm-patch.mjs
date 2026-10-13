import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { stableJson } from '../../../architecture/model.mjs';

export const NPM_TOOLS = Object.freeze({
  yaml: { version: '2.8.1', sha256: '195759b97d3f2e6085474549c069397ad7af6160be6d7c87442b8c47cb724063' },
  pnpm: { version: '9.15.0', sha256: '09a8fe31a34fda706354680619f4002f4ccef6dadff93240d24ef6c831f0fd28' },
});
// Дескрипторы поступают только из доверенного исходника, не из candidate metadata.
export function createNpmPatchQualifier(spec) {
  if (!/^[a-z0-9][a-z0-9._-]*$/.test(spec.name) || !Number.isInteger(spec.inventoryFiles) || spec.inventoryFiles < 1 || spec.inventoryFiles > 30000)
    throw new TypeError('Invalid trusted npm descriptor');
  const hash = value => createHash('sha256').update(value).digest('hex');
  const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
  const key = `${spec.name}@${spec.version}`;
  const version = `${spec.version}(patch_hash=${spec.patchHash})`;
  const snapshot = `${spec.name}@${version}`;
  const slash = value => value.split(sep).join('/');
  const need = (condition, reason) => { if (!condition) throw Error(reason); };
  const keys = (value, allowed, reason) => need(object(value) && Object.keys(value).every(k => allowed.includes(k)), reason);

  function bytes(path, maximum = 8 * 1024 * 1024) {
    const s = lstatSync(path);
    need(s.isFile() && !s.isSymbolicLink() && s.size <= maximum, 'source-file-unqualified');
    return readFileSync(path);
  }
  function files(root) {
    const result = [];
    const visit = directory => {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        need(!entry.isSymbolicLink(), 'source-indirection');
        const path = join(directory, entry.name);
        if (entry.isDirectory()) visit(path);
        else { need(entry.isFile(), 'source-entry-unqualified'); result.push(slash(relative(root, path))); }
        need(result.length <= 30000, 'source-budget');
      }
    };
    visit(root);
    return result.sort();
  }

  function parsePatchYaml(text, yaml) {
    const document = yaml.parseDocument(text, { version: '1.2', schema: 'core', uniqueKeys: true, merge: false });
    need(document.errors.length === 0 && document.warnings.length === 0, 'lock-yaml-unqualified');
    yaml.visit(document, { Alias() { throw Error('lock-alias-unqualified'); } });
    return document.toJS({ maxAliasCount: 0 });
  }

  function safeImporter(value) {
    return value === '.' || (typeof value === 'string' && /^[A-Za-z0-9@_.+/-]+$/.test(value) && !value.split('/').some(p => !p || p === '.' || p === '..'));
  }
  function targetReference(name, reference) {
    if (name === spec.name) { need(reference === version, 'unpatched-or-mixed-reference'); return true; }
    if (typeof reference === 'string' && reference.includes(spec.name)) {
      need(reference === snapshot, 'unqualified-alias-reference'); return true;
    }
    return false;
  }

  // Lockfile понимается структурно. Проверяются ВСЕ snapshots, в том числе orphan:
  // scanner fingerprint не различает два экземпляра того же package/version.
  function lockClosure(lock, manifest, readManifest) {
    keys(lock, ['lockfileVersion', 'settings', 'overrides', 'patchedDependencies', 'importers', 'packages', 'snapshots'], 'unsupported-lock-shape');
    need(lock.lockfileVersion === '9.0', 'unsupported-lock-version');
    need(manifest.packageManager === spec.packageManager, 'package-manager-mismatch');
    keys(manifest.pnpm ?? {}, ['overrides', 'patchedDependencies'], 'unsupported-pnpm-configuration');
    const patches = manifest.pnpm?.patchedDependencies;
    need(object(patches) && patches[key] === spec.patchPath, 'manifest-patch-mismatch');
    need(Object.keys(patches).filter(p => p.includes(spec.name)).length === 1, 'mixed-manifest-patches');
    const patch = lock.patchedDependencies?.[key];
    keys(patch, ['hash', 'path'], 'lock-patch-shape');
    need(patch.hash === spec.patchHash && patch.path === spec.patchPath, 'lock-patch-mismatch');
    need(Object.keys(lock.patchedDependencies).filter(p => p.includes(spec.name)).length === 1, 'mixed-lock-patches');
    for (const overrides of [manifest.pnpm?.overrides ?? {}, lock.overrides ?? {}]) {
      need(object(overrides) && !JSON.stringify(overrides).includes(spec.name), 'target-override-unqualified');
    }
    keys(lock.settings, ['autoInstallPeers', 'excludeLinksFromLockfile'], 'unsupported-lock-settings');
    need(typeof lock.settings.autoInstallPeers === 'boolean' && lock.settings.excludeLinksFromLockfile === false, 'incomplete-lock-links');
    need(object(lock.packages) && object(lock.snapshots) && object(lock.importers), 'missing-lock-closure');
    // Совпадение части имени не превращает опубликованный пакет типов в runtime-копию.
    // Такое соседство допускается только для заранее проверенного неизменённого артефакта.
    const distinct = new Set();
    for (const [name, entry] of Object.entries(spec.distinctPackages ?? {})) {
      if (lock.packages[name] === undefined && lock.snapshots[name] === undefined) continue;
      need(stableJson(lock.packages[name]) === stableJson(entry.package) &&
        stableJson(lock.snapshots[name]) === stableJson(entry.snapshot), 'distinct-package-mismatch');
      distinct.add(name);
    }
    need(Object.keys(lock.packages).filter(p => p.includes(spec.name) && !distinct.has(p)).join() === key, 'mixed-package-identities');
    need(Object.keys(lock.snapshots).filter(p => p.includes(spec.name) && !distinct.has(p)).join() === snapshot, 'unpatched-or-mixed-snapshots');
    const pkg = lock.packages[key];
    keys(pkg, ['resolution', 'engines'], 'unqualified-package-metadata');
    keys(pkg.resolution, ['integrity'], 'unqualified-package-resolution');
    need(pkg.resolution.integrity === spec.integrity, 'package-integrity-mismatch');
    need(stableJson(lock.snapshots[snapshot]) === stableJson(spec.dependencies ? { dependencies: spec.dependencies } : {}), 'target-dependencies-mismatch');
    for (const dependency of spec.installDependencies ?? []) {
      const identity = `${dependency.name}@${dependency.version}`;
      const pkg = lock.packages[identity];
      keys(pkg, ['resolution', 'engines'], 'dependency-package-metadata');
      keys(pkg.resolution, ['integrity'], 'dependency-package-resolution');
      need(pkg.resolution.integrity === dependency.integrity, 'dependency-package-integrity');
      need(stableJson(lock.snapshots[identity]) === stableJson(dependency.dependencies ? { dependencies: dependency.dependencies } : {}), 'dependency-snapshot-mismatch');
      for (const overrides of [manifest.pnpm?.overrides ?? {}, lock.overrides ?? {}])
        need(!JSON.stringify(overrides).includes(dependency.name), 'dependency-override-unqualified');
    }
    let references = 0;
    const inspectDeps = (deps, importer) => {
      need(object(deps), 'dependency-map-unqualified');
      for (const [name, ref] of Object.entries(deps)) {
        if (importer) {
          keys(ref, ['specifier', 'version'], 'importer-reference-unqualified');
          need(typeof ref.specifier === 'string' && typeof ref.version === 'string', 'importer-reference-unqualified');
          const target = targetReference(name, ref.version);
          need(!ref.specifier.includes(spec.name) || target, 'alias-reference-mismatch');
          if (target) references++;
        } else { need(typeof ref === 'string', 'snapshot-reference-unqualified'); if (targetReference(name, ref)) references++; }
      }
    };
    for (const [name, entry] of Object.entries(lock.snapshots)) {
      keys(entry, ['dependencies', 'optionalDependencies', 'transitivePeerDependencies', 'optional'], 'snapshot-shape-unqualified');
      need(entry.optional === undefined || typeof entry.optional === 'boolean', 'snapshot-shape-unqualified');
      need(entry.transitivePeerDependencies === undefined ||
        (Array.isArray(entry.transitivePeerDependencies) && entry.transitivePeerDependencies.every(peer => typeof peer === 'string')),
        'snapshot-shape-unqualified');
      for (const group of ['dependencies', 'optionalDependencies']) inspectDeps(entry[group] ?? {}, false);
      // Не допускаем скрытую target identity в peer suffix/неизвестном поле.
      if (name !== snapshot && !distinct.has(name)) need(!name.includes(spec.name), 'unqualified-peer-identity');
    }
    const importerPaths = [];
    for (const [path, entry] of Object.entries(lock.importers)) {
      need(safeImporter(path), 'importer-path-unqualified');
      keys(entry, ['dependencies', 'devDependencies', 'optionalDependencies'], 'importer-shape-unqualified');
      const source = path === '.' ? manifest : readManifest(path);
      need(object(source) && (!source.packageManager || source.packageManager === spec.packageManager), 'importer-manifest-unqualified');
      if (path !== '.') need(source.pnpm === undefined, 'nested-pnpm-configuration');
      for (const group of ['dependencies', 'devDependencies', 'optionalDependencies']) {
        inspectDeps(entry[group] ?? {}, true);
        const declared = source[group] ?? {};
        need(object(declared), 'manifest-dependencies-unqualified');
        for (const name of new Set([...Object.keys(declared), ...Object.keys(entry[group] ?? {})])) {
          const ref = entry[group]?.[name], specifier = declared[name];
          if (name === spec.name || String(specifier).includes(spec.name) || String(ref?.version).includes(spec.name)) {
            need(typeof specifier === 'string' && ref?.specifier === specifier && targetReference(name, ref.version), 'manifest-lock-target-mismatch');
          }
        }
      }
      importerPaths.push(path);
    }
    need(importerPaths.includes('.') && references > 0, 'target-not-consumed');
    return { references, importerPaths: importerPaths.sort(), package: pkg };
  }

  function subject(root, yaml) {
    const paths = files(root);
    for (const path of paths) {
      const name = path.split('/').at(-1);
      need(!['.pnpmfile.cjs', 'pnpmfile.cjs'].includes(name), 'candidate-hook');
      if (['pnpm-lock.yaml', 'pnpm-lock.yml', 'package-lock.json', 'npm-shrinkwrap.json', 'yarn.lock', 'bun.lock', 'bun.lockb'].includes(name)) need(path === 'pnpm-lock.yaml', 'alternate-lockfile');
      if (name === '.npmrc') {
        need(path === '.npmrc', 'nested-package-configuration');
        const entries = bytes(join(root, path)).toString().split(/\r?\n/).filter(l => l.trim() && !l.trim().startsWith('#'));
        need(entries.every(l => /^(engine-strict=true|strict-peer-dependencies=false|auto-install-peers=true|save-exact=true)$/.test(l)), 'unsupported-package-configuration');
      }
    }
    const manifests = new Map(paths.filter(p => p.endsWith('/package.json') || p === 'package.json').map(p => [p, JSON.parse(bytes(join(root, p)).toString())]));
    const manifest = manifests.get('package.json');
    const lockBytes = bytes(join(root, 'pnpm-lock.yaml')), lock = parsePatchYaml(lockBytes.toString(), yaml);
    const closure = lockClosure(lock, manifest, p => manifests.get(`${p}/package.json`));
    for (const [path, source] of manifests) {
      if (closure.importerPaths.includes(path === 'package.json' ? '.' : path.slice(0, -13))) continue;
      need(!JSON.stringify(source).includes(spec.name) && source.pnpm === undefined, 'target-outside-workspace');
    }
    if (existsSync(join(root, 'pnpm-workspace.yaml'))) {
      const workspace = parsePatchYaml(bytes(join(root, 'pnpm-workspace.yaml')).toString(), yaml);
      keys(workspace, ['packages'], 'unsupported-workspace-configuration');
      need(Array.isArray(workspace.packages) && workspace.packages.every(p => typeof p === 'string'), 'workspace-packages-unqualified');
    }
    const patch = bytes(join(root, spec.patchPath));
    need(hash(patch) === spec.patchSha256, 'patch-digest-mismatch');
    return { patch, package: closure.package, evidence: { lockPath: 'pnpm-lock.yaml', lockSha256: hash(lockBytes), manifestSha256: hash(bytes(join(root, 'package.json'))), references: closure.references, importers: closure.importerPaths } };
  }

  function advisory(raw, now = Date.now()) {
    const doc = JSON.parse(raw);
    need(hash(stableJson(doc)) === spec.advisorySha256, 'advisory-evidence-changed');
    need(doc.id === spec.rules[1] && !doc.withdrawn && doc.aliases.includes(spec.rules[0]), 'advisory-identity-mismatch');
    return { id: doc.id, sha256: spec.advisorySha256, rawSha256: hash(raw), modified: doc.modified, fetchedAt: new Date(now).toISOString() };
  }

  function inventory(root) {
    const paths = files(root);
    const text = paths.map(path => `${hash(bytes(join(root, path)))}  ${path}\n`).join('');
    need(paths.length === spec.inventoryFiles && hash(text) === spec.inventorySha256, 'installed-inventory-mismatch');
    return { files: paths.length, sha256: hash(text) };
  }
  function environment(home) {
    return { PATH: `${dirname(process.execPath)}:/usr/bin:/bin`, HOME: home, TMPDIR: home, XDG_CONFIG_HOME: join(home, 'config'), XDG_CACHE_HOME: join(home, 'cache'), LANG: 'C.UTF-8', CI: 'true', NO_COLOR: '1' };
  }
  function download(url, destination, env, maximum = 8 * 1024 * 1024) {
    execFileSync('/usr/bin/curl', ['-q', '--fail', '--silent', '--show-error', '--proto', '=https', '--header', 'Cache-Control: no-cache', '--max-time', '60', '--retry', '1', '--max-filesize', String(maximum), '--output', destination, url], { env, timeout: 125000, stdio: ['ignore', 'pipe', 'pipe'] });
  }
  function fetchPinned(url, expected, destination, env, maximum = 8 * 1024 * 1024) {
    download(url, destination, env, maximum);
    need(hash(bytes(destination, maximum)) === expected, 'download-identity-mismatch');
  }
  function tool(work, name, env) {
    const pin = NPM_TOOLS[name], archive = join(work, `${name}.tgz`), root = join(work, name);
    fetchPinned(`https://registry.npmjs.org/${name}/-/${name}-${pin.version}.tgz`, pin.sha256, archive, env);
    mkdirSync(root);
    execFileSync('/usr/bin/tar', ['-xzf', archive, '--strip-components=1', '-C', root], { env, timeout: 30000, stdio: ['ignore', 'pipe', 'pipe'] });
    return root;
  }

  function qualifyFindings(root, findings, parent) {
    const selected = findings.filter(f => f.kind === 'vulnerability' && f.package === spec.name && f.installedVersion === spec.version && spec.rules.includes(f.rule));
    if (!selected.length) return [];
    const rejected = reason => selected.map(f => ({ fingerprint: f.fingerprint, status: 'unqualified', reason }));
    if (selected.length > 8 || process.platform !== 'linux' || process.arch !== 'x64') return rejected('qualification-platform-or-budget');
    if (selected.some(f => f.path !== 'pnpm-lock.yaml' || f.fixedVersion)) return rejected('finding-scope-unqualified');
    const work = mkdtempSync(join(resolve(parent), `${spec.name}-artifact-`));
    try {
      const env = environment(work), advisoryFile = join(work, 'advisory.json');
      download(`https://api.osv.dev/v1/vulns/${spec.rules[1]}`, advisoryFile, env, 256 * 1024);
      const advisoryEvidence = advisory(bytes(advisoryFile));
      const yamlRoot = tool(work, 'yaml', env), yaml = createRequire(join(yamlRoot, 'package.json'))('./dist/index.js');
      const qualifiedSubject = subject(root, yaml), pnpmRoot = tool(work, 'pnpm', env);
      const archive = join(work, 'forge.tgz');
      fetchPinned(`https://registry.npmjs.org/${spec.name}/-/${spec.name}-${spec.version}.tgz`, spec.archiveSha256, archive, env);
      need(`sha512-${createHash('sha512').update(bytes(archive)).digest('base64')}` === spec.integrity, 'archive-integrity-mismatch');
      const fixture = join(work, 'fixture'); mkdirSync(fixture);
      const dependencyPackages = {}, dependencySnapshots = {};
      for (const dependency of spec.installDependencies ?? []) {
        const name = `${dependency.name}@${dependency.version}`;
        const file = `${dependency.name}-${dependency.version}.tgz`;
        const archive = join(work, file);
        fetchPinned(`https://registry.npmjs.org/${dependency.name}/-/${file}`, dependency.archiveSha256, archive, env);
        need(`sha512-${createHash('sha512').update(bytes(archive)).digest('base64')}` === dependency.integrity, 'dependency-integrity-mismatch');
        dependencyPackages[name] = { resolution: { integrity: dependency.integrity, tarball: `file:../${file}` } };
        dependencySnapshots[name] = dependency.dependencies ? { dependencies: dependency.dependencies } : {};
      }
      writeFileSync(join(fixture, 'forge.patch'), qualifiedSubject.patch);
      writeFileSync(join(fixture, 'package.json'), JSON.stringify({ private: true, packageManager: spec.packageManager, dependencies: { [spec.name]: spec.version }, pnpm: { patchedDependencies: { [key]: 'forge.patch' } } }));
      writeFileSync(join(fixture, 'pnpm-lock.yaml'), JSON.stringify({ lockfileVersion: '9.0', settings: { autoInstallPeers: false, excludeLinksFromLockfile: false }, patchedDependencies: { [key]: { hash: spec.patchHash, path: 'forge.patch' } }, importers: { '.': { dependencies: { [spec.name]: { specifier: spec.version, version } } } }, packages: { [key]: { ...qualifiedSubject.package, resolution: { integrity: spec.integrity, tarball: 'file:../forge.tgz' } }, ...dependencyPackages }, snapshots: { [snapshot]: spec.dependencies ? { dependencies: spec.dependencies } : {}, ...dependencySnapshots } }));
      execFileSync(process.execPath, [join(pnpmRoot, 'bin/pnpm.cjs'), 'install', '--offline', '--frozen-lockfile', '--ignore-scripts', '--ignore-pnpmfile', '--store-dir', join(work, 'store'), '--config.auto-install-peers=false', '--config.side-effects-cache=false', '--config.verify-store-integrity=true'], { cwd: fixture, env, timeout: 90000, maxBuffer: 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
      const installed = join(fixture, 'node_modules/.pnpm', `${spec.name}@${spec.version}_patch_hash=${spec.patchHash}`, `node_modules/${spec.name}`);
      const installedInventory = inventory(installed);
      return selected.map(f => ({ fingerprint: f.fingerprint, status: 'code-fixed', justification: 'vulnerable_code_fixed', advisory: advisoryEvidence, patch: { source: spec.source, upstream: spec.upstream, sha256: spec.patchSha256, pnpmHash: spec.patchHash }, native: { packageManager: spec.packageManager, tools: NPM_TOOLS, archiveSha256: spec.archiveSha256, integrity: spec.integrity, inventory: installedInventory }, subject: qualifiedSubject.evidence, limitation: `Только ${spec.rules[1]} и весь опубликованный ${spec.name}@${spec.version}; не другие находки или runtime image.` }));
    } catch (error) {
      // stderr package tools не является безопасной диагностикой candidate source.
      return rejected(`artifact-proof-incomplete:${hash(String(error.message))}`);
    } finally { rmSync(work, { recursive: true, force: true }); }
  }

  return Object.freeze({ parsePatchYaml, lockClosure, subject, advisory, inventory, qualifyFindings });
}
