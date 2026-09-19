import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { binaryPath as binary } from 'karia-css';
test('Rust CLI returns actionable JSON and nonzero on unknown variable', async () => {
  const root = await mkdtemp(join(tmpdir(), 'css-cli-'));
  try {
    await writeFile(join(root, 'tokens.css'), ':root { --accent: #285a43 }');
    await writeFile(join(root, 'button.module.css'), '.button { background: var(--accent); color: var(--optional, white); }');
    const run = args => spawnSync(binary, args, { encoding: 'utf8' });
    const inspected = run(['inspect', root, '--token', '--accent']);
    assert.equal(inspected.status, 0, inspected.stderr);
    assert.equal(JSON.parse(inspected.stdout)[0].value, '#285a43');
    const valid = run(['check', root, '--format', 'json']);
    assert.equal(valid.status, 0, valid.stderr);
    assert.equal(JSON.parse(valid.stdout).diagnostics.length, 0);
    await writeFile(join(root, 'button.module.css'), '.button { background: var(--typo); }');
    const invalid = run(['check', root, '--format', 'json']);
    assert.equal(invalid.status, 1, invalid.stderr);
    const diagnostics = JSON.parse(invalid.stdout).diagnostics;
    assert.ok(diagnostics.some(x => /--typo/.test(x.message)));
    assert.ok(diagnostics.every(x => typeof x.uri === 'string' && Number.isInteger(x.start)));
    const missing = run(['check', join(root, 'missing'), '--format', 'json']);
    assert.notEqual(missing.status, 0, 'Missing directory must not report success');
  } finally { await rm(root, { recursive: true, force: true }); }
});
test('Rust CLI respects .gitignore and hard directory exclusions', async () => {
  const root = await mkdtemp(join(tmpdir(), 'css-cli-ignore-'));
  try {
    await mkdir(join(root, 'generated'));
    await mkdir(join(root, 'dist'));
    await writeFile(join(root, '.gitignore'), 'generated/\nsecret.css\n');
    await writeFile(join(root, 'generated', 'hidden.css'), ':root { --hidden: 1px }');
    await writeFile(join(root, 'secret.css'), ':root { --secret: 2px }');
    await writeFile(join(root, 'dist', 'built.css'), ':root { --built: 3px }');
    await writeFile(join(root, 'app.css'), '.a { color: var(--hidden); background: var(--secret); padding: var(--built); }');
    const run = args => spawnSync(binary, args, { encoding: 'utf8' });
    const check = run(['check', root, '--format', 'json']);
    assert.equal(check.status, 1, check.stdout);
    const diagnostics = JSON.parse(check.stdout).diagnostics;
    for (const name of ['--hidden', '--secret', '--built']) {
      assert.ok(diagnostics.some(d => d.message.includes(name)), `${name} should be unknown`);
    }
    assert.equal(JSON.parse(run(['inspect', root]).stdout).length, 0);
    await writeFile(join(root, '.gitignore'), 'generated/\n');
    const recheck = run(['check', root, '--format', 'json']);
    assert.equal(recheck.status, 1, recheck.stdout);
    const remaining = JSON.parse(recheck.stdout).diagnostics;
    assert.ok(remaining.some(d => d.message.includes('--hidden')));
    assert.ok(remaining.some(d => d.message.includes('--built')));
    assert.ok(!remaining.some(d => d.message.includes('--secret')), 'un-ignored file must be indexed');
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('Rust worker CSS context follows update/remove and UTF-8 token ranges', () => {
  const uri = 'file:///context.css';
  const text = '/* 🌿 */ .a { color: var(--日本); }';
  const start = Buffer.byteLength(text.slice(0, text.indexOf('--日本')));
  const end = start + Buffer.byteLength('--日本');
  const requests = [
    { method: 'update', uri, text },
    { method: 'css-context', uri, offset: start + 2 },
    { method: 'update', uri, text: '/* var(--日本) */' },
    { method: 'css-context', uri, offset: 9 },
    { method: 'remove', uri },
    { method: 'css-context', uri, offset: start + 2 },
  ];
  const run = spawnSync(binary, ['serve'], {
    encoding: 'utf8', input: requests.map((request, id) => JSON.stringify({ id, ...request })).join('\n') + '\n',
  });
  assert.equal(run.status, 0, run.stderr);
  const responses = run.stdout.trim().split('\n').map(line => JSON.parse(line));
  assert.equal(responses.length, requests.length);
  assert.deepEqual(responses[1].result.variable, { name: '--日本', start, end });
  assert.equal(responses[1].result.completion.prefix, '--');
  assert.equal(responses[3].result.variable, null);
  assert.equal(responses[3].result.completion, null);
  assert.equal(responses[5].result, null);
});
