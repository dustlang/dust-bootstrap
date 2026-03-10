# Changelog - Dust Compiler

All notable changes to the Dust compiler are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - Unreleased (DPL v0.2)

### Added

#### Frontend Enhancements
- Enhanced parser for K Regime v0.2 syntax
- Variable declarations (`let`, `mut let`)
- Control flow parsing (`if/else`, `for`, `while`, `break`, `continue`)
- Match expression parsing (`match`, `_`, `=>`)
- Function definition parsing with parameters and return types
- Structure definition parsing
- Memory operation parsing (`alloc`, `dealloc`, pointer operations)
- Added type-first parameter parsing (`K[Int] x`) in addition to name-first (`x: K[Int]`)
- Added assignment statement parsing (`x = expr;`)
- Added block tail-expression implicit return parsing
- Added `else if` parsing via nested-if lowering
- Fixed numeric lexing so range syntax (`0..n`) does not tokenize as float + dot
- Added match OR-pattern parsing (`a | b => ...`)
- Added struct literal parsing in postfix form (`Type { field: expr, ... }`)
- Added deterministic binary operator precedence parsing across logical/bitwise/comparison/shift/arithmetic levels
- Added `unsafe { ... }` block-expression parsing in primary expression handling
- Added generic type-form parsing (`Type<...>`) for v0.2 additions such as `Thread<type>`
- Restored `bind` declaration parsing, including contract block clause parsing and proc references
- Restored process `uses` clause parsing with named literal args
- Restored effect statement parsing across all effect keywords (`observe`, `emit`, `seal`)
- Added constraint/witness statement parsing (`constrain <expr>;`, `prove <ident> from <expr>;`)

#### Lexer Updates
- Float literal support (`3.14`)
- Char literal support (`'a'`)
- New keywords: `mut`, `if`, `else`, `for`, `while`, `break`, `continue`, `in`, `match`
- New tokens: `..` (range), `!` (not), `_` (underscore), `=>` (fat arrow)

#### AST Updates
- New statement types: `MutLet`, `If`, `For`, `While`, `Break`, `Continue`, `Expr`
- New expression types: `Unary`, `Index`, `Array`, `Block`, `Match`
- New pattern types: `MatchExpr`, `MatchArm`, `MatchPattern`
- Extended primitive types: Float, Char

#### Type System
- Extended type checking for all K Regime v0.2 types
- Pointer type validation
- Array and structure type checking
- Function signature type checking
- Type inference support
- Type environment (`TypeEnv`) implementation
- Type inferrer and checker
- Added semantic validation for mutable assignment rules (`mut let` required for reassignment)
- Added semantic validation for assignment to declared locals/params
- Added loop-control validation (`break`/`continue` must appear inside loop bodies)
- Added semantic validation that `constrain`/`prove` are restricted to Φ-regime processes
- Added witness-local registration for `prove` statements in semantic scope tracking
- Added semantic validation that Φ-regime effects require at least one witness-producing `prove`
- Added semantic validation that cross-regime local proc calls require an explicit `bind` between caller/callee

#### Memory Safety Analysis
- Use-after-free detection
- Buffer overflow prevention
- Dangling pointer elimination
- Proper alignment checking

#### Backend Improvements
- Enhanced x64 code generation
- Extended ELF object file generation
- Structure layout optimization
- Function call optimization
- Extended DIR IR with v0.2 statements
- New codegen framework (`v02_codegen.rs`)
- Host codegen now lowers struct-literal bindings into flattened field slots (`name.field`) and resolves matching field-read payloads
- Host codegen now handles direct struct literal expressions as deterministic opaque values in staged lowering
- Host codegen now accepts lowered block-expression placeholders (`{ ... }`) as deterministic zero-valued expressions (staged support for parsed `unsafe` block forms)
- Host codegen now treats unresolved identifier payloads that resolve to local K procedures as staged function-reference arguments (for v0.2 effect forms such as `spawn(worker, seed)`)
- Host runtime shim now exports staged v0.2 system-effect symbols (`alloc`, `free`, `spawn`, `join`, `mutex_*`, `io_*`, `mmio_*`) to allow deterministic linking/build of effect-form programs
- Host codegen now stages `observe` and `seal` effect lowering by evaluating payload expressions while deferring host-side effect emission semantics
- Host codegen now stages `constrain` predicate lowering and materializes `prove` witness locals for deterministic host-path builds

