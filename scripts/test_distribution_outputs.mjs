import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, writeFile, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { projectHandoff, bundleHandoff } from './ci/distribution_outputs.mjs';

const source = 'a'.repeat(40);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
function fixture() {
  const binary=Buffer.from('immutable compiled product');
  const sbom=Buffer.from(JSON.stringify({bomFormat:'CycloneDX',specVersion:'1.6',components:[]}));
  const record=(path,bytes)=>({path,bytes:bytes.length,sha256:digest(bytes)});
  const statement={subject:[{name:'product',digest:{sha256:digest(binary)}}],predicate:{
    source:{commit:source},build:{noRebuild:true},evidence:{
      binary:record('product',binary),sbom:record('product.sbom.json',sbom),
    },
  }};
  return {binary,sbom,statement};
}

test('subject digest and compact SBOM preserve the verified distribution identity',()=>{
  const {binary,sbom,statement}=fixture();
  assert.deepEqual(projectHandoff(statement,sbom,source),{
    'subject-name':'product','subject-digest':'sha256:'+digest(binary),sbom:sbom.toString(),
  });
});

test('source, subject and SBOM substitutions cannot reach the attester',()=>{
  for(const change of [
    value=>{value.statement.predicate.source.commit='b'.repeat(40);},
    value=>{value.statement.subject[0].digest.sha256='b'.repeat(64);},
    value=>{value.statement.subject.push(value.statement.subject[0]);},
    value=>{value.statement.predicate.evidence.binary.path='../product';},
    value=>{value.statement.predicate.evidence.binary.path='product\noutput=forged';},
    value=>{value.statement.predicate.build.noRebuild=false;},
    value=>{value.sbom=Buffer.from('{}');},
    value=>{value.statement.predicate.evidence.sbom.bytes=0;},
  ]) {
    const value=fixture();change(value);
    assert.throws(()=>projectHandoff(value.statement,value.sbom,source),/Distribution output/);
  }
  const value=fixture();
  value.sbom=Buffer.from(JSON.stringify({bomFormat:'CycloneDX',text:'x'.repeat(61*1024)}));
  value.statement.predicate.evidence.sbom={bytes:value.sbom.length,sha256:digest(value.sbom)};
  assert.throws(()=>projectHandoff(value.statement,value.sbom,source),/exceeds a job output/);
});

test('real bundle reads reject altered bytes and linked files before producing outputs',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'distribution-output-'));
  try {
    const {binary,sbom,statement}=fixture();
    await writeFile(join(directory,'product.intoto.json'),JSON.stringify(statement));
    await writeFile(join(directory,'product.sbom.json'),sbom);
    await writeFile(join(directory,'product'),binary);
    const result=bundleHandoff(directory,'product.intoto.json',source);
    assert.equal(result.output['subject-digest'],'sha256:'+digest(binary));
    assert.deepEqual(Object.keys(result.views).sort(),['product.intoto.json','product.sbom.json']);
    await writeFile(join(directory,'product'),Buffer.from('changed'));
    assert.throws(()=>bundleHandoff(directory,'product.intoto.json',source),/binary bytes differ/);
    await rm(join(directory,'product'));
    await symlink(join(directory,'product.sbom.json'),join(directory,'product'));
    assert.throws(()=>bundleHandoff(directory,'product.intoto.json',source));
  } finally {await rm(directory,{recursive:true,force:true});}
});
