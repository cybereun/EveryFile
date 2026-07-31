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
