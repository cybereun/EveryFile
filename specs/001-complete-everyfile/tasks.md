# Tasks: Complete EveryFile

**Input**: Design artifacts in `specs/001-complete-everyfile/`

**Rule**: A task is checked only after its focused tests pass. A user story is
checked only after its packaged independent test passes.

## Phase 1: Specification and Tracking

- [x] T001 Create project constitution in `.specify/memory/constitution.md`
- [x] T002 Create complete product specification in `specs/001-complete-everyfile/spec.md`
- [x] T003 Create architecture/research/contracts in `specs/001-complete-everyfile/`
- [x] T004 Create executable completion checklist in `specs/001-complete-everyfile/tasks.md`

## Phase 2: Foundational Release Protection

- [x] T005 Ignore generated agent-private files while retaining product specs in `.gitignore`
- [x] T006 Record a reproducible low-memory full-test command in `scripts/test-rust.ps1` and `.github/workflows/windows.yml`
- [x] T007 Add a packaged-app E2E harness and fixture-only registration boundary in `tests/e2e/`, `wdio.conf.ts`, and `src-tauri/src/application/commands.rs`
- [x] T008 Add a reusable visible command-error/status surface in `src/components/CommandStatus.tsx` and `src/styles/app.css`

**Checkpoint**: A disconnected production UI can no longer pass the release gate.

## Phase 3: User Story 1 - Register, Index, and Find a Folder (P1)

**Independent Test**: Clean packaged app → select fixture folder → folder appears
→ indexing completes → content-only phrase returns and opens the fixture.

- [x] T009 [P] [US1] Add failing production orchestration tests in `src/app/DesktopApp.test.tsx`
- [x] T010 [P] [US1] Add typed `registerFolder` and library-summary IPC contracts/tests in `src/lib/ipc.ts` and `src/lib/ipc.test.ts`
- [x] T011 [US1] Implement production folder/library state orchestration in `src/app/DesktopApp.tsx`
- [x] T012 [US1] Render `DesktopApp` instead of an unconnected shell in `src/main.tsx`
- [x] T013 [US1] Make folder callbacks required in production and add empty-panel add/remove actions in `src/app/App.tsx`
- [x] T014 [US1] Refresh folder counts and queue state from index events in `src/app/DesktopApp.tsx`
- [x] T015 [US1] Surface selection, registration, indexing, and refresh failures in `src/components/CommandStatus.tsx`
- [x] T016 [US1] Pass the packaged folder-index-search acceptance in `tests/e2e/folder-search.e2e.ts`

## Phase 4: User Story 2 - Control Workspace and Inspect Documents (P1)

**Independent Test**: Toggle and resize each panel independently, restart, then
preview a result and exercise all non-AI document actions.

- [x] T017 [P] [US2] Add panel visibility/persistence/responsive tests in `src/app/App.test.tsx`
- [x] T018 [P] [US2] Add accessible left/right toggle icon tests in `src/app/Header.test.tsx`
- [x] T019 [US2] Persist independent left/right visibility preferences in `src/app/App.tsx`
- [x] T020 [US2] Add explicit header controls and keyboard shortcuts for both panels in `src/app/Header.tsx`
- [x] T021 [US2] Add visible folder-add action inside the empty left panel in `src/app/App.tsx`
- [x] T022 [US2] Preserve user preference when responsive layout temporarily hides a panel in `src/app/App.tsx`
- [x] T023 [US2] Close remaining preview toolbar/dialog keyboard gaps in `src/features/preview/`
- [x] T024 [US2] Pass packaged panel/restart/preview acceptance in `tests/e2e/workspace-preview.e2e.ts`

## Phase 5: User Story 3 - Search Scans with Local OCR (P2)

**Independent Test**: With a network sentinel active, index text PDF, scanned
PDF, JPG, PNG, WebP, BMP, and TIFF fixtures; verify skip/recognition behavior.

- [x] T025 [P] [US3] Pin OCR runtime/model licenses and manifests in `sidecar/ocr-host/` and `THIRD_PARTY_NOTICES.md`
- [x] T026 [P] [US3] Add OCR settings/model state migration in `src-tauri/migrations/0008_ocr.sql`
- [x] T027 [P] [US3] Add OCR eligibility and protocol contract tests in `src-tauri/tests/ocr_flow.rs`
- [x] T028 [US3] Implement bounded local OCR sidecar protocol in `sidecar/ocr-host/`
- [x] T029 [US3] Build GUI-subsystem OCR executable and verify model hashes in `scripts/build-ocr-sidecar.ps1`
- [x] T030 [US3] Implement embedded-text quality and scan eligibility in `src-tauri/src/ocr/eligibility.rs`
- [x] T031 [US3] Integrate OCR attempt ownership/cancellation into indexing in `src-tauri/src/indexing/coordinator.rs`
- [x] T032 [US3] Implement OCR and separate math-OCR settings/warnings in `src/features/settings/SearchSettings.tsx`
- [x] T033 [US3] Add zero-egress, timeout, crash, and resource-bound tests in `src-tauri/tests/ocr_privacy.rs`
- [ ] T034 [US3] Pass packaged OCR acceptance for every supported format in `tests/e2e/ocr.spec.ts`

## Phase 6: User Story 4 - Choose and Use AI Explicitly (P2)

**Independent Test**: AI controls are absent while disabled; each provider
passes a mock stream contract, consent, summary/question, and cancellation test.

