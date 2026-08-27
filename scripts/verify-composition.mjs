import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const lock = JSON.parse(readFileSync(resolve(root, 'composition.lock.json'), 'utf8'));

function fail(message) {
  throw new Error(message);
}

function git(args, options = {}) {
  const result = execFileSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    ...options,
  });
  return Buffer.isBuffer(result) ? result : result.trim();
}

function packageName(componentPath) {
  const manifest = readFileSync(resolve(root, componentPath, 'Cargo.toml'), 'utf8');
  const match = manifest.match(/^name\s*=\s*"([^"]+)"/m);
  return match?.[1];
}

export function verifyResolutionPolicy(document) {
  if (document.schema !== 'opto-sync.composition-lock.v1') {
    fail('unsupported composition lock schema');
  }
  if (document.resolver !== 'git-tree-and-deterministic-tar-v1') {
    fail('composition resolver changed');
  }
  if (document.publicationEnabled !== false) {
    fail('composition publication must remain disabled');
  }
  if (!Array.isArray(document.components) || document.components.length !== 4) {
    fail('composition must contain exactly four component repositories');
  }
  if (
    document.engine?.resolution !== 'nested-gitlink' ||
    document.engine.zedCoordinates?.length !== 0
  ) {
    fail('mixed Git and Zed engine resolution is forbidden');
  }
}

verifyResolutionPolicy(lock);

const staged = new Map(
  git(['ls-files', '--stage'])
    .split('\n')
    .filter((line) => line.startsWith('160000 '))
    .map((line) => {
      const [metadata, componentPath] = line.split('\t');
      return [componentPath, metadata.split(' ')[1]];
    }),
);

const seen = new Set();
for (const component of lock.components) {
  if (seen.has(component.name)) fail('duplicate component identity');
  seen.add(component.name);
  if (staged.get(component.path) !== component.revision) {
    fail(`${component.name} gitlink differs from the immutable lock`);
  }
  if (git(['-C', component.path, 'rev-parse', 'HEAD']) !== component.revision) {
    fail(`${component.name} checkout differs from the immutable lock`);
  }
  if (git(['-C', component.path, 'rev-parse', 'HEAD^{tree}']) !== component.tree) {
    fail(`${component.name} tree differs from the immutable lock`);
  }
  const archive = git(
    ['-C', component.path, 'archive', '--format=tar', component.revision],
    { encoding: null },
  );
  const digest = createHash('sha256').update(archive).digest('hex');
  if (digest !== component.archiveSha256) {
    fail(`${component.name} deterministic archive digest changed`);
  }
  const configuredUrl = git(['config', '-f', '.gitmodules', '--get', `submodule.${component.path}.url`]);
  if (configuredUrl !== component.repository) {
    fail(`${component.name} repository identity changed`);
  }
  git(['-C', component.path, 'merge-base', '--is-ancestor', component.revision, 'HEAD']);
}

const cargoIdentities = new Map([
  ['apps/opto-sync-interfaces', 'opto-sync-interfaces'],
  ['apps/opto-sync-lib', 'opto-sync-lib'],
  ['apps/opto-sync-cli', 'opto-sync-cli'],
]);
for (const [componentPath, expected] of cargoIdentities) {
  if (packageName(componentPath) !== expected) {
    fail(`${componentPath} package identity changed`);
  }
}

const clientsRoot = resolve(root, 'apps/opto-sync-clients');
const languageTargets = readdirSync(resolve(clientsRoot, 'clients')).filter((entry) =>
  statSync(resolve(clientsRoot, 'clients', entry)).isDirectory(),
);
if (languageTargets.length < 15) {
  fail('client language matrix fell below fifteen targets');
}
for (const target of lock.cleanRoomTargets) {
  if (!statSync(resolve(clientsRoot, target)).isDirectory()) {
    fail(`clean-room target is absent: ${target}`);
  }
}
for (const evidence of lock.transportEvidence) {
  try {
    git(['-C', 'apps/opto-sync-clients', 'grep', '-i', '-l', evidence, 'HEAD', '--']);
  } catch {
    fail(`transport evidence is absent: ${evidence}`);
  }
}

const engineLine = git([
  '-C',
  lock.engine.owner,
  'ls-tree',
  'HEAD',
  'syncer.c',
]);
const expectedEngine = `160000 commit ${lock.engine.revision}\tsyncer.c`;
if (engineLine !== expectedEngine) {
  fail('nested sync engine differs from the single reviewed revision');
}
if (lock.engine.owner !== 'apps/opto-sync-clients') {
  fail('sync engine ownership moved outside the clients repository');
}

for (const component of lock.components.filter(({ name }) => name !== 'opto-sync-clients')) {
  const manifests = git(['-C', component.path, 'ls-files', 'Cargo.toml', 'Cargo.lock', '.zpkg.toml'])
    .split('\n')
    .filter(Boolean);
  for (const manifest of manifests) {
    const content = readFileSync(resolve(root, component.path, manifest), 'utf8');
    if (/^name\s*=\s*"syncer(?:-c|-rs)?"/m.test(content)) {
      fail(`${component.name} installs a second sync engine`);
    }
  }
}

console.log(
  JSON.stringify({
    schema: lock.schema,
    components: lock.components.length,
    languageTargets: languageTargets.length,
    engineRevision: lock.engine.revision,
    publicationEnabled: false,
    verified: true,
  }),
);
