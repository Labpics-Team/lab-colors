import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { userInfo } from 'node:os';
import { pathToFileURL } from 'node:url';
import { fileDigest, prepareQualifiedExecutable } from './identity.mjs';

export const GO = Object.freeze({ version:'1.26.6', archive:'708effb774be8237570d0add163225abbdfaf4fca28b2611df167beba4feef89', binary:'29e6e0b8be61beb1489ceae62b304343566de8a1dc700af74bde7aeb9c80ad45' });
const hash = value => createHash('sha256').update(value).digest('hex');
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const modulePath = value => typeof value === 'string' && /^[A-Za-z0-9][A-Za-z0-9._~+/-]*$/.test(value) && !value.split('/').some(x => x === '..' || !x);

// Берётся union всех affected ranges/platforms. Ни версии, ни symbols здесь
// не используются для исключения: пакет присутствует => исходный blocker остаётся.
export function advisoryPackages(document, finding) {
  if (!object(document) || !/^1\./.test(document.schema_version??'') || document.id !== finding.rule || document.withdrawn || document.database_specific?.review_status !== 'REVIEWED' || !Array.isArray(document.affected)) throw Error('unqualified advisory');
  const matching = document.affected.filter(a => a?.package?.ecosystem === 'Go' && a.package.name === finding.package);
  if (!matching.length) throw Error('module absent from advisory');
  const paths = [];
  for (const entry of matching) {
    const imports = entry.ecosystem_specific?.imports;
    if (!Array.isArray(imports) || !imports.length || imports.length > 256) throw Error('incomplete import scope');
    for (const item of imports) {
      if (!object(item) || !modulePath(item.path) || !(item.path === finding.package || item.path.startsWith(finding.package + '/'))) throw Error('foreign import scope');
      paths.push(item.path);
    }
  }
  return [...new Set(paths)].sort();
}

export function vendorPackages(text) {
  if (text.length > 4 * 1024 * 1024) throw Error('inventory budget');
  const packages = text.split('\n').filter(line => line && !line.startsWith('#'));
  if (!packages.every(modulePath)) throw Error('malformed native inventory');
  return [...new Set(packages)].sort();
}

export function qualifiedReplacements(metadata) {
  if (!object(metadata) || !modulePath(metadata.Module?.Path) || !Array.isArray(metadata.Replace??[])) throw Error('invalid module metadata');
  const replacements=metadata.Replace??[];
  if (replacements.length>64) throw Error('replacement budget');
  return replacements.map(entry=>{
    const old=entry?.Old,newModule=entry?.New;
    if (!object(old) || !object(newModule) || !modulePath(old.Path) || old.Path===metadata.Module.Path ||
        (old.Version!==undefined && (typeof old.Version!=='string' || !old.Version)) ||
        !modulePath(newModule.Path) || typeof newModule.Version!=='string' ||
        !/^v[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.+-]+)?$/.test(newModule.Version)) {
      throw Error('replacement is not a remote versioned module');
    }
    return {oldPath:old.Path,...(old.Version?{oldVersion:old.Version}:{}),newPath:newModule.Path,newVersion:newModule.Version};
  }).sort((a,b)=>`${a.oldPath}@${a.oldVersion??''}`.localeCompare(`${b.oldPath}@${b.oldVersion??''}`));
}

export const replacesAffectedModule = (replacements, affected) =>
  replacements.some(r=>r.oldPath===affected || r.newPath===affected);