#### Build System
- Project structure support (State.toml, sector directories)
- Multi-sector project compilation
- Dependency resolution
- Cross-sector optimization (LTO)

#### Runtime Support
- Heap allocator implementation (`runtime.rs`)
- String operations (alloc, concat, len)
- Error handling (panic, assert, unreachable)
- Type conversions (int<->float, char<->int)
- Array operations

#### File Format Support
- State.toml project configuration parsing
- Support for both `.ds` and `.dust` file extensions
- DPL project structure (sector directories)

#### Host Link and Toolchain Integration
- Host link resolution now prefers `dustlink` for general executable builds, with ordered `lld` fallbacks.
- Bootstrap exception for building `dustlink` itself to avoid recursive self-linking.
- Expanded host runtime shim coverage for Dust-built host tooling workflows (argv/fs/path/string/linker helper intrinsics).
- Host runtime shim linker parity extensions:
  - shared-symbol ingest for ELF dynsym, PE export tables, COFF external definitions, and Mach-O external definitions
  - block-aware linker-script statement splitting (brace/paren-aware) instead of naive split behavior
  - script parsing additions for `SECTIONS` output-address forms and `ENTRY(symbol)` required-symbol registration
  - script parsing additions for `SEARCH_DIR(=...)` sysroot-aware resolution and `INPUT` token handling for `-L`/`-l`
  - script parsing additions for expression evaluation (`ORIGIN/LENGTH/ADDR/LOADADDR/SIZEOF/ALIGN` + `+/-`) and `ASSERT(...)`
  - linker-script compatibility handling for `PHDRS` and `VERSION` blocks now validates block structure
  - script parsing additions for `SECTIONS ... AT(<expr>)` load-address capture
  - compatibility-flag state wiring for `--hash-style`, `--threads`, `--thread-count`, `--eh-frame-hdr`, fatal/color diagnostics toggles, print-gc toggles, and `--icf=*` mode
  - broader host CLI compatibility handling for ld/lld-style script/export flags (`--version-script`, `--dynamic-list`, `--trace-symbol`, `--print-map`, `--start-lib`, `--end-lib`)
  - broader host CLI compatibility handling for `lld-link` slash-option families (`/OUT`, `/ENTRY`, `/MACHINE`, `/LIBPATH`, `/DEFAULTLIB`, `/MAP`, `/DLL`, `/SUBSYSTEM`, `/OPT`, `/WX`) and common slash metadata options
  - dynamic-policy alias coverage for `--no-allow-shlib-undefined` and split-value parsing parity for `--dynamic-linker` / `--soname`
  - expanded `-z` option semantics in host runtime (`defs`/`undefs`) plus accepted compatibility tokens (`text`/`notext`/`origin`)
  - additional `lld-link` no-value compatibility acceptance (`/NOENTRY`, `/DYNAMICBASE`, `/NXCOMPAT`, `/LARGEADDRESSAWARE`)
  - target alias expansion to accept `aarch64`/`arm64` triples in host linker target parsing
  - COFF/Mach-O object-format probe and symbol-ingest acceptance for arm64 machine/cpu IDs
  - ELF writer flow now emits a complete executable during header/finalize stages (instead of ident-only priming), and output-section stream calls validate section-index bounds
  - PE and Mach-O host writers now emit sectionized images from alloc chunks rather than single synthetic text payload sections
  - parity-oriented relocation/machine support surfaced to Dust linker modules (including additional x86_64 relocation IDs and ELF `EM_AARCH64` acceptance in validator paths)
  - architecture-aware target identity is preserved end-to-end (`x86_64` vs `aarch64`) instead of collapsing all targets into x86_64 families
  - ELF/PE/Mach-O host writers now stamp architecture-correct machine/cpu fields based on resolved linker target
  - host runtime relocation pipeline is machine-aware per object via `host_linker_object_machine`
  - baseline AArch64 relocation handling is available in Dust linker relocation logic (`R_AARCH64_NONE`, `R_AARCH64_ABS64`, `R_AARCH64_ABS32`, `R_AARCH64_PREL32`)
  - `lld-link` compatibility toggles `/NOENTRY`, `/DYNAMICBASE`, `/NXCOMPAT`, and `/LARGEADDRESSAWARE` are state-wired into PE header emission behavior
  - host CLI compatibility coverage expanded with soft-compatibility families for common ld/lld/lld-link metadata/profiling options (`--warn-*`, `--time-trace*`, `--lto-*`, `/GUARD:*`, `/TIMESTAMP:*`, `/MERGE:*`, `/SECTION:*`, etc.)
  - linker-script expression evaluator now covers unary, multiplicative, shift, and bitwise operators in addition to existing additive/script builtins
  - linker-script parsing now supports direct symbol assignments (`SYMBOL = <expr>`) and rejects unknown directive heads instead of silently accepting them
  - linker-script `OUTPUT_FORMAT`, `TARGET`, and `OUTPUT_ARCH` directives now return invalid/unsupported errors for bad values instead of silently succeeding
  - host runtime shim now exposes/consumes parity state for `fatal-warnings`, `color-diagnostics`, `print-gc-sections`, `--dependency-file`, and `--emit-relocs`
  - ELF writer now consumes `--hash-style` state to emit hash-table dynamic tags (`DT_HASH`, `DT_GNU_HASH`) in generated ELF outputs
  - host object ingestion uses refined machine-aware relocation mapping for COFF and Mach-O relocation records
  - target alias parsing expanded for musl triples, Windows GNU triples, and `*-none[-elf]` bare-metal aliases used by Dust-built tools
  - host runtime shim / Dust linker parity coverage expanded for AArch64 ELF instruction relocations:
    - branch/literal/ADR/ADRP/ADD/LDST (including `LDST128`) bitfield patching
    - MOVW `UABS` / `SABS` / `PREL` families
    - starter TLS instruction-form relocation ID support (`TLSGD`, `TLSLD`, `TLSDESC`) with strict unsupported apply-path handling for non-implemented descriptor semantics
    - host-runtime-backed AArch64 TLS data relocation values for `TLS_DTPMOD`, `TLS_DTPREL`, and `TLS_TPREL` in non-shared links using deterministic TLS layout metadata
    - shared-link AArch64 TLS data relocation behavior now distinguishes models: `TLS_DTPREL` can resolve from TLS layout metadata, shared-link `TLS_TPREL` returns invalid relocation, and shared-link `TLS_DTPMOD` remains not yet implemented
    - AArch64 TLSLE/TLSLD low12 offset instruction relocations (`ADD`/`LDST64`/`LDST128`) now route through host-runtime TLS offset helpers in non-shared links
    - `R_AARCH64_TLSDESC_CALL` now applies as a validated `BLR` preserve relocation even while broader TLSDESC descriptor/GOT semantics remain incomplete
    - preparatory AArch64 TLS synthetic descriptor/GOT planning state + host helper ABI (`reserve`, `count`, `slot address`, `reloc value`) for staged descriptor-sequence parity implementation
    - host ELF writer synthetic AArch64 TLS descriptor/GOT-like slot-region materialization plus minimal synthetic `.rela.dyn` metadata emission (`DT_SYMTAB`, `DT_SYMENT`, `DT_RELA*`) for reserved descriptor-sequence slots
  - host runtime shared-object ingestion now returns `ERR_INVALID_FORMAT` for unknown/unsupported shared-object payloads instead of silently succeeding
  - host runtime shared-object symbol ingest now validates target/ABI compatibility and shared-file kind before symbol ingestion (`ELF ET_DYN`, Windows PE DLL/COFF machine, Mach-O dylib CPU type)
  - host runtime shared-object symbol ingest now filters non-exported metadata entries more strictly (ELF hidden/internal dynsyms; Mach-O private extern / debug-symbol entries)
