import test from 'node:test';
import assert from 'node:assert/strict';
import { mayRetainOnSuccess, isStorageSource } from './code-admission/storage.mjs';
import { commandJson } from './code-admission.mjs';

test('unknown inputs cannot grant routine storage', () => {
  for (const condition of [undefined, 'true', 'always()', '!failure()', "inputs.export", "failure() || inputs.export", "github.event_name != 'pull_request'", "failure() || (true && unknown())", "failure('wrong')", 'invalid('])
    assert.equal(mayRetainOnSuccess(condition), true, String(condition));
});

test('guard composition preserves the storage boundary under nesting and permutation', () => {
  const guards = ["failure()", "github.event_name == 'workflow_dispatch'", "github.ref_type == 'tag'", "startsWith(github.ref, 'refs/tags/')"];
  for (const guard of guards) for (const other of ['true', 'always()', 'inputs.enabled', 'unknown()']) {
    for (const condition of [guard, `(${guard})`, `${guard} && ${other}`, `${other} && (${guard})`, '${{ ' + guard + ' && ' + other + ' }}'])
      assert.equal(mayRetainOnSuccess(condition), false, condition);
    assert.equal(mayRetainOnSuccess(`${guard} || ${other}`), true);
  }
});


test('action metadata stays in scope when moved between local action roots', () => {
  for (const path of ['action.yml','action.yaml','.github/actions/publish/action.yml','tools/storage/action.yaml','.github/workflows/check.yml'])
    assert.equal(isStorageSource(path),true,path);
  for (const path of ['action.yml.txt','readme.md','example-actions.yaml','.github/workflows/nested/fixture.yml'])
    assert.equal(isStorageSource(path),false,path);
});


test('failure OR success is conservatively executable on success', () => {
  assert.equal(mayRetainOnSuccess('failure() || success()'),true);
  assert.equal(mayRetainOnSuccess('failure() && success()'),false);
});


test('timeout errors preserve structural analyzer failure reason', () => {
  const runtime={run:()=>({error:new Error('ETIMEDOUT')})};
  assert.throws(()=>commandJson(runtime,['run'],{allowedStatuses:[0,1],cwd:'/tmp',home:'/tmp'}),/Qualified structural analyzer failed to start: ETIMEDOUT/);
});
