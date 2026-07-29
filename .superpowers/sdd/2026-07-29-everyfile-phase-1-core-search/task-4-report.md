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

---

## Fix round 1/5: Important findings

### Scope

Fixed all three Important review findings:

1. Windows placeholder and directory-reparse classification now happens from
   `ignore`/`walkdir` directory-enumeration metadata before any candidate path is opened.
2. The unused opener plugin and renderer path capability were removed completely.
3. `DiscoveryStream` is now a genuinely incremental, bounded, cancellation-aware
   iterator backed by an owned worker and a 32-item synchronous channel.

Folder registration, encrypted database behavior, sidebar behavior, and
`FolderRecord` serialization were not changed.

### RED evidence

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --lib folders::discovery::tests
```

Exit code: `1`

Expected failure:

```text
error[E0432]: unresolved imports
`super::candidate_from_snapshot`,
`super::classify_snapshot`,
`super::spawn_discovery_worker`,
`super::DiscoveryEntryKind`,
`super::DiscoveryEntrySnapshot`,
`super::EntryClassification`,
`super::PathOpenProvider`,
`super::DISCOVERY_CHANNEL_CAPACITY`
```

This RED covers the missing instrumented placeholder/reparse boundary and the missing
bounded incremental worker.

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery renderer_capability_does_not_grant_generic_path_operations
```

Exit code: `1`

Expected behavioral failure:

```text
renderer capability permits a generic path operation: opener:default
test result: FAILED. 0 passed; 1 failed
```

### GREEN evidence

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --lib folders::discovery::tests
```

Output:

```text
running 4 tests
test folders::discovery::tests::directory_reparse_snapshot_is_pruned_before_path_open ... ok
test folders::discovery::tests::metadata_only_snapshot_never_invokes_path_open_provider ... ok
test folders::discovery::tests::dropping_stream_cancels_a_bounded_producer ... ok
test folders::discovery::tests::first_candidate_is_observable_while_traversal_is_blocked ... ok
test result: ok. 4 passed; 0 failed
```

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery renderer_capability_does_not_grant_generic_path_operations
```

Output:

```text
running 1 test
test renderer_capability_does_not_grant_generic_path_operations ... ok
test result: ok. 1 passed; 0 failed
```

Command:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery
```

Output:

```text
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored
```

### Full verification

Commands:

```powershell
cargo fmt --manifest-path src-tauri\Cargo.toml -- --check
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri\Cargo.toml
npm test -- --run src/features/folders/FolderSidebar.test.tsx
npm test -- --run
npm run build
git diff --check
```

Output:

```text
cargo fmt: exit 0
cargo clippy: exit 0, Finished `dev` profile
cargo test:
  discovery unit tests 4 passed
  domain contracts 2 passed
  encrypted database 6 passed
  folder discovery 10 passed
  0 failed
focused FolderSidebar: 1 file, 3 tests passed
full Vitest: 3 files, 6 tests passed
npm build: TypeScript + Vite exit 0, 16 modules transformed
git diff --check: exit 0
```

The encrypted-database wrong-key test continues to emit its expected SQLCipher HMAC
diagnostic while passing.

Command:

```powershell
rg -n "opener" package.json package-lock.json src-tauri\Cargo.toml src-tauri\Cargo.lock src-tauri\src src-tauri\capabilities
```

Output:

```text
NO_OPENER_REFERENCES
```

### Files changed in fix round 1/5

- `src-tauri/src/folders/discovery.rs`
- `src-tauri/tests/folder_discovery.rs`
- `src-tauri/capabilities/default.json`
- `src-tauri/src/lib.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `package.json`
- `package-lock.json`
- `.superpowers/sdd/2026-07-29-everyfile-phase-1-core-search/task-4-report.md`

### Safety and implementation decisions

- On Windows, `walkdir::DirEntry::metadata` is cached from directory enumeration. The
  snapshot classifier reads those attributes before any candidate path provider can be
  invoked.
- Directory symlinks and entries with `FILE_ATTRIBUTE_REPARSE_POINT` are rejected in
  `filter_entry`, before traversal can descend into them.
- Offline, recall-on-open, and recall-on-data-access files produce metadata-only
  candidates by joining the already-canonical parent and enumerated filename. They do
  not invoke the path-open/canonicalization provider.
- Only confirmed ordinary hydrated files invoke canonicalization, after which the
  canonical root containment check remains mandatory.
- Discovery uses a `sync_channel` with capacity 32. The producer uses cancel-aware
  bounded sends; dropping the receiver sets cancellation and stops a producer waiting
  on a full channel.
- Candidates and warnings are emitted as traversal proceeds. `discover_all` remains a
  compatibility helper that explicitly drains the stream into a report.
- Removed `tauri-plugin-opener`, `@tauri-apps/plugin-opener`, Rust initialization, and
  `opener:default`. The renderer retains only `core:default`; no generic filesystem,
  shell, dialog, or opener capability replaced it.

### Fix-round self-review

- Confirmed no `symlink_metadata` or path `metadata` call remains in candidate
  classification.
- Confirmed the metadata-only branch is guarded by a panic-on-use path provider test.
- Confirmed the real external-directory-link integration test still rejects escape.
- Confirmed the incremental test observes the first item while the producer is blocked
  before completion.
- Confirmed drop cancellation terminates a producer and cannot buffer more than the
  configured channel capacity.
- Confirmed no opener reference remains in either lockfile or source/config.
- Confirmed native-picker-only registration and no-path Tauri command signatures are
  unchanged.

### Remaining concerns/deferred minor coverage

- A real OneDrive Files On-Demand placeholder is not created in CI; the instrumented
  branch test proves the classified placeholder path is never opened/canonicalized.
- Representative Windows junction and ACL-denied integration fixtures remain deferred
  minor coverage. Reparse classification and real directory-symlink escape coverage
  exercise the applicable safety branches.
- `npm install --package-lock-only --ignore-scripts` reported the existing jsdom engine
  warning because local Node `24.13.1` is below jsdom's declared `24.15.0` minimum;
  focused/full Vitest and the production build all passed on the current runtime.
