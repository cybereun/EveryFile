# Validation Quickstart: Complete EveryFile

## Prerequisites

- Windows 10/11 x64
- Node 24, stable Rust with rustfmt/clippy, complete Perl runtime
- Repository submodules initialized
- Sufficient disk/page-file capacity for the OCR model and SQLCipher build

## Focused P1 validation

1. Run frontend and Rust focused tests for `DesktopApp`, folder IPC, and panels.
2. Build the parser sidecar and a test desktop executable.
3. From a clean app-data directory, launch the packaged app.
4. Click the visible folder-add action and select the fixture library.
5. Confirm the left panel updates, indexing begins, and a content-only phrase
   returns a previewable document.
6. Toggle and resize left/right panels independently; restart and verify state.

## OCR validation

Index a fixture set containing a normal text PDF, scanned PDF, and each supported
image format. Verify eligibility/skips, local recognition, cancellation, math
warning, and zero network sentinel connections.

## AI validation

With AI disabled, verify no AI controls. Enable each provider against a mock
endpoint, validate provider-specific payload/stream parsing and no fallback,
then run a selected-document summary and question with citations and cancel.

## Full release gate

Run UI tests/build, format, Clippy, constrained Rust tests, parser/OCR sidecar
tests, privacy sentinel, packaged E2E, installer and portable creation,
no-console/icon verification, and checksums. Repeat the P1 journey in both
distributions on a clean Windows user profile before tagging.
