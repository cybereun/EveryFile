# EveryFile v1.0.0 Convergence Audit

Date: 2026-07-31

Audited branch: `codex/phase-1-core-search`

## Requirement convergence

| Requirement group | Result | Evidence |
| --- | --- | --- |
| FR-001–006 folder/index lifecycle | Pass | `folder-search.e2e.ts`; indexing recovery, watcher, cancellation, and ownership Rust suites |
| FR-007 detailed search | Pass | `search_queries.rs`; `SearchWorkspace.test.tsx`; packaged folder search |
| FR-008 workspace panels | Pass | `App.test.tsx`; `Header.test.tsx`; `workspace-preview.e2e.ts` |
| FR-009–011 preview and formats | Pass | preview unit suites; `parser_sidecar.rs`; `workspace-preview.e2e.ts` |
| FR-012–015 local OCR | Pass | `ocr_flow.rs`; `ocr_privacy.rs`; all-format `ocr.e2e.ts` |
| FR-016–020 explicit AI | Pass | provider/secret/retrieval Rust suites; `ai.e2e.ts` |
| FR-021–024 library management | Pass | statistics/export/library persistence suites; `library-management.e2e.ts` |
| FR-025–029 privacy and safe failures | Pass | encrypted database, folder discovery, reset, zero-egress, and command-status suites |
| FR-030–031 Windows distributions | Pass | `release-windows.ps1`; PE subsystem/icon verification; clean-profile release acceptance |
| FR-032 clean-room boundary | Pass | Frozen comparison evidence below |

All six packaged application journeys passed together on Windows. The E2E-only
fixture registration bridge is feature-gated, disabled in production, and its
window is moved off-screen so test progress cannot be mistaken for a user job.

## Performance and privacy

- 10,000-document filename gate: passed under the two-second SC-003 limit.
- 100,000-document interactive gate: at least 19 of 20 searches passed under
  the one-second SC-004 limit.
- Core indexing, parsing, OCR, search, preview, and statistics network-sentinel
  suites: passed with zero outbound requests.
- Text PDFs skip OCR; scanned PDF, JPG, PNG, WebP, BMP, and TIFF fixtures become
  searchable using the packaged local OCR sidecar.
- Cloud-placeholder and reset/source-identity tests passed without hydration or
  source mutation.

## Accessibility and localization

- Icon-only controls have bilingual accessible names, dialogs expose headings,
  tabs use tab semantics, popup controls restore focus, and left/right resizers
  have keyboard controls.
- Buttons, tabs, filters, toolbar actions, and the bottom status bar use
  single-line labels with responsive font sizing and horizontal overflow where
  shrinking further would reduce readability.
- Korean is the complete primary interface. The English product tagline and
  bilingual assistive labels remain available; no invalid UTF-8 or replacement
  character is present in product source or packaged documentation.

## Deferred-note disposition

- Unused opener permission: removed.
- DTO/migration drift: covered by migration and stable contract suites.
- Full Perl prerequisite: documented in `README.md` and checked by the release
  script.
- Expected SQLCipher wrong-key diagnostics: test-only stderr, not a product
  failure.
- Real placeholder/junction/ACL behavior: conservative enumeration and
  reparse/placeholder guards are covered by Windows tests.
- Kordoc dependency findings: no confirmed reachable runtime exploit; pinned
  Kordoc license/notice and parser isolation are packaged.
- Search snippets: rendered as text with explicit highlight ranges, never as
  trusted document HTML.
- Popup/dialog/tab keyboard patterns: covered by preview and statistics tests.
- Case-duplicate tags and bookmark removal: normalized uniqueness and cascade
  persistence are covered by library-action tests.

## Frozen comparison evidence

The comparison repository `L:\codex-L\Everyfile-copy` was read only for behavior
analysis. Its recorded HEAD remains:

`ea1ec3e5b4caf6fec055ff07e52c940994478a1c`

Its `Everyfile-App/` untracked directory existed before this work and was not
created, copied, edited, staged, or removed by this project. No source file from
the comparison application was copied into this repository.

## Release decision

The final release is acceptable only when `scripts/release-windows.ps1`
finishes, installer and portable clean-profile acceptance passes, artifact
checksums match, the tested commit is pushed, and the GitHub `v1.0.0` release
contains the regenerated installer, portable ZIP, checksum, license, notices,
and release notes.
