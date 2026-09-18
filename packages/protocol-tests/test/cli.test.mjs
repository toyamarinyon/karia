import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { binaryPath as binary } from 'karia';
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
