# Task 5 Report: Integrate the Pinned Kordoc Parser Sidecar

## Status

Implemented the pinned Kordoc integration as a persistent, validated NDJSON sidecar
with a typed Rust client and a self-contained hidden Windows executable. The renderer
retains only `core:default`; filesystem access and process creation remain inside the
Rust/sidecar privileged boundary.

Kordoc is pinned as a git submodule at:

```text
31ec46a0a55cfa92d37b4a5ad34f4a5de9db4133
https://github.com/cybereun/kordoc--.git
```

## TDD evidence

### RED 1: sidecar protocol

Command:

```powershell
npm test --prefix sidecar\parser-host -- --run
```

Exit code: `1`

Expected failure:

```text
FAIL test/protocol.test.ts
Error: Cannot find module '../src/protocol.js'
Test Files  1 failed (1)
Tests  no tests
```

### RED 2: Rust client boundary

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test parser_sidecar
```

Exit code: `1`

Expected failure:

```text
error[E0433]: failed to resolve: could not find `parsing` in `everyfile_lib`
```

### GREEN: focused parser verification

Commands:

```powershell
npm test --prefix sidecar\parser-host -- --run
npm run typecheck --prefix sidecar\parser-host
cargo test --manifest-path src-tauri\Cargo.toml --test parser_sidecar
```

Output:

```text
parser host:
  Test Files  1 passed (1)
  Tests  11 passed (11)
  typecheck exit 0

Rust sidecar integration:
  running 5 tests
  test result: ok. 5 passed; 0 failed
```

The focused suites cover strict request validation, one-of document/error responses,
typed error mapping, maximum input size, unsupported formats, timeout behavior,
three-request concurrency limiting, log redaction, real Kordoc HWPX/HWP5/PDF/XLSX/DOCX
parsing, out-of-order response matching, persistent-process reuse, one restart after
unexpected exit, Windows child flags, and the packaged executable.

## Build and packaged-artifact evidence

Command:

```powershell
node scripts\build-parser-sidecar.mjs
```

Result:

```text
clean Kordoc npm ci + build: exit 0
clean parser-host npm ci + bundle: exit 0
pkg target node22-win-x64: exit 0
output:
src-tauri\binaries\everyfile-parser-x86_64-pc-windows-msvc.exe
size: 105,987,693 bytes
PE subsystem: 2 (Windows GUI)
```

The build script uses the exact host target from `rustc -vV`, packages with
`--fallback-to-source`, changes the PE subsystem to Windows GUI, verifies the patched
header, and copies the target-triple filename required by Tauri.

Manual packaged-executable smoke evidence:

```text
synthetic HWPX: ok=true, plainTextLength=112
18-page PDF fixture: ok=true, plainTextLength=32800
```

The final Rust integration test independently launches the rebuilt target-triple
executable, verifies GUI subsystem 2, and parses a synthetic HWPX successfully.

## Final regression evidence

Commands:

```powershell
npm test -- --run
npm run build
npm test --prefix vendor\kordoc
cargo fmt --manifest-path src-tauri\Cargo.toml -- --check
cargo test --manifest-path src-tauri\Cargo.toml
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets --all-features -- -D warnings
cargo build --manifest-path src-tauri\Cargo.toml
git diff --check
```

Output:

```text
root Vitest: 3 files, 6 tests passed
root TypeScript + Vite build: exit 0, 16 modules transformed
pinned Kordoc: 83 suites, 329 tests passed, 0 failed
cargo fmt: exit 0
cargo test:
  discovery unit tests 4 passed
  domain contracts 2 passed
  encrypted database 6 passed
  folder discovery 10 passed
  parser sidecar 5 passed
  0 failed
