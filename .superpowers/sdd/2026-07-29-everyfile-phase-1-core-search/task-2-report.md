# Task 2 Report: Domain Contracts and Settings

## Implementation

- Added camel-case serde DTOs for folders, persisted documents, search, previews,
  indexing, and settings in `src-tauri/src/domain/models.rs`.
- Added `AppSettings::default()` with the requested EveryFile values: Korean/light,
  90-day retention, disabled startup/tray flags, 200 MiB maximum file size, and 100
  results per page.
- Added in-memory `AppState` with `RwLock<AppSettings>` and temporary
  `AtomicBool` database readiness, then registered typed `get_settings` and
  `save_settings` commands with the Tauri builder.
- Added TypeScript DTOs with camelCase field names and the five narrow invoke
  wrappers (`getSettings`, `saveSettings`, `listFolders`, `searchDocuments`, and
  `getPreview`). Tauri `invoke` is used only in `src/lib/ipc.ts`.

## Files Changed

- `src-tauri/src/lib.rs`
- `src-tauri/src/application/mod.rs`
- `src-tauri/src/application/commands.rs`
- `src-tauri/src/domain/mod.rs`
- `src-tauri/src/domain/models.rs`
- `src-tauri/src/settings.rs`
- `src-tauri/src/state.rs`
- `src-tauri/tests/domain_contracts.rs`
- `src/lib/types.ts`
- `src/lib/ipc.ts`
- `src/lib/ipc.test.ts`

## RED Evidence

1. `cargo test --manifest-path src-tauri\Cargo.toml --test domain_contracts`

   Result: failed as expected before implementation. `everyfile_lib::domain` and
   `everyfile_lib::settings` could not be resolved (`E0433` and `E0432`).

2. `npm test -- --run src/lib/ipc.test.ts`

   Result: failed as expected before implementation. Vitest could not resolve
   `./ipc` from `src/lib/ipc.test.ts`.

## GREEN Evidence

1. `cargo test --manifest-path src-tauri\Cargo.toml --test domain_contracts`

   Result: `2 passed; 0 failed`.

2. `npm test -- --run src/lib/ipc.test.ts`

   Result: `1 passed; 0 failed`.

## Full Verification

```text
cargo fmt --manifest-path src-tauri\Cargo.toml --all -- --check
# exit 0

npm run build
# tsc && vite build; built in 191ms

cargo test --manifest-path src-tauri\Cargo.toml
# 2 integration tests passed; no failures

npm test -- --run
# Test Files 2 passed (2); Tests 3 passed (3)

git diff --check
# no whitespace errors
```

## Self-Review

- `DocumentRecord` mirrors all columns from the planned `documents` migration,
  using optional strings for nullable columns and camelCase JSON serialization.
- Tauri command boundaries take and return concrete DTOs; no untyped JSON maps
  cross them.
- Wrapper tests assert the stable command names and camelCase payload property
  names. The Tauri invoke function is mocked only because the desktop runtime is
  external to Vitest; the test verifies this module's outbound boundary.
- The contract test includes `private_search` and `sort` while constructing
  `SearchRequest`, because both fields are required by the specified stable DTO.

## Concerns / Deferred Work

- `list_folders`, `search_documents`, and `get_preview` are deliberately typed
  frontend wrappers only; their backend commands arrive in later tasks.
- Settings are intentionally in-memory until Task 3 replaces the temporary
  readiness flag with database-backed services.
- `tauri-plugin-opener` remains initialized and granted but unused. This is the
  Task 1 ledger Minor and was intentionally not expanded in this task.