function environment(home, root) {
  // Reuse only ZIP proxy bytes, never mutable unpacked modules/.ziphash. Native
  // Go copies into a private cache and verifies go.sum/sumdb before inspection.
  const proxyCache = join(userInfo().homedir,'go/pkg/mod/cache/download');
  const proxy = (existsSync(proxyCache) ? pathToFileURL(proxyCache).href + ',' : '') + 'https://proxy.golang.org';
  return { PATH:'/usr/bin:/bin', HOME:home, TMPDIR:home, GOROOT:root, GOENV:'off', GOWORK:'off', GOTOOLCHAIN:'local', GOFLAGS:'-mod=readonly -modcacherw', GOCACHE:join(home,'build-cache'), GOMODCACHE:join(home,'modules'), GOPROXY:proxy, GOSUMDB:'sum.golang.org', GOVCS:'*:off', LANG:'C.UTF-8' };
}
function native(runtime, args, cwd, env) {
  const r = runtime.run(args, { cwd, env, encoding:'utf8', maxBuffer:8*1024*1024, timeout:180000, killSignal:'SIGKILL' });
  if (r.error || r.status !== 0) throw Error('native Go closure incomplete');
  return r.stdout;
}
function curl(url, env, extra = []) {
  return execFileSync('curl', ['-q','--fail','--silent','--show-error','--proto','=https','--max-time','90','--retry','2',...extra,url], { env, encoding:'utf8', maxBuffer:256*1024, timeout:120000, stdio:['ignore','pipe','pipe'] });
}
function qualifiedGo(work) {
  const env = { PATH:'/usr/bin:/bin', HOME:work, TMPDIR:work, LANG:'C.UTF-8' };
  const archive = join(work,'go.tar.gz');
  curl(`https://dl.google.com/go/go${GO.version}.linux-amd64.tar.gz`,env,['--max-filesize','66890545','--output',archive]);
  if (fileDigest(archive) !== GO.archive) throw Error('Go archive identity mismatch');
  execFileSync('tar',['-xzf',archive,'-C',work],{env,timeout:30000,stdio:['ignore','pipe','pipe']});
  rmSync(archive);
  const root = join(work,'go'), runtime = prepareQualifiedExecutable(join(root,'bin/go'),GO.binary,work,'Go import oracle');
  return { root, runtime };
}

// vendor подавляет ошибки отсутствующих импортов, похожих на stdlib.
// Перед использованием inventory разбираем исходники всех платформ и tags.
// Исполняется только проверенный Go SDK и этот доверенный код проверки.
const importClosureCheck = `package main
import (
 "fmt"
 "go/parser"
 "go/token"
 "io/fs"
 "os"
 "path/filepath"
 "strconv"
 "strings"
)
func packageDir(path string) bool {
 entries, err := os.ReadDir(path)
 if err != nil { return false }
 for _, entry := range entries { if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".go") { return true } }
 return false
}
func checkTree(tree, subject, goroot, module string) error {
 return filepath.WalkDir(tree, func(path string, entry fs.DirEntry, err error) error {
  if err != nil { return err }
  if entry.Type()&os.ModeSymlink != 0 { return fmt.Errorf("source indirection") }
  if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".go") { return nil }
  file, err := parser.ParseFile(token.NewFileSet(), path, nil, parser.ImportsOnly)
  if err != nil { return err }
  for _, imp := range file.Imports {
   name, err := strconv.Unquote(imp.Path.Value)
   if err != nil { return err }
   if name == "C" { continue }
   first := strings.SplitN(name, "/", 2)[0]
   if strings.Contains(first, ".") { continue }
   parts := strings.Split(name, "/")
   for _, part := range parts { if part == "" || part == "." || part == ".." { return fmt.Errorf("invalid import path") } }
   if name == module || strings.HasPrefix(name, module+"/") {
    suffix := strings.TrimPrefix(name, module)
    suffix = strings.TrimPrefix(suffix, "/")
    if packageDir(filepath.Join(subject, filepath.FromSlash(suffix))) { continue }
   }
   if packageDir(filepath.Join(goroot, "src", filepath.FromSlash(name))) { continue }
   return fmt.Errorf("unresolved import")
  }
  return nil
 })
}
func main() {
 if len(os.Args) != 5 { os.Exit(2) }
 for _, tree := range []string{os.Args[1], os.Args[2]} {
  if err := checkTree(tree, os.Args[1], os.Args[3], os.Args[4]); err != nil { fmt.Fprintln(os.Stderr, err); os.Exit(1) }
 }
}
`;
function verifyImportClosure(runtime, root, work, source, vendor, module, env) {
  const checker=join(work,'import-closure.go');
  writeFileSync(checker,importClosureCheck);
  native(runtime,['run',checker,source,vendor,root,module],work,{...env,GO111MODULE:'off',GOPROXY:'off'});
}

