# Feature Specification: Complete EveryFile

**Feature Branch**: `codex/phase-1-core-search`

**Created**: 2026-07-31

**Status**: Approved

**Input**: Complete the clean-room EveryFile Windows application from the
original requirements, fix production folder registration, add independently
collapsible left and right panels, local OCR, opt-in selectable AI, full
document workflows, and a verified installer and portable release.

## User Scenarios & Testing

### User Story 1 - Register, Index, and Find a Folder (Priority: P1)

A user selects one or more folders, sees them appear immediately, follows
indexing progress, and searches filenames and document contents.

**Why this priority**: Without this journey the application cannot perform its
primary purpose.

**Independent Test**: In a newly installed application, select a fixture folder,
wait for completion, search a phrase found only inside a document, and open the
matching result.

**Acceptance Scenarios**:

1. **Given** no registered folders, **When** the user selects a folder, **Then**
   it appears in the left panel and indexing begins without restarting.
2. **Given** an indexed folder, **When** the user searches by filename or
   content, **Then** matching files appear with relevant snippets and filters.
3. **Given** folder selection or indexing fails, **When** the failure occurs,
   **Then** a useful error is displayed and the user can retry safely.

---

### User Story 2 - Control the Workspace and Inspect Documents (Priority: P1)

A user independently opens or closes the left folder panel and right preview
panel, resizes them, and inspects document text or original layout.

**Why this priority**: The workspace must adapt to searching, reading, and small
screens without hiding essential content.

**Independent Test**: Toggle both panels independently, restart the application,
verify their state and width, select a result, search inside it, and use every
non-AI toolbar action.

**Acceptance Scenarios**:

1. **Given** both panels are open, **When** either toggle is used, **Then** only
   that panel closes and the center workspace expands.
2. **Given** saved panel state, **When** the app restarts, **Then** visibility
   and valid widths are restored.
3. **Given** a selected result, **When** the user switches preview modes,
   **Then** extracted text and supported original layout are available.
4. **Given** a preview, **When** the user uses the toolbar, **Then** open,
   in-document find, copy, Markdown save, bookmark, path copy, and tagging work.

---

### User Story 3 - Search Scans with Local OCR (Priority: P2)

A user enables OCR and finds text inside scanned PDFs and supported image files
without sending those files outside the PC.

**Why this priority**: Image-only documents are otherwise invisible to content
search.

**Independent Test**: Index a folder containing a text PDF, scanned PDF, and
JPG. Verify the text PDF skips OCR, scans are searchable only when OCR is
enabled, and a network sentinel receives no connection.

**Acceptance Scenarios**:

1. **Given** OCR enabled, **When** an image-only supported document is indexed,
   **Then** recognized text becomes searchable locally.
2. **Given** a PDF with usable embedded text, **When** it is indexed, **Then**
   embedded text is used without OCR.
3. **Given** OCR disabled, **When** an image is indexed, **Then** filename and
   path remain searchable but image text does not.
4. **Given** mathematical OCR enabled, **When** the user confirms its resource
   warning, **Then** math-heavy PDFs may use the additional local model.

---

### User Story 4 - Choose and Use AI Explicitly (Priority: P2)

A user enables AI, chooses Ollama, Gemini, or OpenAI, tests the configuration,
then summarizes or asks questions about the selected document.

**Why this priority**: AI is valuable only when provider choice and data
disclosure remain under user control.

**Independent Test**: Verify AI controls are absent while disabled; test each
provider with a mock endpoint; verify local and remote disclosure behavior,
streaming cancellation, and that no provider fallback occurs.

**Acceptance Scenarios**:

1. **Given** AI disabled, **When** settings and previews are opened, **Then** AI
   provider controls and AI document actions are unavailable.
2. **Given** AI enabled, **When** a provider is chosen, **Then** only fields
   relevant to that provider are displayed and validated.
3. **Given** a remote provider, **When** a summary or question is submitted,
   **Then** the app explains that selected document context will leave the PC.
4. **Given** a configured provider, **When** the user requests a summary or
   answer, **Then** the result cites document excerpts and can be cancelled.