- [x] T035 [P] [US4] Add AI settings/request migrations in `src-tauri/migrations/0009_ai.sql`
- [x] T036 [P] [US4] Add protected provider-secret storage tests in `src-tauri/tests/ai_secrets.rs`
- [x] T037 [P] [US4] Add provider contract fixtures/tests in `src-tauri/tests/ai_providers.rs`
- [x] T038 [US4] Implement endpoint policy, consent, cancellation, and secret storage in `src-tauri/src/ai/`
- [x] T039 [US4] Implement Ollama native chat/model adapter in `src-tauri/src/ai/ollama.rs`
- [x] T040 [US4] Implement Gemini streaming adapter in `src-tauri/src/ai/gemini.rs`
- [x] T041 [US4] Implement OpenAI streaming Responses adapter in `src-tauri/src/ai/openai.rs`
- [x] T042 [US4] Implement bounded local chunk retrieval and cited prompts in `src-tauri/src/ai/retrieval.rs`
- [x] T043 [US4] Add AI activation/provider/connection settings in `src/features/settings/AiSettings.tsx`
- [x] T044 [US4] Add cancellable summary and document-question UI in `src/features/preview/DocumentAiPanel.tsx`
- [ ] T045 [US4] Pass disabled/local/remote packaged AI acceptance in `tests/e2e/ai.spec.ts`

## Phase 7: User Story 5 - Understand and Manage the Library (P2)

**Independent Test**: Create normal/private history, bookmark, note, and tags;
restart, filter statistics, export, reset, and compare source fixture hashes.

- [x] T046 [P] [US5] Audit and expand production statistics/history tests in `src/features/statistics/StatisticsDialog.test.tsx`
- [x] T047 [P] [US5] Add restart persistence/cascade fixtures in `src-tauri/tests/library_actions.rs`
- [x] T048 [US5] Connect statistics filters and history queries to production folder state in `src/app/App.tsx`
- [x] T049 [US5] Complete accessible charts/tables, deletion, and private-search states in `src/features/statistics/`
- [x] T050 [US5] Verify CSV/XLSX/Markdown exports and source identity locks in `src-tauri/tests/statistics_and_export.rs`
- [ ] T051 [US5] Pass packaged persistence/private/reset/source-integrity acceptance in `tests/e2e/library-management.spec.ts`

## Phase 8: User Story 6 - Install or Run Portably (P1)

**Independent Test**: Complete all P1 journeys in installer and portable builds
on a clean Windows profile with the correct icon and no console.

- [ ] T052 [P] [US6] Add parser/OCR/app GUI-subsystem and icon resource assertions in `scripts/verify-no-console.ps1`
- [ ] T053 [P] [US6] Add dependency/license/model completeness checks in `scripts/build-portable.ps1`
- [ ] T054 [US6] Run frontend build/tests and low-memory Rust format/Clippy/tests via `scripts/release-gate.ps1`
- [ ] T055 [US6] Run zero-egress privacy and cloud-placeholder gates via `scripts/release-gate.ps1`
- [ ] T056 [US6] Build and smoke installer and portable distributions via `scripts/release-windows.ps1`
- [ ] T057 [US6] Complete a second convergence audit against `spec.md` in `specs/001-complete-everyfile/convergence.md`
- [ ] T058 [US6] Stage installer, portable ZIP, checksums, notices, and release notes in `artifacts/release/`
- [ ] T059 [US6] Pass the clean-profile installer and portable independent test in `tests/e2e/release.spec.ts`

## Final Phase: Polish and Cross-Cutting Concerns

- [ ] T060 [P] Complete Korean/English strings and remove mojibake in `src/app/translations.ts` and `src/`
- [ ] T061 [P] Update user/developer/privacy/OCR/AI documentation in `README.md`
- [ ] T062 Verify `L:\codex-L\Everyfile-copy` remains unchanged and clean-room evidence is recorded in `specs/001-complete-everyfile/convergence.md`
- [ ] T063 Review all deferred minor accessibility/data-migration notes from `.superpowers/sdd/2026-07-29-everyfile-phase-1-core-search/progress.md`
- [ ] T064 Complete the final accessibility/localization audit in `specs/001-complete-everyfile/convergence.md`
- [ ] T065 Re-verify every detailed search operator/filter/sort/paging/preset contract in `src-tauri/tests/search_queries.rs` and `src/features/search/SearchWorkspace.test.tsx`
- [ ] T066 Re-verify every promised parser format and notice with fixtures in `src-tauri/tests/parser_sidecar.rs`
- [ ] T067 Re-verify encrypted storage, cloud-placeholder avoidance, and zero-egress core behavior in `src-tauri/tests/encrypted_database.rs` and `src-tauri/tests/folder_discovery.rs`
- [ ] T068 Run the 10,000-file filename and 100,000-document search performance gates in `src-tauri/tests/performance.rs`
- [ ] T069 Tag the tested revision and publish installer, portable ZIP, checksums, and notices to GitHub Release
- [ ] T070 Confirm no unchecked task remains in `specs/001-complete-everyfile/tasks.md` and provide final run/install instructions

## Dependencies and Execution Order

1. Phase 2 blocks every story.
2. US1 blocks US2 packaged preview, US3 index integration, US5 production
   statistics, and all release work.
3. US2 can complete after US1; US3 and US4 are independent after the foundation.
4. US5 converges existing library features after US1 production state exists.
5. US6 and polish require all selected stories; T069 release publication requires
   T001-T068 complete.

## Parallel Opportunities

- US1 tests/IPC contract can be written independently before orchestration.
- Panel UI tests and header tests are independent.
- OCR licensing/migration/contracts are independent until sidecar integration.
- AI migration/secrets/provider contracts are independent before adapters.
- Statistics frontend and Rust persistence audits are independent.

## Implementation Strategy

First deliver and locally validate US1 plus US2 as an unreleased `v1.0.1`
hotfix, because they make the current application usable. Do not publish that
build as “complete.” Continue through OCR, AI, convergence, and the full release
gate; publish the next stable version only when T059 is checked.
