import { createHash } from 'node:crypto';
import { constants, openSync, fstatSync, readSync, closeSync, appendFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const MAX_FILE = 64 * 1024 * 1024;
const MAX_OUTPUT = 60 * 1024;
const namePattern = /^[A-Za-z0-9][A-Za-z0-9._-]{0,254}$/u;
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const refuse = reason => { throw new Error('Distribution output: ' + reason); };

function readFile(directory, name, limit = MAX_FILE) {
  if (typeof name !== 'string' || !namePattern.test(name)) refuse('file escapes the bundle');
  const fd = openSync(join(directory, name), constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const metadata = fstatSync(fd);
    if (!metadata.isFile() || metadata.size > limit) refuse('expected a bounded regular file');
    const buffer = Buffer.alloc(limit + 1 > 1024 * 1024 ? 1024 * 1024 : limit + 1);
    const chunks = [];
    let size = 0;
    for (;;) {
      const count = readSync(fd, buffer, 0, Math.min(buffer.length, limit + 1 - size));
      if (!count) break;
      size += count;
      if (size > limit) refuse('file exceeded its bound while reading');
      chunks.push(Buffer.from(buffer.subarray(0, count)));
    }
    return Buffer.concat(chunks);
  } finally {
    closeSync(fd);
  }
}

// Аттестации получают проверенный subject и малый SBOM. Сам бинарник остаётся
// у сборщика; передача его копии не даёт подписывающему заданию новых сведений.
export function projectHandoff(statement, sbomBytes, expectedSha) {
  if (!/^[a-f0-9]{40}$/u.test(expectedSha ?? '') || statement?.predicate?.source?.commit !== expectedSha) {
    refuse('source revision differs');
  }
  const { subject, predicate } = statement;
  const binary = predicate?.evidence?.binary;
  const sbom = predicate?.evidence?.sbom;
  if (!Array.isArray(subject) || subject.length !== 1 || predicate?.build?.noRebuild !== true
    || typeof binary?.path !== 'string' || !namePattern.test(binary.path)
    || !/^[a-f0-9]{64}$/u.test(binary?.sha256 ?? '')
    || subject[0]?.name !== binary.path || subject[0]?.digest?.sha256 !== binary.sha256) {
    refuse('subject is not bound to the verified binary');
  }
  if (!Buffer.isBuffer(sbomBytes) || sbomBytes.length > MAX_FILE
    || sbom?.bytes !== sbomBytes.length || sbom?.sha256 !== sha256(sbomBytes)) {
    refuse('SBOM bytes differ from the statement');
  }
  const document = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(sbomBytes));
  if (document?.bomFormat !== 'CycloneDX') refuse('SBOM format is unknown');
  const compact = JSON.stringify(document);
  if (Buffer.byteLength(compact) > MAX_OUTPUT) refuse('SBOM exceeds a job output');
  return { 'subject-name': binary.path, 'subject-digest': 'sha256:' + binary.sha256, sbom: compact };
}

export function bundleHandoff(directory, statementName, expectedSha) {
  const bytes = readFile(directory, statementName, 1024 * 1024);
  const statement = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  const evidence = statement?.predicate?.evidence;
  if (!evidence || typeof evidence !== 'object' || Array.isArray(evidence)) refuse('evidence is absent');
  const views = {};
  let sbom;
  for (const [kind, record] of Object.entries(evidence)) {
    const data = readFile(directory, record?.path);
    if (record.bytes !== data.length || record.sha256 !== sha256(data)) refuse(kind + ' bytes differ');
    if (kind === 'sbom') sbom = data;
    if (record.path.endsWith('.json')) views[record.path] = JSON.parse(data.toString('utf-8'));
  }
  return { output: projectHandoff(statement, sbom, expectedSha), views: { [statementName]: statement, ...views } };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [directory, statementName, expectedSha] = process.argv.slice(2);
  const { output, views } = bundleHandoff(directory, statementName, expectedSha);
  for (const [name, value] of Object.entries(output)) appendFileSync(process.env.GITHUB_OUTPUT, name + '=' + value + '\n');
  for (const [file, value] of Object.entries(views)) console.log(JSON.stringify({ file, value }));
}