export function qualifyImportOracle(runtime, root, work) {
  const env = { ...environment(work,root), GOPROXY:'off', GOSUMDB:'off' };
  const fixture = join(work,'controls'), dep = join(fixture,'dependency'), main = join(fixture,'main');
  for (const p of [join(dep,'safe'),join(dep,'unsafe'),main]) mkdirSync(p,{recursive:true});
  writeFileSync(join(dep,'go.mod'),'module fixture.invalid/dependency\n\ngo 1.26.0\n');
  for (const p of ['safe','unsafe']) writeFileSync(join(dep,p,'p.go'),`package ${p}\n`);
  writeFileSync(join(main,'go.mod'),'module fixture.invalid/main\n\ngo 1.26.0\n\nrequire fixture.invalid/dependency v0.0.0\nreplace fixture.invalid/dependency => ../dependency\n');
  writeFileSync(join(main,'main.go'),'package main\nimport _ "fixture.invalid/dependency/safe"\nfunc main() {}\n');
  const inventory = () => {
    const vendor=join(fixture,'vendor');
    native(runtime,['mod','vendor','-o',vendor],main,env);
    verifyImportClosure(runtime,root,work,main,vendor,'fixture.invalid/main',env);
    return vendorPackages(readFileSync(join(vendor,'modules.txt'),'utf8'));
  };
  if (inventory().includes('fixture.invalid/dependency/unsafe')) throw Error('clean native control failed');
  // Linux-only go list пропустил бы этот импорт. AnyTags у native vendor обязан увидеть его.
  writeFileSync(join(main,'hostile_windows.go'),'//go:build windows && custom_capability\n\npackage main\nimport _ "fixture.invalid/dependency/unsafe"\n');
  if (!inventory().includes('fixture.invalid/dependency/unsafe')) throw Error('cross-platform native control failed');
  writeFileSync(join(main,'incomplete.go'),'package main\nimport _ "missing.invalid/package"\n');
  let rejected = false; try { inventory(); } catch { rejected = true; }
  if (!rejected) throw Error('incomplete graph was accepted');
  writeFileSync(join(main,'incomplete.go'),'package main\nimport _ "missing/package"\n');
  rejected = false; try { inventory(); } catch { rejected = true; }
  if (!rejected) throw Error('missing stdlib-like import was accepted');
  rmSync(fixture,{recursive:true,force:true});
  return {clean:'passed',foreignPlatformAndTag:'detected',incomplete:'rejected'};
}

function qualifiedSubject(root, mod, runtime, env, work) {
  const directory = dirname(resolve(root,mod));
  if (!mod.endsWith('go.mod') || relative(root,directory).startsWith('..')) throw Error('foreign subject');
  for (let p=directory;;p=dirname(p)) {
    if (existsSync(join(p,'go.work')) || existsSync(join(p,'vendor'))) throw Error('workspace/vendor source not qualified');
    if (p===resolve(root)) break;
    if (dirname(p)===p) throw Error('subject root mismatch');
  }
  // AnyTags intentionally excludes "ignore". Не выдаём полный closure при
  // наличии таких особых tools в источнике; поддержка требует отдельного proof.
  function visit(path) {
    for (const e of readdirSync(path,{withFileTypes:true})) {
      const p=join(path,e.name);
      if (e.isSymbolicLink()) throw Error('source indirection');
      if (e.isDirectory()) visit(p);
      else if (e.name.endsWith('.go') && /^\/\/\s*(?:go:build|\+build).*\bignore\b/m.test(readFileSync(p,'utf8'))) throw Error('ignore-tag source not qualified');
    }
  }
  visit(directory);
  const metadata=JSON.parse(native(runtime,['mod','edit','-json'],directory,env));
  const replacements=qualifiedReplacements(metadata);
  const before = { mod:fileDigest(join(directory,'go.mod')), sum:existsSync(join(directory,'go.sum'))?fileDigest(join(directory,'go.sum')):null };
  const vendor=join(work,'vendor');
  native(runtime,['mod','vendor','-o',vendor],directory,env);
  verifyImportClosure(runtime,env.GOROOT,work,directory,vendor,metadata.Module.Path,env);
  native(runtime,['mod','verify'],directory,env);
  const after = { mod:fileDigest(join(directory,'go.mod')), sum:existsSync(join(directory,'go.sum'))?fileDigest(join(directory,'go.sum')):null };
  if (JSON.stringify(before)!==JSON.stringify(after)) throw Error('module inputs changed');
  const inventory=existsSync(join(vendor,'modules.txt'))?readFileSync(join(vendor,'modules.txt'),'utf8'):'';
  const packages=vendorPackages(inventory);
  return {mainModule:metadata.Module.Path,replacements,packages,goModSha256:before.mod,goSumSha256:before.sum,inventorySha256:hash(inventory)};
}