- Dust linker AArch64 TLS descriptor-sequence instruction relocation application now routes through the host synthetic-slot reloc-value helper and patches against reserved/materialized synthetic slot addresses (staged semantics; full TLSDESC runtime parity still incomplete)
- TLS descriptor-sequence staged relaxation behavior now includes deterministic synthetic-slot reuse/coalescing (including TLSLD module-slot coalescing in emitted synthetic slot metadata), while full instruction-sequence rewrite relaxations remain deferred
  - host runtime needed-library recording now prefers embedded shared-library names (`DT_SONAME` / PE export DLL name / Mach-O install name) when present

### Changed

- Minimum Rust version updated to support new features
- Default build output directory structure updated
- Error messages improved for new syntax
- `dust kernel-link` is explicitly deprecated in favor of `dust obj` + `dustlink` workflow guidance.

### Fixed

- Memory safety issues in generated code
- Type inference edge cases
- Code generation for complex control flow
- Host linker-script runtime no longer silently accepts unknown linker-script directives.
- Host linker compatibility/no-op flag handling now surfaces diagnostics instead of silent acceptance.
- Dust-built linker relocation parsing/validation now accepts and processes a broader AArch64 ELF relocation set (including MOVW and TLS starter forms) instead of rejecting them during ingest.
- Dust-built linker relocation apply path no longer blanket-rejects all AArch64 TLS instruction-family relocations; TLSLE/TLSLD low12 offset forms now apply in non-shared links via host TLS offset helpers.
- Dust-built linker relocation apply path now permits `R_AARCH64_TLSDESC_CALL` (validated `BLR` preserve) instead of rejecting it with the remaining unsupported TLSDESC descriptor-sequence relocs.
- Dust-built linker relocation apply path no longer stops at a blanket unsupported gate for remaining AArch64 TLS descriptor-sequence instruction relocs; they now patch against host-planned synthetic slot addresses via the synthetic-slot reloc-value helper.
- Dust-built linker/runtime now exposes an explicit AArch64 TLS synthetic descriptor/GOT planning ABI for upcoming descriptor-sequence parity work instead of relying on implicit future state.
- Dust-built host ELF outputs now materialize reserved AArch64 TLS synthetic descriptor/GOT slots and emit minimal synthetic `.rela.dyn` metadata (`DT_SYMTAB`, `DT_SYMENT`, `DT_RELA*`) instead of leaving descriptor-sequence parity work entirely in planning state.
- Host shared-object symbol ingest no longer accepts cross-target or wrong-kind binaries as valid shared inputs during symbol-ingest resolution.
- Host shared-object symbol ingest no longer treats hidden/private metadata symbols as exported shared symbols during ELF/Mach-O symbol-ingest fallback.
- Host needed-library emission no longer depends only on filename normalization when embedded shared library names are available.

## [0.1.0] - 2026-02-12

### Added

- Initial compiler implementation v0.1
- Basic K Regime parsing (`K main { ... }`)
- Emit effect handling
- Dust IR (DIR) generation
- Native executable generation (ELF/Mach-O/PE)
- Basic type checking
- Example programs in `examples/K/`

### Known Issues

- Limited to emit-only K Regime in v0.1
- No function definitions or user types
- No memory operations

---

Copyright © 2026 Dust LLC
