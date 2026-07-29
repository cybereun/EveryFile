CREATE TABLE IF NOT EXISTS folders (
  id TEXT PRIMARY KEY,
  canonical_path TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY,
  folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
  canonical_path TEXT NOT NULL UNIQUE,
  file_name TEXT NOT NULL,
  extension TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  modified_at TEXT NOT NULL,
  content_hash TEXT,
  parser_kind TEXT,
  parse_state TEXT NOT NULL,
  parse_error_code TEXT,
  indexed_at TEXT
);

CREATE TABLE IF NOT EXISTS document_content (
  document_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
  title TEXT,
  body TEXT NOT NULL,
  markdown TEXT NOT NULL,
  blocks_json TEXT NOT NULL,
  warnings_json TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS document_fts USING fts5(
  document_id UNINDEXED,
  file_name,
  title,
  body,
  tokenize='unicode61'
);

CREATE TABLE IF NOT EXISTS bookmarks (
  document_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
  note TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  color TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS document_tags (
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (document_id, tag_id)
);

CREATE TABLE IF NOT EXISTS search_history (
  id TEXT PRIMARY KEY,
  query TEXT NOT NULL,
  mode TEXT NOT NULL,
  filters_json TEXT NOT NULL,
  result_count INTEGER NOT NULL,
  elapsed_ms INTEGER NOT NULL,
  searched_at TEXT NOT NULL,
  private INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS index_jobs (
  id TEXT PRIMARY KEY,
  folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
  state TEXT NOT NULL,
  completed_files INTEGER NOT NULL DEFAULT 0,
  total_files INTEGER NOT NULL DEFAULT 0,
  last_path TEXT,
  updated_at TEXT NOT NULL
);
