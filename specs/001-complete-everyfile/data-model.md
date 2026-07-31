# Data Model: Complete EveryFile

## WorkspacePreference

- `left_panel_open: bool`
- `right_panel_open: bool`
- `left_panel_width: integer` (validated UI range)
- `right_panel_width: integer` (validated UI range)
- Stored locally; invalid/out-of-range values reset independently.

## OcrSettings

- `enabled: bool` (default false)
- `languages: list` (default Korean and English)
- `math_enabled: bool` (default false)
- `max_pages`, `max_pixels`, `timeout_seconds`
- `model_manifest_version`, readiness and last verification result

Transitions: disabled → enabled/model check → ready or actionable error. Math
enablement requires a separate warning confirmation.

## OcrAttempt

- document and parse-attempt ownership identifiers
- eligibility reason and page range
- state: queued/running/completed/skipped/failed/cancelled
- model/version, recognized text, warnings, duration
- failure code contains no source text or secret

## AiSettings

- `enabled: bool` (default false)
- selected provider: Ollama, Gemini, or OpenAI
- provider endpoint, model, temperature, maximum tokens
- secret reference (protected outside the database), never raw secret
- remote disclosure consent version and timestamp per remote provider

## AiRequest

- request id, selected document id, operation: summary or question
- provider snapshot and model
- bounded local source chunk identifiers
- state: pending/streaming/completed/failed/cancelled
- output and numbered citations; not added to search history

## Existing Entities

RegisteredFolder, Document, IndexJob, SearchRequest, SearchHistory, Bookmark,
Tag, and ReleaseArtifact remain as already implemented. Migrations only add OCR,
AI, and workspace fields; existing encrypted data remains compatible.
