# Search and UX Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make result refinement reliable, remove dead-end actions, improve legibility, and publish reviewed source without disrupting signed updates.

**Architecture:** Preserve the existing Tauri/React architecture. Add a backward-compatible optional `withinQuery` field to the search request and apply its literal substring predicate before SQL count and pagination. Keep the existing signed Tauri update chain and add static compatibility checks around its invariant configuration.

**Tech Stack:** React 19, TypeScript 7, Tauri 2, Rust 2021, SQLite FTS5/SQLCipher, Vitest, GitHub Actions

**Spec:** User-approved design stated in this task on 2026-09-08

## Global Constraints

- Keep `com.cybereun.everyfile`, the updater public key and the latest-release endpoint unchanged.
- Preserve signed download, verification, passive NSIS install and restart ordering.
- Do not publish a release tag or replace current v1.1.0 release assets in this source-only change.
- Do not add a database migration or new runtime dependency.

---

### Task 1: Correct result refinement

**Files:** `src/lib/types.ts`, `src/features/search/useImmediateSearch.ts`, `src/features/search/SearchWorkspace.tsx`, `src/features/search/SearchFilters.tsx`, `src-tauri/src/domain/models.rs`, `src-tauri/src/search/repository.rs`

**Interfaces:** Consume the existing `SearchRequest`; produce optional TypeScript `withinQuery` and serde-defaulted Rust `within_query`. The value is a literal string limited to 512 characters.

- [x] Add request contract tests for missing, empty, overlong and literal wildcard input.
- [x] Send trimmed refinement through the existing debounce and cancellation flow.
- [x] Apply refinement to filename, path, title and body before count, sort and pagination.
- [x] Clear stale rows and paging immediately when any search criterion changes.
- [x] Verify full-body, unloaded-result, folder-scope and paging behavior.

### Task 2: Remove dead ends and improve legibility

**Files:** `src/features/search/SearchResults.tsx`, `src/features/search/SearchResults.test.tsx`, `src/app/translations.ts`, `src/styles/app.css`, `src/styles/refresh.css`

**Interfaces:** Preserve open file, open location, copy path, keyboard navigation and preview selection behavior.

- [x] Remove inactive similar-document and selection-only comparison actions.
- [x] Remove their unused state, styles and translations.
- [x] Increase filter, extension badge, preview-control and result metadata text sizes.
- [x] Verify the context menu still exposes every working action.

### Task 3: Protect update compatibility and verification stability

**Files:** `src/lib/updateContract.test.ts`, `vitest.config.ts`, `.github/workflows/release.yml`, `.github/workflows/windows.yml`, `README.md`

**Interfaces:** Installed clients continue to read `releases/latest/download/latest.json` and verify the NSIS installer with the existing public key.

- [x] Assert identity, signing key, endpoint, versions, installer mode and restart hook invariants.
- [x] Fail tagged release jobs when the tag differs from the package version.
- [x] Keep all manifest, installer and signature assets in the release workflow.
- [x] Limit Vitest discovery and worker count so native build output and low virtual memory do not destabilize tests.
- [x] Update stale documentation and neutralize the CI artifact display name.

### Task 4: Validate and publish

**Files:** All files above

**Interfaces:** Produce a reviewable GitHub branch and pull request targeting `main`.

- [x] Run 100 frontend unit tests with one worker.
- [x] Run 21 Rust search tests, 2 contract tests and 3 performance tests.
- [x] Run the added 100,000-document refinement performance test.
- [x] Run the final production build, Clippy, formatting and diff checks.
- [ ] Commit, push `codex/search-ux-quality` and open the pull request.

## Follow-up after measurement

Result-list virtualization, a larger visual redesign, and new comparison or semantic-search features require separate measured requirements. This iteration does not publish an installer.

## Verification record

- `npm test -- --run --maxWorkers=1`: 20 files, 100 tests passed.
- Rust targeted suite: 26 tests passed, including existing and refined 100,000-document latency targets.
- `cargo clippy --offline --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`: passed.
- Sequential full Rust audit reached 78 additional passing tests before this PC exhausted system virtual memory while rustc mapped a 448 MB debug library; GitHub Windows CI is the final full-suite environment.
- `npm run build`, `cargo fmt -- --check`, and `git diff --check`: passed after all source edits.
