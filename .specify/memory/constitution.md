<!--
Sync Impact Report
- Version change: template -> 1.0.0
- Added principles: Local Privacy; Complete User Journeys; Explicit AI Consent;
  Safety and Ownership; Verified Windows Releases
- Added sections: Product Constraints; Development and Release Workflow
- Removed sections: none
- Templates: plan/spec/tasks templates reviewed; feature artifacts carry the
  concrete gates without changing the shared templates
- Deferred items: none
-->
# EveryFile Constitution

## Core Principles

### I. Local Privacy Is the Default
Folder discovery, document parsing, OCR, indexing, search, previews, bookmarks,
tags, statistics, and history MUST execute on the user's PC. These operations
MUST NOT transmit document content, filenames, paths, queries, or derived data
to an external service. Network access MUST be denied by default and covered by
an automated zero-outbound-connection test. This protects private document
libraries without requiring the user to understand implementation details.

### II. Complete User Journeys, Not Disconnected Components
A feature is complete only when its production UI, IPC contract, backend
service, persistence, error state, and packaged-app behavior are connected.
Component tests or backend tests alone MUST NOT qualify a feature for release.
Each primary journey MUST have a packaged-app acceptance test, including folder
registration, indexing, search, preview, restart persistence, and reset.

### III. External AI Requires Explicit, Informed Consent
AI controls MUST remain hidden or disabled until the user enables AI. The user
MUST select Ollama, Gemini, or OpenAI and configure the corresponding model and
credentials. Before any remote call, the UI MUST state what content will leave
the PC. Local Ollama requests remain local; Gemini and OpenAI requests MUST send
only the minimum user-selected document context. Provider changes MUST take
effect explicitly and never silently fall back to another provider.

### IV. Safety, Identity, and User Data Ownership
Every file read, opened, exported, or deleted MUST be validated against the
registered source identity and intended root. Application reset and uninstall
MUST remove only EveryFile-owned data and MUST never alter source documents.
Atomic writes, bounded resource use, cancellation ownership, encrypted local
storage, and Windows DPAPI key protection are non-negotiable. Security fixes
MUST include a regression test for the exact failure mode.

### V. Verified Windows Releases
Every release MUST produce an x64 Windows installer and a no-install portable
ZIP using the approved EveryFile icon and publisher. The application and parser
MUST use the Windows GUI subsystem and show no terminal window. A release MUST
not be described as complete until frontend tests, Rust tests, lint/format
checks, privacy tests, packaged-app journeys, installer/portable launch tests,
and SHA-256 artifact verification pass.

## Product Constraints

- Product name: EveryFile; developer: Lebi_Cybereun; copyright year: 2026.
- Platform: Windows desktop, Tauri 2, React/TypeScript, Rust, encrypted SQLite.
- Visual language: warm ivory surfaces with a distinctive coral accent and the
  approved transparent folder-and-magnifier icon.
- Kordoc is the pinned document parser. PaddleOCR-compatible local OCR handles
  scanned PDFs and JPG, PNG, WebP, BMP, and TIFF images.
- Normal text PDFs MUST use embedded text without OCR. Mathematical OCR is a
  separate opt-in setting with clear model-size, CPU, and duration warnings.
- Left folder navigation and right preview panels MUST each open and close
  independently, preserve their widths, and remain keyboard accessible.
- Cloud placeholder files MUST not be downloaded automatically.
- Core features MUST remain usable with AI disabled and without internet access.

## Development and Release Workflow

1. Maintain a requirements-to-task checklist in the active feature directory.
2. Write a failing regression or acceptance test before fixing a defect.
3. Implement one complete vertical journey at a time and mark its tasks complete
   only after production wiring and user-visible error handling are verified.
4. Run focused tests after each task and the full release gate before tagging.
5. Inspect the actual installer and portable build, not only development mode.
6. Keep `L:\codex-L\Everyfile-copy` frozen and exclude its source from this
   clean-room repository.
7. Publish or replace a GitHub Release only after the user authorizes release
   mutation and the complete gate passes.

## Governance

This constitution supersedes conflicting implementation shortcuts and release
claims. Amendments require a documented reason, an updated Sync Impact Report,
and user approval when product behavior, privacy, or release criteria change.
Semantic versioning applies to this document: MAJOR for incompatible principle
changes, MINOR for new principles or material expansion, and PATCH for
clarifications. Every plan and pull request MUST document constitution
compliance; exceptions require explicit justification and a removal task.

**Version**: 1.0.0 | **Ratified**: 2026-07-31 | **Last Amended**: 2026-07-31
