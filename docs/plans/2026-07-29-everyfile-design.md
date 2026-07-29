# EveryFile Product and Technical Design

- Date: 2026-07-29
- Status: Approved
- Product: EveryFile
- Developer: Lebi_Cybereun
- Email: cybereunny@gmail.com
- Copyright: © 2026 Lebi_Cybereun
- Target: Windows desktop

## 1. Purpose

EveryFile is a clean-room functional successor to the comparison application stored at
`L:\codex-L\Everyfile-copy`. The comparison application remains frozen and its source
code and Git history are not copied into this project.

The new application will provide local folder indexing, fast filename and document-body
search, rich previews, document organization, smart semantic search, OCR, document
analysis, and optional AI assistance. It will first reach feature parity with the
comparison application and then add improved workflows, safety, and transparency.

The existing EveryFile icon may be reused. All other application code, layout,
architecture, and implementation will be newly created.

## 2. Product Principles

- Windows-first desktop application distributed as a clean graphical EXE.
- No console, PowerShell, or helper terminal windows during normal use.
- The installer, executable, window, taskbar, and tray use the same application icon.
- Core indexing and search work without an internet connection.
- Only folders explicitly selected by the user are indexed.
- Parsing, OCR, embeddings, search history, and statistics are local by default.
- Documents are sent externally only when the user selects Gemini or OpenAI and
  explicitly invokes an AI feature.
- Source documents are treated as read-only except for an explicit, confirmed
  duplicate-cleanup action that moves selected files to the Windows Recycle Bin.
- Korean is the default interface language; English can be selected.

## 3. Delivery Phases

### Phase 1: Core Search

- Folder registration and indexing
- Filename and document-body search
- Detailed filters and search operators
- Information-dense result list
- Right-side document preview
- Bookmarks, tags, and saved searches
- Windows installer and portable build

### Phase 2: Smart Organization

- Local multilingual semantic search
- Similar-document search
- Automatic document-version grouping
- Statistics and search history
- Smart folders
- Real-time file change tracking

### Phase 3: Document Intelligence

- Local OCR and optional math OCR
- AI summaries and grounded document questions
- Ollama, Gemini, and OpenAI providers
- Document comparison
- Exact and similar duplicate analysis
- Full export workflows

### Phase 4: Parity and Upgrades

- Compare every feature against the frozen reference application
- Close functional gaps
- Improve performance, diagnostics, privacy controls, and accessibility
- Add uniquely designed EveryFile capabilities

Each phase produces a testable Windows installer. The integrated public milestone is
`v1.0.0`.

## 4. Technology Architecture

### Application shell and UI

- Tauri 2
- React and TypeScript
- Rust command layer with narrowly scoped Tauri permissions

### Core services

- Rust file discovery, file watching, job scheduling, and lifecycle management
- Local encrypted metadata and full-text search database
- Local embedding index for smart and similar-document search
- Background queues for parsing, OCR, and embeddings

### Parsing

`cybereun/kordoc--` is the primary structured-document parsing engine and is packaged
as an internal, non-console sidecar where required.

Kordoc handles:

- HWP 3.x and HWP 5.x
- HWPX
- HWPML
- PDF
- XLS and XLSX
- DOCX

Additional local adapters handle:

- DOC
- PPT and PPTX
- ODT, ODS, and ODP
- RTF
- EPUB
- TXT, Markdown, CSV, JSON, XML, HTML, logs, and source code
- Image metadata and OCR routing

Unsupported formats remain searchable by filename, path, extension, size, and dates.
Format detection uses both extension and file signatures.

### AI providers

The user selects one active provider; no provider receives automatic priority.

- Ollama, with `gemma4:e2b` as a recommended local model
- Gemini API
- OpenAI API

All providers implement a common internal interface so models and APIs can be updated
without changing search or preview features.

## 5. Main Window

The main window has three resizable regions:

1. Left navigation sidebar
2. Central search and results area
3. Right document preview

### Header

- EveryFile home button
- Statistics and search-history button
- Add-folder button
- Settings button
- Lower-priority actions collapse into an overflow menu at narrow widths

### Left sidebar

