CREATE TABLE IF NOT EXISTS index_job_recovery (
  job_id TEXT PRIMARY KEY REFERENCES index_jobs(id) ON DELETE CASCADE,
  discovery_complete INTEGER NOT NULL DEFAULT 0
);

INSERT OR IGNORE INTO index_job_recovery (job_id, discovery_complete)
SELECT
  id,
  CASE
    WHEN state IN ('parsing', 'completed', 'cancelled', 'failed') OR total_files > 0 THEN 1
    ELSE 0
  END
FROM index_jobs;

CREATE TABLE IF NOT EXISTS reconciliation_seen (
  folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
  canonical_path TEXT NOT NULL,
  PRIMARY KEY (folder_id, canonical_path)
);
