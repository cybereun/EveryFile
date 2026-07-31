CREATE TABLE IF NOT EXISTS ai_secrets (
  provider TEXT PRIMARY KEY CHECK (provider IN ('gemini', 'openai')),
  secret TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_requests (
  request_id TEXT PRIMARY KEY,
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  provider TEXT NOT NULL CHECK (provider IN ('ollama', 'gemini', 'openai')),
  operation TEXT NOT NULL CHECK (operation IN ('summary', 'question')),
  state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'cancelled')),
  remote_consent INTEGER NOT NULL DEFAULT 0 CHECK (remote_consent IN (0, 1)),
  error_code TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT
);

CREATE INDEX IF NOT EXISTS ai_requests_document_created_idx
  ON ai_requests(document_id, created_at DESC);

UPDATE ai_requests
SET state = 'cancelled',
    error_code = 'AI_INTERRUPTED',
    finished_at = COALESCE(finished_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
WHERE state = 'running';