- Registered folders and indexing states
- Smart folders
- Recent searches
- Bookmarks
- Tags
- Per-folder document counts and errors
- Collapsible and resizable

### Central modes

- Search
- Smart
- AI Question

AI Question is hidden when AI features are disabled.

### Home state

When no folder is registered, the screen shows a simple folder-selection action. Once
folders exist, the home screen shows the primary search box, recent searches,
bookmarks, and recently modified documents. Detailed analytics remain in the
statistics screen to keep the home screen focused.

## 6. Visual Design

- Warm ivory background
- Espresso-brown text and structural lines
- Terracotta `#B95336` for selections, primary actions, and progress
- Light apricot for search-term highlights
- Restrained corner rounding and decoration
- Information density and legibility take priority
- No green primary accent

## 7. Detailed Search Experience

### Search input

- Large, central input
- Immediate search while typing
- Request cancellation prevents stale results from replacing newer results
- Autocomplete from local history and vocabulary
- Private-search control

### Search targets

- Keyword and body search
- Filename search

### Match controls

- All terms
- Any term
- Exact phrase
- Excluded terms
- Near-term search

### Sorting

- Relevance
- Confidence
- Newest
- Oldest
- Name
- Size

### Filters

- Multi-select extensions populated from indexed data
- Today, 7 days, 30 days, 90 days, 6 months, 1 year
- Custom day count
- Custom start and end dates
- All folders, a registered folder, or a specific subfolder
- Include or exclude filename matches
- Search within current results
- Save current conditions as a preset or smart folder

Frequently used filters remain visible as chips. Advanced filters expand when needed.
At narrow widths, low-priority filters move into an overflow panel.

### Query syntax

- `"exact phrase"`
- `-excluded`
- `ext:hwp,pdf`
- `path:Documents`
- `after:2026-01-01`
- `before:2026-12-31`
- `term1 ~10 term2`

Typed operators and visual filter chips stay synchronized.

### Results

- Information-dense list by default
- Filename, path, modified date, extension, and size
- Highlighted matching excerpts
- Filename matches and body matches are visibly separated
- Multiple excerpts are grouped under each file
- List view and file-grouped view
- Result count and query latency
- Copy, CSV, XLSX, and ZIP export
- Batch bookmark, tag, and AI actions

## 8. Indexing Pipeline

1. Discover files in user-selected folders.
2. Make filename and metadata search available first.
3. Detect the true file format.
4. Parse document structure in a background worker.
5. Evaluate extracted-text quality.
6. Route empty or low-quality scan content to OCR when enabled.
7. Normalize headings, paragraphs, lists, tables, images, links, and metadata.
8. Update the full-text index.
9. Create local embeddings when the system is idle.
10. Resume unfinished work after application restart.

### Change tracking

- Real-time creation, modification, movement, and deletion detection
- Only changed files are reprocessed
- Periodic reconciliation recovers missed events
- Existing index entries remain available during temporary network-drive outages
- Cloud-only placeholders are not forcibly downloaded
- Cloud-only files remain searchable by metadata

### Reliability

- One malformed file cannot stop a folder job
- Per-file size, time, and memory limits
- Configurable maximum file size
- Pause, resume, cancel, retry, validate, and rebuild controls
- Structured failure categories and diagnostics
- Original files are never modified by indexing

## 9. OCR

- Search settings contain the OCR enable switch.
- OCR runs automatically only for images and PDFs with missing or low-quality text.
- JPG, PNG, WebP, BMP, and TIFF are supported.
- PaddleOCR performs standard OCR locally.
- Page-level progress and failure reasons are visible.
- Math OCR is a separate optional feature.
- Math OCR clearly warns about model size, CPU usage, and processing time.
- Disabling OCR affects image-contained text only; filename and path search continue.

## 10. Smart Search and Versioning

### Semantic search

- Built-in local multilingual embedding model
- Korean and English semantic search without Ollama or an external API
- Embeddings are generated and stored locally
- Smart indexing can pause or reduce intensity on lower-powered computers
- Embedding models can be upgraded by rebuilding only the vector layer

### Version grouping

