# Feature Specification: Fast Indexing and Refined Sidebar

**Feature Branch**: `codex/phase-1-core-search`

**Created**: 2026-07-31

**Status**: Approved

**Input**: Make mixed-file indexing fast, present progress in a clean full-width
bottom bar, report exact success and failure totals at completion, and refine
the left sidebar into a polished, sectioned library navigator.

The application chrome must remain stable: the header and status footer stay
visible at all times while the center result list, right document preview, and
left library navigator scroll independently.

## User Scenarios & Testing

### User Story 1 - Index a Mixed Folder Quickly (Priority: P1)

A user adds a folder containing documents, images, executables, and other files.
Every file becomes searchable by name/path quickly, while only supported
documents enter the bounded text extraction/OCR pipeline.

**Independent Test**: Index a fixture containing supported documents,
unsupported binaries, images with OCR disabled, and failures; verify unsupported
files are registered without parser errors and supported parsing runs with the
configured bounded concurrency.

**Acceptance Scenarios**:

1. Unsupported files such as EXE are stored as metadata-only and never sent to
   the document parser.
2. Images are metadata-only when OCR is disabled and use local OCR when enabled.
3. Ordinary document parsing uses at most three concurrent requests; OCR remains
   locally serialized and cancellation/pause remain safe.
4. Active status events carry an exact error count without repeatedly loading or
   transmitting the full accumulated error list.

### User Story 2 - Understand Indexing Progress and Results (Priority: P1)

The user sees a compact full-width bottom bar with phase, processed/total,
current file, percentage, and controls. On completion, a result dialog reports
exact successes and failures and can expand a bounded failure detail list.

**Independent Test**: Run a mixed fixture job, observe accessible progress from
start to 100%, then verify the result dialog totals and failure details.

**Acceptance Scenarios**:

1. Progress is never rendered as a narrow panel inside the content workspace.
2. Long filenames truncate to one line and the percentage remains visible.
3. Completion opens one result dialog with exact success/failure counts.
4. Closing the report returns to the normal footer summary without losing data.

### User Story 3 - Navigate a Refined Sidebar (Priority: P2)

The user scans indexed folders, smart folders, recent searches, and bookmarks in
a calm, compact ivory sidebar matching EveryFile's coral identity.

**Independent Test**: Render empty and populated sidebar states at common window
sizes, collapse each section, run a recent search, and confirm all labels remain
single-line or intentionally wrapped helper text.

**Acceptance Scenarios**:

1. Indexed folders show icons and compact count badges with a clear add action.
2. Smart folders, recent searches, and bookmarks have section headers and useful
   empty states rather than raw controls.
3. The existing independent sidebar open/close behavior remains intact.
4. An indexed folder exposes a three-dot menu on hover/focus for favorite,
   Explorer open, reindex, and confirmed index removal; no direct delete button
   is shown on the row.

### User Story 4 - Inspect Original Documents Without Losing App Navigation (Priority: P1)

The user can inspect PDF, HWP, and HWPX in their original page layout, search
inside the rendered pages with yellow highlights, and scroll the preview without
moving the application header or footer.

**Acceptance Scenarios**:

1. PDF, HWP, and HWPX always expose an original-layout tab backed by local bytes.
2. Main-search and in-preview find terms are highlighted in both text and
   original layout views.
3. The text view preserves table spans and natural column alignment.
4. The header/footer remain fixed; left, center, and right content areas own
   their vertical scrolling.

### User Story 5 - Review Detailed Local Statistics (Priority: P2)

The user opens Statistics to understand the local library and search habits at
a glance without sending telemetry outside the PC.

**Acceptance Scenarios**:

1. Summary cards show total files, indexed files, and aggregate size.
2. File types use a labeled donut, while year and folder distributions use
   proportional bars and remain keyboard-filterable.
3. Recent modifications and largest files show ranked, truncated one-line rows.
4. Search history shows totals plus switchable frequent and recent lists with
   frequency bars and relative time.
5. The dialog header/tabs/footer stay visible while its body scrolls.

## Requirements

- **FR-001**: Classify candidates before parsing and persist unsupported/image
  candidates as metadata-only without failure.
- **FR-002**: Bound ordinary parser concurrency to the parser host capacity of 3.
- **FR-003**: Preserve pause, resume, cancellation, attempt ownership, local-only
  OCR, and cloud-placeholder safety.
- **FR-004**: Add `errorCount` to index status; active events omit accumulated
  error detail and terminal events include at most 100 details.
- **FR-005**: Render indexing progress in the application footer with accessible
  phase, counters, filename, progress value, percentage, pause/resume, and cancel.
- **FR-006**: Show a terminal report with exact counts and expandable details.
- **FR-007**: Redesign the sidebar into collapsible indexed-folder, smart-folder,
  recent-search, and bookmark sections without copying frozen source code.
- **FR-008**: Replace direct folder deletion with a hover/focus three-dot action
  menu and a confirmation explaining that source files are never deleted.
- **FR-009**: Render HWP/HWPX locally with bundled WASM and PDF locally with the
  existing renderer; do not upload document bytes.
- **FR-010**: Lock the application shell to the viewport and provide independent
  overflow containers for the sidebar, search results, and preview.
- **FR-011**: Present detailed document and search statistics from the existing
  local database only, with accessible table equivalents and interactive filters.

## Success Criteria

- A 5,000-entry metadata-heavy fixture completes without 5,000 parser requests.
- Active status payload size does not grow with the number of prior failures.
- Parser-host in-flight requests never exceed three.
- Automated Rust, React, and packaged smoke tests cover classification, progress,
  terminal reporting, and sidebar interactions.
