# Tasks: Fast Indexing and Refined Sidebar

## Phase 1: Specification and Baseline

- [x] T001 Record approved feature specification and plan.
- [x] T002 Add failing tests for status payload, unsupported classification, and bounded concurrency.
- [x] T003 Add failing UI tests for footer progress, report dialog, and sidebar sections.

## Phase 2: Fast and Safe Indexing

- [x] T004 Add parser/OCR eligibility classification and metadata-only completion.
- [x] T005 Replace sequential parsing with bounded three-request scheduling.
- [x] T006 Add exact `errorCount` and cap terminal error detail loading.
- [x] T007 Verify pause/resume/cancel/recovery and local privacy behavior.

## Phase 3: Progress and Completion Experience

- [x] T008 Move progress into the full-width application footer.
- [x] T009 Add one-line filename, percentage, and accessible controls.
- [x] T010 Add one-shot completion report with expandable errors.

## Phase 4: Refined Sidebar

- [x] T011 Refactor indexed-folder presentation and count badges.
- [x] T012 Add collapsible smart-folder, recent-search, and bookmark sections.
- [x] T013 Preserve add/remove/search actions and independent panel toggle behavior.
- [x] T014 Replace direct removal with a hover three-dot menu and confirmation.

## Phase 5: Original Layout and Fixed Application Chrome

- [x] T015 Add local PDF/HWP/HWPX original-layout rendering.
- [x] T016 Add yellow search highlighting to text and original layouts.
- [x] T017 Preserve table row/column spans in document text view.
- [x] T018 Fix header/footer to the viewport and isolate panel scroll containers.
- [x] T019 Upgrade document statistics with summary, donut, bars, and rankings.
- [x] T020 Add frequent/recent search history views with local-only disclosure.

## Phase 6: Convergence and Release

- [x] T021 Run full frontend and Rust test suites.
- [x] T022 Build and smoke-test silent Windows installer and portable package.
- [ ] T023 Commit, push, and replace release artifacts with verified checksums.