---

### User Story 5 - Understand and Manage the Library (Priority: P2)

A user reviews statistics and local search history, exports results, manages
bookmarks and tags, uses private search, and resets application data safely.

**Why this priority**: A durable personal library needs transparent management
and privacy controls.

**Independent Test**: Create history, private searches, bookmarks, and tags;
restart; verify persistence and retention; export; reset; verify source files
are unchanged.

**Acceptance Scenarios**:

1. **Given** indexed documents, **When** statistics open, **Then** totals,
   formats, years, folders, recent files, largest files, and parse status are
   visible and filterable.
2. **Given** normal and private searches, **When** history opens, **Then** only
   normal searches are present and can be deleted.
3. **Given** application data, **When** confirmed reset completes, **Then** only
   EveryFile-owned data is removed and source documents remain unchanged.

---

### User Story 6 - Install or Run Portably (Priority: P1)

A user installs EveryFile or extracts a no-install package and starts it with
the approved identity and no terminal window.

**Why this priority**: A feature is not delivered until the packaged build works
on a normal Windows account.

**Independent Test**: On a clean Windows user profile, exercise the complete
P1 journey in both installer and portable builds.

**Acceptance Scenarios**:

1. **Given** either distribution, **When** EveryFile starts, **Then** the correct
   app, window, taskbar, and installer icon appear without a terminal.
2. **Given** a published release, **When** assets are downloaded, **Then** their
   checksums match and the release points to the tested source revision.

### Edge Cases

- The user cancels folder selection or selects the same folder twice.
- A selected folder is deleted, renamed, disconnected, permission denied, or a
  parent/child of another registered folder.
- Files change or disappear during discovery, parsing, preview, or export.
- A cloud placeholder, symbolic link, junction, or path outside a trusted root
  is encountered.
- Indexing is paused, resumed, cancelled, or interrupted by a crash.
- OCR models are unavailable, documents exceed limits, or recognition fails.
- AI credentials are missing, endpoints are invalid, responses time out, or the
  user changes provider during a request.
- The window is too narrow for both panels or saved dimensions are invalid.
- The local database is locked, corrupted, or opened with the wrong key.

## Requirements

### Functional Requirements

- **FR-001**: The system MUST provide visible folder-add actions in the header
  and empty left panel.
- **FR-002**: Folder selection MUST register, display, activate, and begin
  indexing the chosen folder in the running production application.
- **FR-003**: The app MUST load existing folders and library counts at startup.
- **FR-004**: Users MUST be able to remove folders without deleting source files.
- **FR-005**: Indexing MUST expose progress, current file, errors, pause, resume,
  cancel, restart recovery, and file-change reconciliation.
- **FR-006**: Filename matches MUST become searchable before body parsing ends.
- **FR-007**: Search MUST support keyword/filename modes, all/any/exact
  matching, operators, extensions, dates, folders, filename include/exclude,
  result-within-result, sorting, paging, and saved presets.
- **FR-008**: The left and right panels MUST open, close, and resize
  independently with accessible controls and persisted state.
- **FR-009**: Preview MUST support structured text and original layout where
  available, with bounded reads and cancellation.
- **FR-010**: Preview actions MUST include source open, location open,
  in-document find, text copy, Markdown save, path copy, bookmark, and tags.
- **FR-011**: The system MUST support core office, Korean document, PDF,
  spreadsheet, presentation, plain-text, and common image formats.
- **FR-012**: OCR MUST be independently enabled and MUST run locally for
  image-only PDFs and JPG, PNG, WebP, BMP, and TIFF.
- **FR-013**: PDFs with usable embedded text MUST skip OCR.
- **FR-014**: Mathematical OCR MUST be separately enabled and show a resource
  warning before model use.
- **FR-015**: OCR disabled MUST preserve filename and path search.
- **FR-016**: AI MUST default to disabled and reveal configuration only after
  explicit activation.
- **FR-017**: Users MUST choose Ollama, Gemini, or OpenAI without automatic
  fallback or provider priority.