- Detect suffixes such as `final`, `final-final`, `modified`, and `v2`
- Use folder, extension, size, modified time, and body similarity
- Prefer the newest likely version as the representative
- Allow manual split, merge, and representative selection
- Never merge, rename, move, or delete the source files
- Setting can disable grouping entirely

## 11. Document Preview

### Preview layout

- Opens in the resizable right panel after a single result click
- Can be collapsed or expanded to a full-screen viewer
- Provides Document Text and Original Layout tabs

### Document Text

- Readable rendering of headings, paragraphs, lists, tables, and images
- Search-term and current-match highlighting
- In-document find with next and previous navigation
- Outline navigation when structure is available
- Footnotes, hyperlinks, and page references
- Law-reference links that open only when the user requests them
- Copy text and save Markdown

### Original Layout

- Page rendering for PDF
- Kordoc structure and local rendering for HWP and HWPX
- Format-specific local previews for Office documents
- Page navigation, zoom, fit-width, and full screen
- Clear fidelity notice and text-view fallback when exact rendering is unavailable
- Open in the system's default application
- No document upload for preview generation

### Toolbar

- Open file
- Find in document
- AI summary
- Ask about this file
- Bookmark
- More:
  - Open file location
  - Copy text
  - Save Markdown
  - Copy path
  - Add tag
  - Find similar documents
  - Compare with another document

AI actions are hidden when AI is disabled.

## 12. Document Management

- Bookmarks with notes
- User tags and colors
- Saved searches as smart folders
- Version groups
- Side-by-side document comparison
- Paragraph and table-cell change highlighting
- Exportable comparison report
- Local deadline and expiration detection
- Exact duplicates by SHA-256
- Similar duplicates by local embeddings and document structure
- Duplicate analysis runs only when requested from the Tools menu
- Cleanup requires explicit selection and confirmation
- Cleanup moves files to the Windows Recycle Bin, never permanent deletion

## 13. Statistics and Search History

### Document statistics

- Total files, indexed files, and total size
- Distribution by format, year, and folder
- Recently modified and largest documents
- Parsing, OCR, and queue states
- Duplicate estimates and potentially recoverable storage
- Clicking a chart element runs the corresponding search

### Search history

- Total searches and unique terms
- Frequent and recent searches
- Search mode and period statistics
- Zero-result searches
- Average query latency
- Re-run a search by selecting it
- Delete one item or all items

History is enabled by default with a 90-day default retention period. Available
retention choices are 30 days, 90 days, 1 year, and unlimited. Private Search records
no queries, opened results, or AI questions.

All statistics and history remain in the encrypted local database.

## 14. AI Experience

AI is disabled by default. Enabling it reveals provider settings and AI actions.

### Settings

The primary view shows only:

- Provider
- API key where applicable
- Model
- Connection test

An Advanced section contains:

- Base URL
- Temperature
- Maximum tokens
- Timeout
- Other provider-specific settings

API keys are protected in Windows Credential Manager. Installed Ollama models can be
discovered automatically, and custom model names remain supported.

### Question scopes

- Current file
- Selected results
- Selected folder
- Entire index

Answers cite local source documents and locations. Selecting a citation navigates to
the relevant preview text. The application reports insufficient evidence instead of
presenting unsupported claims as document facts.

### External transfer

Before a Gemini or OpenAI request, the UI identifies:

- Provider
- Number of source documents
- Approximate excerpt size
- Possibility of sensitive content
- Cancel action

Only necessary excerpts are transmitted. Full index data, search history, and local
file paths are not transmitted automatically. Paths are replaced with internal
document identifiers.

## 15. Settings

### General

- Language
- Theme
- Default search mode
- Maximum result count and batch size
- Result density and layout
- UI scale
- Single-click or double-click file opening
- Exact or relative date display

### Search

- Included and excluded folders
- Include subfolders
- Maximum file size
- OCR and math OCR
- Version grouping
- Smart-search model and status
- Validate or rebuild indexes

### AI

- Enable AI
- Provider
- Credentials, model, and connection test
- Collapsible advanced controls
- Local or external processing status

### System

