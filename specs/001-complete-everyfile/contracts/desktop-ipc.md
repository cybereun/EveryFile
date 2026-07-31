# Desktop IPC Contract

## Folder and workspace

- `register_folder() -> FolderRecord | null`: native picker; null is cancellation.
- `list_folders() -> FolderRecord[]`
- `start_indexing(folder_id) -> JobId`
- `get_library_summary() -> counts and queue state`
- Frontend production orchestration MUST refresh folders and summary after every
  mutation and surface structured command failures.

## OCR

- `get_ocr_status() -> settings/model readiness`
- `verify_ocr_models() -> readiness`
- OCR is an indexing implementation detail; no arbitrary filesystem path is
  accepted from the webview.
- Sidecar protocol is length-bounded JSON lines: request id, trusted path,
  media/page metadata, options; response id, state, text, page blocks, warnings.

## AI

- `get_ai_settings()`, `save_ai_settings(redacted)`, `set_ai_secret(provider)`
- `test_ai_provider() -> provider/model/latency result`
- `start_document_ai(request) -> request id`
- `cancel_document_ai(request_id)`
- streaming events contain request id, delta, citations, terminal state
- APIs accept document ids, never frontend-supplied filesystem paths or raw
  document bodies.

## Errors

Every command failure includes a stable code, safe message, retryability, and
optional remediation. Secrets, full source text, and unrelated absolute paths
are excluded.
