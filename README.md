# opto-sync-monorepo

Composition workspace for Opto Sync interfaces, library, and clients without
duplicate engine ownership.

## Status

Bootstrap repository. It contains no vendored implementation and publishes no
package. Composition will be enabled only after the independent package
contracts and immutable locks exist.

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

## First implementation gates

- Add immutable component coordinates and a resolver-generated lock; do not
  fabricate lock state.
- Verify repository identity, commit ancestry, package identity, and engine
  uniqueness before build or test execution.
- Run clean-room TypeScript, Dart/Flutter, Rust/WebAssembly, Gleam/BEAM, and
  mobile background-lifecycle consumers.
- Exercise IndexedDB, SQLite, PostgreSQL, Supabase, HTTP, WebSocket, and TCP
  convergence with deterministic fault injection and bounded evidence.
- Record release-set provenance and deterministic archive digests while keeping
  publication disabled until every declared target is independently certified.
