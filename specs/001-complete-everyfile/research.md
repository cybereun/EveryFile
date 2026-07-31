# Research: Complete EveryFile

## Production orchestration

**Decision**: Add a production `DesktopApp` container that owns folder/library
state and passes non-optional actions into the presentational shell.

**Rationale**: The current release renders `<App />` with optional props, making
the folder button disabled and leaving persisted folders unloaded. A single
container provides an auditable vertical boundary and user-visible errors.

**Alternatives considered**: Let every child fetch independently (duplicated and
racy); move all state into the Rust window (harder frontend testing).

## Panel behavior

**Decision**: Persist independent `left-open`, `right-open`, and width values;
add explicit header controls plus keyboard shortcuts. On narrow windows,
visibility may be temporarily constrained without overwriting user preference.

**Rationale**: This satisfies independent control while preserving responsive
layout and restart behavior.

## OCR runtime

**Decision**: Use a pinned local PaddleOCR CPU sidecar with bundled Korean/
English PP-OCR models. Keep math recognition a separate optional local model
pack and execution path. The app hashes model files before use.

**Rationale**: Official PaddleOCR supports Windows and CPU local inference.
Process isolation permits cancellation and hard memory/time limits. Bundling
models avoids sending documents or fetching a model during indexing.

**Alternatives considered**: Hosted OCR (violates privacy); OCR in the webview
(poor background lifecycle); unpinned first-use downloads (non-reproducible).

## OCR eligibility

**Decision**: Use embedded PDF text when it exceeds quality thresholds. Render
and OCR only image-only/low-text pages. Common image files enter OCR only when
enabled; filename/path metadata always remains indexed.

**Rationale**: Avoids unnecessary CPU work and duplicate/poorer text.

## AI boundary

**Decision**: Implement explicit adapters for Ollama chat, Gemini streaming
generation, and OpenAI streaming Responses. Store secrets protected for the
Windows user, never in the database or logs. Permit only the selected endpoint.

**Rationale**: Provider-specific parsing and disclosure are safer than silent
compatibility fallbacks. All three support streamed/cancellable generation.

**Alternatives considered**: Browser-side calls (exposes keys); automatic
fallback (violates user choice); a generic OpenAI-compatible adapter only
(does not meet Gemini/Ollama-native requirements).

## Document QA retrieval

**Decision**: Chunk the already indexed selected document locally, score chunks
against the question, and send only the smallest relevant set plus numbered
source excerpts. Summaries use bounded hierarchical chunks.

**Rationale**: Controls tokens and remote disclosure without requiring a cloud
embedding service.

## Release testing

**Decision**: Add packaged tests that invoke the real folder picker command
through an E2E-only fixture registration boundary, then exercise indexing,
search, preview, persistence, private history, OCR, and mocked AI.

**Rationale**: This directly covers the integration gap that escaped the first
release while keeping arbitrary test paths out of production builds.
