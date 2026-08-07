ALTER TABLE index_jobs
ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual'
CHECK (origin IN ('manual', 'watcher', 'reconciliation'));
