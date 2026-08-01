# Implementation Plan: Fast Indexing and Refined Sidebar

**Branch**: `codex/phase-1-core-search` | **Date**: 2026-07-31 |
**Spec**: [spec.md](spec.md)

## Summary

Remove the O(n²) status-error payload and unsupported-file parser work, then use
the existing persistent parser host at its safe three-request capacity. Move the
index UI into the full-width footer and add a terminal report. Refactor the left
pane into small reusable sections while preserving current folder actions,
history behavior, panel persistence, privacy, and EveryFile styling.

## Technical Approach

- Rust coordinator classifies extensions before extraction.
- Unsupported/image-without-OCR candidates use the existing metadata-only
  transaction and count as completed successes.
- A bounded `JoinSet` schedules up to three standard parsing tasks. OCR remains
  serialized by the existing OCR client mutex.
- `load_status` queries an exact error count and fetches details only for terminal
  states, capped at 100 ordered rows.
- TypeScript contracts gain `errorCount` with backward-compatible normalization.
- The status controller owns the footer bar and one-shot terminal dialog.
- Sidebar sections use semantic buttons, compact badges, empty states, existing
  search-history IPC, and current add/remove callbacks.

## Verification

1. Focused Rust coordinator/model tests.
2. Focused React status/sidebar tests and full Vitest suite.
3. Rust integration suite with low-memory settings.
4. Production build, installer/portable smoke, then release replacement only
   after clean packaged acceptance.

## Constitution Check

- Local indexing/OCR remains zero-egress.
- Unsupported data is never uploaded or silently downloaded.
- AI behavior is unchanged and stays explicitly opt-in.
- User-visible cancel/pause and failure explanations remain available.
- Windows release artifacts will not be replaced until verified.

