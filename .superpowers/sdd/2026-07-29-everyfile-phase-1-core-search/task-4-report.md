# Task 4 Report: Register Folders and Discover Files Safely

## Status

Implemented folder persistence, picker-only Tauri commands, safe file discovery, and
the registered-folder sidebar. No indexing, parsing, source-file mutation, frozen-copy
changes, or plan/design changes were made.

## TDD evidence

### RED 1: discovery and repository contracts

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery
```

Exit code: `1`

Expected failure:

```text
error[E0433]: failed to resolve: could not find `folders` in `everyfile_lib`
```

### RED 2: sidebar contract

Command:

```powershell
npm test -- --run src/features/folders/FolderSidebar.test.tsx
```

Exit code: `1`

Expected failure:

```text
FAIL  src/features/folders/FolderSidebar.test.tsx
Error: Failed to resolve import "./FolderSidebar"
```

### RED 3: picker cancellation

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery cancelled_folder_selection_is_a_successful_no_op
```

Exit code: `1`

Expected failure:

```text
error[E0432]: unresolved import
`everyfile_lib::application::commands::register_selected_folder`
```

GREEN output:

```text
running 1 test
test cancelled_folder_selection_is_a_successful_no_op ... ok
test result: ok. 1 passed; 0 failed
```

### RED 4: registered-folder discovery stream

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery registered_folder_discovery_stream_yields_file_metadata
```

Exit code: `1`

Expected failure:

```text
error[E0432]: unresolved import `everyfile_lib::folders::discovery::discover`
```

GREEN output:

```text
running 1 test
test registered_folder_discovery_stream_yields_file_metadata ... ok
test result: ok. 1 passed; 0 failed
```

### RED 5: generic ignore-file regression

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery discovery_does_not_apply_repository_ignore_files
```

Exit code: `1`

Expected behavioral failure:

```text
left: [".ignore", "visible.txt"]
right: [".ignore", "hidden-by-rule.txt", "visible.txt"]
```

GREEN output after explicitly disabling `.ignore` processing:

```text
running 1 test
test discovery_does_not_apply_repository_ignore_files ... ok
test result: ok. 1 passed; 0 failed
```

## Final verification evidence

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery
```

Output:

```text
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored
```

Command:

```powershell
npm test -- --run src/features/folders/FolderSidebar.test.tsx
```

Output:

```text
Test Files  1 passed (1)
Tests  3 passed (3)
```

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml
```

Output:

```text
domain_contracts: 2 passed
encrypted_database: 6 passed
folder_discovery: 9 passed
all test results: 0 failed
```

The wrong-key encryption test intentionally emits SQLCipher HMAC/decryption diagnostics
to stderr while passing.

Command:

```powershell
npm test -- --run
```

Output:

```text
Test Files  3 passed (3)
Tests  6 passed (6)
```

Commands:

```powershell
cargo fmt --manifest-path src-tauri\Cargo.toml -- --check
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets --all-features -- -D warnings
npm run build
git diff --check
```

Output:

```text
cargo fmt: exit 0
cargo clippy: exit 0, Finished `dev` profile
npm build: TypeScript and Vite exit 0, 16 modules transformed
git diff --check: exit 0
```

## Files

- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/src/folders/mod.rs`
- `src-tauri/src/folders/discovery.rs`
- `src-tauri/src/folders/repository.rs`
- `src-tauri/src/application/commands.rs`
- `src-tauri/src/state.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/tests/folder_discovery.rs`
- `src/features/folders/FolderSidebar.tsx`
- `src/features/folders/FolderSidebar.test.tsx`
- `.superpowers/sdd/2026-07-29-everyfile-phase-1-core-search/task-4-report.md`

## Safety decisions

- The webview `register_folder` command has no path parameter. It opens the native
  Rust `tauri-plugin-dialog` folder picker and only passes the returned desktop path
  to `FolderRepository::register`.
- Picker cancellation returns successful `None`; it neither registers a path nor
  returns an error.
- Startup obtains the app data directory, loads/creates the DPAPI-protected key, opens
  and migrates the encrypted database, then exposes `AppState` with `Arc<Database>` and
  `FolderRepository`.
- `tauri-plugin-dialog` is pinned exactly to `2.7.2`. No frontend dialog/filesystem/shell
  capability was granted and `src-tauri/capabilities/default.json` was unchanged.
- Registration canonicalizes and persists the selected directory. SQLite uniqueness
  rejects a second canonical root with `FOLDER_ALREADY_REGISTERED`.
- Discovery canonicalizes the root once, walks with hidden files enabled and link
  following disabled, prunes directory reparse points (including junctions), and
  canonical-containment-checks candidates.
- `.git`, `node_modules`, and `$RECYCLE.BIN` are excluded by directory name. Generic
  `.ignore`/Git ignore files are explicitly not applied, so they cannot silently broaden
  the exclusion scope.
- Discovery reads metadata only and never opens file bodies. Windows offline,
  recall-on-open, and recall-on-data-access attributes mark a candidate `metadata_only`.
  Those candidates resolve through the canonical parent rather than canonicalizing the
  placeholder file, avoiding recall-on-open hydration.
- Entry metadata/walk/canonicalization failures become structured warnings and iteration
  continues. A missing or invalid registered root remains a fatal discovery setup error.
- Folder removal deletes FTS rows explicitly and relies on foreign-key cascades for the
  remaining folder-owned index rows. It performs no filesystem delete/write operation;
  the source-file-preservation test confirms the source remains.
- `FolderRecord` and its camel-case serialization were not changed. The earlier deferred
  broader DTO drift-test work remains deferred and was not silently expanded here.

## Self-review

- Confirmed the Tauri command cannot deserialize an arbitrary frontend path.
- Confirmed native dialog use required no webview capability change.
- Confirmed directory symlink/reparse traversal is filtered before descent and candidate
  canonical paths are checked against the registered root.
- Confirmed discovery contains no file-content reads or indexing/parsing calls.
- Confirmed removing a folder cannot address a source filesystem path.
- Confirmed empty sidebar state exposes exactly one `폴더 선택` action and states that
  only selected folders are indexed.
- Mutation check coverage: removing link pruning, exclusions, canonical duplicate
  handling, cancellation no-op, document/FTS cleanup, count projection, metadata-only
  flags, `.ignore(false)`, or sidebar actions breaks at least one test.

## Concerns

- Cloud-placeholder behavior is covered through Windows attribute semantics and a
  no-body-read implementation, but an actual OneDrive Files On-Demand placeholder is
  not created in the automated fixture.
- The link fixture exercises a Windows directory symbolic link. Junctions share the
  reparse-point pruning branch but are not separately created by the test fixture.
