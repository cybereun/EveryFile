CREATE TABLE IF NOT EXISTS ocr_attempts (
  document_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
  attempt_token TEXT,
  state TEXT NOT NULL DEFAULT 'pending'
    CHECK (state IN ('pending', 'running', 'completed', 'skipped', 'failed', 'cancelled')),
  engine TEXT NOT NULL DEFAULT 'paddleocr',
  model_kind TEXT NOT NULL DEFAULT 'text'
    CHECK (model_kind IN ('text', 'math')),
  source_hash TEXT,
  error_code TEXT,
  started_at TEXT,
  finished_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS ocr_attempt_token_unique
  ON ocr_attempts(attempt_token)
  WHERE attempt_token IS NOT NULL;

CREATE TABLE IF NOT EXISTS ocr_model_state (
  model_kind TEXT PRIMARY KEY CHECK (model_kind IN ('text', 'math')),
  model_name TEXT NOT NULL,
  manifest_sha256 TEXT,
  state TEXT NOT NULL DEFAULT 'missing'
    CHECK (state IN ('missing', 'ready', 'invalid', 'error')),
  error_code TEXT,
  last_verified_at TEXT
);

UPDATE ocr_attempts
SET state = 'cancelled',
    error_code = 'OCR_INTERRUPTED',
    finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    attempt_token = NULL
WHERE state = 'running' OR attempt_token IS NOT NULL;