cargo clippy -D warnings: exit 0
cargo build: exit 0
git diff --check: exit 0
```

The encrypted-database wrong-key test continues to emit its expected SQLCipher HMAC
diagnostic while passing.

The root Vitest configuration now excludes the independent `sidecar/parser-host` and
`vendor/kordoc` projects while preserving `configDefaults.exclude`. Each independent
project is exercised explicitly by its own final command and environment.

## Files

- `.gitmodules`
- `vendor/kordoc` (gitlink)
- `.gitignore`
- `THIRD_PARTY_NOTICES.md`
- `scripts/build-parser-sidecar.mjs`
- `sidecar/parser-host/package.json`
- `sidecar/parser-host/package-lock.json`
- `sidecar/parser-host/tsconfig.json`
- `sidecar/parser-host/tsup.config.ts`
- `sidecar/parser-host/src/protocol.ts`
- `sidecar/parser-host/src/kordoc-adapter.ts`
- `sidecar/parser-host/src/main.ts`
- `sidecar/parser-host/test/protocol.test.ts`
- `src-tauri/src/parsing/mod.rs`
- `src-tauri/src/parsing/client.rs`
- `src-tauri/tests/parser_sidecar.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/tauri.conf.json`
- `src-tauri/capabilities/default.json`
- `vitest.config.ts`
- `.superpowers/sdd/2026-07-29-everyfile-phase-1-core-search/task-5-report.md`

## Protocol and safety decisions

- Requests are newline-delimited JSON and must match a strict Zod schema: non-empty
  `id`, operation `parse`, non-empty `path`, and an optional positive integer
  `maxBytes` capped at 500 MB.
- Responses are a discriminated union containing exactly one `document` or `error`.
  Error codes are narrow and typed across TypeScript and Rust.
- File size is checked before parsing. Requests time out, and the host admits at most
  three concurrent parses.
- Production diagnostics use a fixed stderr message. Protocol errors never expose
  source paths, stack traces, environment data, or raw exception messages. The Rust
  child also discards sidecar stderr.
- The adapter calls the pinned Kordoc `parse(path)` API, normalizes its markdown,
  blocks, metadata, and warnings, and strips binary image payloads before emitting
  JSON.
- The Rust client keeps one long-lived child, tags pending requests by process
  generation and request ID, matches out-of-order responses, applies a per-request
  timeout, and retries exactly once after unexpected process exit.
- Windows children use `CREATE_NO_WINDOW`. The packaged executable is patched and
  verified as GUI subsystem 2, preventing a console flash even when launched outside
  the normal parent.
- No Tauri shell plugin, generic filesystem permission, or renderer path/process API
  was added. `capabilities/default.json` still grants only `core:default`.
- `tauri.conf.json` bundles `binaries/everyfile-parser`; generated `.exe` files remain
  ignored rather than committed.
- Parser-host runtime and build dependencies are exact-version pinned. The build-time
  CFB compatibility rewrite operates on the in-memory pinned Kordoc bundle only; the
  submodule working tree remains unchanged.
- Optional Phase 3 OCR/formula dependencies are externalized and `formulaOcr` is
  disabled for this Phase 1 parser boundary.

## Licensing and pin verification

The Kordoc `LICENSE` and `NOTICE` sections in `THIRD_PARTY_NOTICES.md` were extracted
and compared after CRLF/LF normalization:

```text
license verbatim: true (1066 characters on both sides)
notice verbatim: true (4673 characters on both sides)
```

Final submodule verification:

```text
31ec46a0a55cfa92d37b4a5ad34f4a5de9db4133 vendor/kordoc (heads/main)
submodule git status: clean
```

## Self-review

- Confirmed malformed or adversarial request data cannot bypass schema validation.
- Confirmed each response preserves the caller ID and cannot contain both success and
  error payloads.
- Confirmed a deliberately injected secret is absent from stdout, stderr, and the
  returned error object.
- Confirmed the limiter never observes more than three active parse operations.
- Confirmed the Rust pending map handles responses arriving in reverse order.
- Confirmed timeout and crash paths terminate the affected generation and cannot leave
  stale requests attached to a replacement child.
- Confirmed only unexpected process exit is retried, and only once.
- Confirmed the generated executable has no committed artifact drift and is discoverable
  by Tauri under the exact host target filename.
- Confirmed the submodule is at the required SHA and has no local modifications after
  clean builds and tests.
- Confirmed removing strict validation, size checks, response exclusivity, redaction,
  concurrency limiting, timeout/restart behavior, hidden-process flags, or the packaged
  executable breaks at least one focused test.

## Concerns

- Clean installation reports 21 audit findings in the pinned upstream Kordoc dependency
  tree (5 low, 6 moderate, 10 high) and one low finding in the parser-host toolchain.
  Updating or auditing those transitive dependencies requires a separate upstream/pin
  decision; changing the required Kordoc commit was outside this task.
- `pkg` warns that intentionally absent optional Phase 3 OCR modules cannot be found.
  They are externalized and unreachable with `formulaOcr: false` in this integration.
- `pkg` ships two PDF.js ESM files as source because their top-level-await/export form
  cannot be bytecode-compiled. Both a real packaged PDF smoke test and the complete
  parser-host PDF test pass.
