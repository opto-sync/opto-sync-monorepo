# opto-sync-monorepo

Composition workspace for Opto Sync interfaces, library, and clients without
duplicate engine ownership.

## Status

Implemented composition superproject; publication remains disabled. The
`apps/` directory contains exact Git submodules for interfaces, policy library,
the 27-target client matrix, and the read-only CLI. `composition.lock.json`
records every commit, Git tree, package identity, and SHA-256 digest of a
deterministic `git archive`.

```sh
git clone --recurse-submodules https://github.com/opto-sync/opto-sync-monorepo.git
npm ci --ignore-scripts
npm test
npm run verify
```

## Immutable component set

| Component | Reviewed revision | Role |
| --- | --- | --- |
| `apps/opto-sync-interfaces` | `b92b3a2eb43eeb183144521a188ae465a013951e` | Transport-neutral contracts and generated declarations. |
| `apps/opto-sync-lib` | `f2ea017328aff58401d38a6d36480c45b39d3c15` | Deterministic lifecycle, retry, checkpoint, and replay policy. |
| `apps/opto-sync-clients` | `1d22a98fbef4888e36ca1f78b72d469f74f61721` | 27 client targets plus the sole nested `syncer.c` engine. |
| `apps/opto-sync-cli` | `7a07ed4de7bed1656f3dfa1cbe73a8c996a8eab3` | Bounded validation, trace replay, and redacted diagnostics. |

The verifier checks the committed gitlinks, repository URLs, commit ancestry,
tree IDs, package names, deterministic archive digests, required clean-room
target directories, and evidence for IndexedDB, SQLite, PostgreSQL, Supabase,
HTTP, WebSocket, and TCP. It rejects a second engine or a mixed Git/Zed engine
resolution before any component build is started.

## Composition contract

This repository exists to test and release a coherent Opto Sync package set. It
does not become the source of truth for code owned by the component
repositories.

The intended logical graph is:

```text
interfaces <- lib <- clients <- applications
                         ^
                         |
                        cli
```

The workspace may pin component repositories or released Zed packages for
integration testing, but one composition must not include the same logical
dependency through both mechanisms. In particular, the `syncer.c` gitlink owned
by `opto-sync-clients` must not be accompanied by a second Zed-resolved engine.

## Repository boundaries

- Application and service code remains in its owning repository.
- Kubernetes cluster composition remains outside this repository.
- SQL declarations remain domain-owned and centrally registered; this
  repository does not apply production migrations.
- Secrets and decrypted environment files are never committed.
- Destructive and cross-runtime tests run in `opto-sync-test` or another
  isolated environment, not against production systems.

## Implemented composition gates

- `.gitmodules` and the Git index pin all four components under `apps/`.
- `composition.lock.json` is the reviewed resolver output and release-set
  provenance record; `scripts/verify-composition.mjs` recomputes its evidence.
- The clients pin supplies TypeScript, Dart/Flutter, Rust/WebAssembly,
  Gleam/BEAM, and mobile-background targets and owns the only engine gitlink.
- Component-specific clean-room and fault-injection suites stay in their owning
  client/E2E repositories; this repository verifies their immutable source set
  instead of recursively rediscovering and running unbuilt component tests.
- CI initializes submodules recursively, verifies the lock and safety policy,
  runs composition tests, and proves `.zpkg.toml` publication is disabled.
