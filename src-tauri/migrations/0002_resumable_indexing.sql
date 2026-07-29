CREATE TABLE IF NOT EXISTS index_job_files (
  job_id TEXT NOT NULL REFERENCES index_jobs(id) ON DELETE CASCADE,
  canonical_path TEXT NOT NULL,
  relative_path TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  modified_at TEXT NOT NULL,
  metadata_only INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL DEFAULT 'queued',
  PRIMARY KEY (job_id, canonical_path)
);

CREATE TABLE IF NOT EXISTS index_job_errors (
  id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL REFERENCES index_jobs(id) ON DELETE CASCADE,
  file_name TEXT NOT NULL,
  code TEXT NOT NULL,
  message TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS index_job_files_state
  ON index_job_files(job_id, state);
