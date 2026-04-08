# dust-bootstrap

`dust-bootstrap` is the Rust-based bootstrap compiler for the Dust Programming Language (DPL).

This repository exists to provide a stable, trusted compiler while the primary compiler/runtime stack and dependency ecosystem are rewritten in Dust.

## Purpose

`dust-bootstrap` is used to:

- build and validate Dust-native replacements
- debug regressions during Rust-to-Dust migration
- provide a deterministic baseline for output and diagnostics parity
- continuously compile migrated `dustsoftware/*` dependencies during licensing-preserving port work

## Role In Migration

Use this repository as the migration anchor:

1. Keep `dust-bootstrap` buildable at all times.
2. Port dependencies and toolchain components to Dust in other repos/workspaces.
3. Compile those ports with `dust-bootstrap`.
4. Compare behavior/artifacts against `dust-bootstrap` until parity is reached.

## Policy

`dust-bootstrap` should remain conservative:

- no speculative language feature work
- prioritize correctness, determinism, and debuggability
- accept only fixes that improve bootstrap reliability or unblock migration

## Quickstart

From this repository root:

```bash
cargo run -p dust -- --help
```

Build a Dust source program:

```bash
cargo run -p dust -- build examples/K/k_hello_world.ds
```

Check/validate source:

```bash
cargo run -p dust -- check examples/K/k_hello_world.ds
```

Emit object output:

```bash
cargo run -p dust -- obj examples/K/k_hello_world.ds -o target/dust/k_hello_world.o
```

## Recommended Bootstrap Workflow

For each migrated component (compiler module or `dustsoftware` repo):

1. Build with `dust-bootstrap`.
2. Run component tests/conformance checks.
3. Compare diagnostics and outputs against baseline snapshots.
4. Fix mismatches before advancing migration stage.

## Repository Layout

- `crates/` - Rust implementation of the bootstrap toolchain
- `docs/` - compiler internals and operational references
- `spec/` - DPL specification snapshots and related docs
- `examples/` - sample Dust programs

## Licensing And Provenance

The migration preserves licensing and provenance by maintaining repository-level lineage and explicit source attribution for dependency ports.

See `LICENSE` and per-repo metadata in the `dustsoftware` workspace for details.

## Status

Bootstrap status is tracked through branch/CI health and migration parity reports in project planning docs.