export function qualifyGoFindings(root, findings, parent) {
  const selected=findings.filter(f=>f.kind==='vulnerability' && /^GO-\d{4}-\d+$/.test(f.rule) && /(^|\/)go\.mod$/.test(f.path) && modulePath(f.package));
  if (!selected.length) return [];
  if (selected.length>8 || process.platform!=='linux' || process.arch!=='x64') return selected.map(f=>({fingerprint:f.fingerprint,status:'unqualified'}));
  const work=mkdtempSync(join(parent,'go-import-scope-')), dispositions=[];
  let sdk;
  try {
    const env={PATH:'/usr/bin:/bin',HOME:work,TMPDIR:work,LANG:'C.UTF-8'}, prepared=[];
    for (const f of selected) {
      try {
        const raw=curl(`https://vuln.go.dev/ID/${f.rule}.json`,env), document=JSON.parse(raw), imports=advisoryPackages(document,f);
        prepared.push({f,imports,advisorySha256:hash(raw)});
      } catch { dispositions.push({fingerprint:f.fingerprint,status:'unqualified',reason:'upstream-scope-incomplete'}); }
    }
    if (!prepared.length) return dispositions;
    sdk=qualifiedGo(work);
    const controls=qualifyImportOracle(sdk.runtime,sdk.root,work), memo=new Map();
    for (const item of prepared) {
      const {f,imports,advisorySha256}=item;
      try {
        if (!memo.has(f.path)) {
          const subject=mkdtempSync(join(work,'subject-'));
          memo.set(f.path,qualifiedSubject(root,f.path,sdk.runtime,environment(subject,sdk.root),subject));
        }
        const proof=memo.get(f.path);
        // Замена затронутого модуля не наследует доказательство отсутствия
        // уязвимых пакетов из upstream advisory.
        if (replacesAffectedModule(proof.replacements,f.package)) throw Error('affected module replaced');
        const absent=proof.mainModule!==f.package && imports.every(p=>!proof.packages.includes(p));
        dispositions.push({fingerprint:f.fingerprint,status:absent?'not-affected':'affected',justification:absent?'vulnerable_code_not_present':'affected-import-present',scope:'native Go build/test imports across platforms and tags; not runtime symbols',advisory:{id:f.rule,sha256:advisorySha256,imports},native:{version:GO.version,archiveSha256:GO.archive,binarySha256:GO.binary,controls},subject:{goModPath:f.path,...proof}});
      } catch { dispositions.push({fingerprint:f.fingerprint,status:'unqualified',reason:'native-import-closure-incomplete'}); }
    }
    return dispositions;
  } catch {
    return selected.map(f=>({fingerprint:f.fingerprint,status:'unqualified',reason:'native-qualification-incomplete'}));
  } finally { sdk?.runtime.close(); rmSync(work,{recursive:true,force:true}); }
}
