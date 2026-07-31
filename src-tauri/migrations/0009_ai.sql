CREATE TABLE IF NOT EXISTS ai_secrets (
  provider TEXT PRIMARY KEY CHECK (provider IN ('gemini', 'openai')),
  secret TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
