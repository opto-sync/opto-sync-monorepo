import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import { verifyResolutionPolicy } from '../scripts/verify-composition.mjs';

const root = new URL('../', import.meta.url);

async function loadLock() {
  return JSON.parse(await readFile(new URL('composition.lock.json', root), 'utf8'));
}

test('pins the required repositories as immutable apps gitlinks', async () => {
  const lock = await loadLock();
  assert.deepEqual(
    lock.components.map(({ name }) => name),
    ['opto-sync-interfaces', 'opto-sync-lib', 'opto-sync-clients', 'opto-sync-cli'],
  );
  const staged = execFileSync('git', ['ls-files', '--stage'], {
    cwd: root,
    encoding: 'utf8',
  });
  for (const component of lock.components) {
    assert.match(staged, new RegExp(`160000 ${component.revision} 0\\t${component.path}`));
    assert.match(component.tree, /^[0-9a-f]{40}$/);
    assert.match(component.archiveSha256, /^[0-9a-f]{64}$/);
  }
});

test('rejects mixed Git and Zed resolution for the single engine', async () => {
  const lock = await loadLock();
  const mixed = structuredClone(lock);
  mixed.engine.zedCoordinates.push('opto-sync/syncer.c@0.1.0');
  assert.throws(
    () => verifyResolutionPolicy(mixed),
    /mixed Git and Zed engine resolution/,
  );
  assert.doesNotThrow(() => verifyResolutionPolicy(lock));
});

test('keeps the clean-room matrix and publication boundary explicit', async () => {
  const lock = await loadLock();
  assert.equal(lock.publicationEnabled, false);
  assert.equal(lock.engine.owner, 'apps/opto-sync-clients');
  assert.equal(lock.engine.resolution, 'nested-gitlink');
  assert.deepEqual(lock.engine.zedCoordinates, []);
  assert.deepEqual(lock.cleanRoomTargets.sort(), [
    'clients/dart',
    'clients/flutter',
    'clients/flutter_background',
    'clients/gleam',
    'clients/rust',
    'clients/typescript',
    'clients/wasm',
  ]);
  assert.deepEqual(lock.transportEvidence.sort(), [
    'http',
    'indexeddb',
    'postgresql',
    'sqlite',
    'supabase',
    'tcp',
    'websocket',
  ]);
});