- **FR-018**: Provider settings MUST support endpoint, API key where applicable,
  model, temperature, maximum tokens, visibility, and connection test.
- **FR-019**: AI MUST provide selected-document summary and question answering
  with cancellable output and supporting excerpts.
- **FR-020**: Remote AI MUST disclose document transmission before first use;
  Ollama MUST support a user-selected local model.
- **FR-021**: Statistics MUST cover totals, size, formats, years, folders,
  recent/largest documents, and processing states.
- **FR-022**: Search history MUST be local, retained for the configured period,
  searchable, individually deletable, and clearable.
- **FR-023**: Private search MUST write no history or query statistics.
- **FR-024**: Bookmarks, notes, tags, settings, panel state, and library state
  MUST persist across restart.
- **FR-025**: The local database MUST be encrypted with a user-bound protected
  key.
- **FR-026**: Core indexing, parsing, OCR, search, preview, and statistics MUST
  create zero outbound network connections.
- **FR-027**: Cloud placeholder files MUST not be downloaded automatically.
- **FR-028**: Reset and uninstall MUST never modify indexed source documents.
- **FR-029**: User-facing failures MUST state what failed and offer a safe next
  action; production handlers MUST NOT silently discard failures.
- **FR-030**: Installer and portable distributions MUST use the approved name,
  publisher, copyright, icon, version, and GUI-only launch behavior.
- **FR-031**: Every published release MUST include SHA-256 checksums and a clear
  code-signing/SmartScreen status.
- **FR-032**: The frozen comparison application MUST remain unchanged and its
  source MUST NOT be copied into this repository.

### Key Entities

- **Registered Folder**: User-selected trusted root, display name, state, and
  document counts.
- **Document**: Trusted source identity, metadata, parse/OCR state, indexed text,
  preview data, bookmark, and tags.
- **Index Job**: Discovery and parsing progress with lifecycle and errors.
- **Search Request/History**: Query, mode, filters, ordering, timing, privacy
  flag, and retention state.
- **OCR Configuration**: Enablement, languages, math option, model readiness,
  and resource limits.
- **AI Configuration**: Enablement, selected provider, endpoint, protected
  credential, model, sampling, and output limits.
- **AI Conversation**: Selected document, question, answer, citations, provider,
  cancellation, and transmission consent.
- **Workspace State**: Left/right panel visibility and valid widths.
- **Release Artifact**: Installer/portable asset, version, source revision, and
  checksum.

## Success Criteria

### Measurable Outcomes

- **SC-001**: A first-time user can register a folder and see indexing begin in
  under 30 seconds without documentation.
- **SC-002**: All primary installer and portable journeys pass on a clean
  Windows profile, including restart persistence.
- **SC-003**: Filename-only results begin appearing within two seconds for a
  10,000-file local fixture on the reference PC.
- **SC-004**: At least 95% of interactive searches in a 100,000-document library
  display an initial result page within one second after indexing.
- **SC-005**: Core operations produce zero connections at the network sentinel.
- **SC-006**: Every supported scan fixture becomes searchable when OCR is
  enabled, while ordinary text PDFs are recorded as OCR-skipped.
- **SC-007**: AI-disabled testing observes zero AI controls and zero provider
  connections; each enabled provider passes contract and consent tests.
- **SC-008**: Closing either side panel never closes the other and increases the
  center workspace; state survives 100 restart cycles without invalid layout.
- **SC-009**: Reset/uninstall verification reports byte-identical source
  fixtures before and after the operation.
- **SC-010**: No release is published with a failing required checklist item,
  missing artifact, mismatched checksum, terminal window, or wrong icon.

## Assumptions

- The target is 64-bit Windows with a supported system web runtime.
- Users explicitly choose which local folders to index and have permission to
  read them.
- AI is optional; all non-AI functionality works fully offline.
- Remote-provider billing, quotas, and account terms remain the user's
  responsibility.
- Code signing is outside the current scope until a certificate is supplied;
  unsigned status is disclosed clearly.
- Existing clean-room backend, search, preview, statistics, security, and
  packaging work will be retained where it satisfies this specification.