- Start with Windows
- Minimize to tray
- Start hidden
- Indexing intensity
- Reconciliation interval
- Data location
- Cloud and network file policy

The close button exits the application by default. Tray behavior is opt-in.

### Diagnostics

- Parsing and indexing error list
- Open local log folder
- Log retention
- Save a privacy-scrubbed diagnostic report
- Reset application data without touching source documents

## 16. Security and Privacy

- Index keys are protected using the current Windows user account.
- API credentials are stored separately in Windows Credential Manager.
- Application UI does not receive unrestricted filesystem capabilities.
- No analytics telemetry or automatic crash reporting.
- Local diagnostic logs expire after seven days by default.
- Update checks can be disabled and never include document information.
- Core search works offline.
- Resetting application data never deletes source documents.
- Network behavior is tested to verify that documents stay local unless an external
  AI provider is explicitly invoked.

## 17. Keyboard Shortcuts

- `/`: focus search
- `Ctrl+K`: command palette
- `Ctrl+B`: toggle sidebar
- Arrow keys: move result selection
- `Enter`: open selected file
- `Ctrl+F`: find in preview
- `Ctrl+Shift+C`: copy file path
- `Esc`: clear the current selection or search state

## 18. Error Handling

- Parsing errors are classified as encrypted, damaged, unsupported, timed out,
  permission denied, or other actionable categories.
- A failed file can be retried, opened in Explorer, or excluded.
- Interrupted jobs resume after restart.
- A damaged index can be rebuilt without changing source files.
- Provider-specific AI errors include connection and corrective guidance.
- Normal search remains available during AI or network failures.
- Streaming AI requests support cancellation and retry.
- Deleted or moved source files are reconciled safely.

## 19. Testing and Performance

Test coverage includes:

- Keyword, filename, operator, and filter correctness
- Korean, English, numeric, special-character, and long-path behavior
- Supported parsing formats
- Corrupted, encrypted, oversized, and hostile files
- OCR and math OCR
- Real-time change detection
- Semantic search, version grouping, duplicates, and comparison
- All AI providers, cancellation, errors, and citations
- Restart and recovery
- UI responsiveness during indexing
- Network privacy behavior
- Installation and uninstallation safety

Tests use public or synthetic fixtures, not personal documents.

Performance goals:

- Filename search feels immediate on ordinary Windows computers.
- Body search does not interrupt typing.
- Search has priority over indexing and OCR.
- Background work reduces intensity when the application is not active.
- CPU and memory limits are user-adjustable.
- A dedicated load suite covers at least 100,000 files.

## 20. Windows Distribution

- Primary installer: `EveryFile-Setup-v1.0.0.exe`
- Additional portable build
- No console or helper terminal window during normal operation
- Consistent icon across installer, executable, window, taskbar, and tray
- Uninstaller asks whether encrypted application data should be retained
- Optional GitHub Releases update checks
- Signed update packages
- Windows x64 first; ARM64 can be added later

An unsigned development build may trigger Windows SmartScreen. A trusted code-signing
certificate is recommended before public distribution.

## 21. Repository and Licensing

- Local repository: `L:\codex-L\EveryFile`
- Intended GitHub repository: `EveryFile`
- Application copyright: `© 2026 Lebi_Cybereun`
- Developer: `Lebi_Cybereun`
- Email: `cybereunny@gmail.com`

Kordoc is MIT-licensed. Its original copyright and license text, along with notices for
all other incorporated open-source components, must remain in
`THIRD_PARTY_NOTICES`. Those notices are not replaced with the EveryFile application
copyright.

The frozen reference application's source, commits, and license text are not imported.

## 22. Acceptance Criteria

The design is successfully delivered when:

- All approved phases are implemented and tested.
- Feature parity has been checked against the frozen reference application.
- Core search and local smart search work without internet access.
- External AI transfer occurs only after the user enables AI, selects an external
  provider, and invokes an AI action.
- Search data, statistics, and history remain locally encrypted.
- The installer and portable build launch without visible terminals.
- The correct icon appears in all Windows surfaces.
- Third-party license notices are complete.
- A fresh Windows installation can install, index, search, preview, update, and
  uninstall the application without modifying source documents.
