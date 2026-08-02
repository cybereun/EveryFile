use std::sync::Arc;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::infrastructure::database::Database;

const MAX_HISTORY_PAGE_SIZE: u32 = 500;

// `modified_at` is stored as text for backwards compatibility. Older rows may
// contain an ISO date, while the indexer writes SystemTime as nanoseconds
// since the Unix epoch. Normalize both representations before grouping so the
// yearly chart remains populated for existing libraries as well as new ones.
const BY_YEAR_SQL: &str = r#"
WITH normalized AS (
  SELECT CASE
    WHEN modified_at GLOB '[0-9][0-9][0-9][0-9]-*'
      THEN substr(modified_at, 1, 4)
    WHEN modified_at NOT GLOB '*[^0-9]*'
         AND length(modified_at) >= 18
      THEN strftime('%Y', datetime(CAST(modified_at AS INTEGER) / 1000000000, 'unixepoch'))
    WHEN modified_at NOT GLOB '*[^0-9]*'
         AND length(modified_at) >= 15
      THEN strftime('%Y', datetime(CAST(modified_at AS INTEGER) / 1000000, 'unixepoch'))
    WHEN modified_at NOT GLOB '*[^0-9]*'
         AND length(modified_at) >= 12
      THEN strftime('%Y', datetime(CAST(modified_at AS INTEGER) / 1000, 'unixepoch'))
    WHEN modified_at NOT GLOB '*[^0-9]*'
         AND trim(modified_at) != ''
      THEN strftime('%Y', datetime(CAST(modified_at AS INTEGER), 'unixepoch'))
  END AS year
  FROM documents
)
SELECT year, COUNT(*)
FROM normalized
WHERE year IS NOT NULL
GROUP BY year
ORDER BY year DESC
"#;

#[derive(Clone)]
pub struct StatisticsRepository {
    database: Arc<Database>,
}

impl StatisticsRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn get_statistics(&self) -> Result<DocumentStatistics, StatisticsError> {
        let connection = self.database.connection();
        let (total_documents, indexed_documents, total_bytes) = connection.query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN indexed_at IS NOT NULL THEN 1 ELSE 0 END),
                    COALESCE(SUM(MAX(size_bytes, 0)), 0)
             FROM documents",
            [],
            |row| {
                Ok((
                    non_negative(row.get::<_, i64>(0)?),
                    non_negative(row.get::<_, i64>(1)?),
                    non_negative(row.get::<_, i64>(2)?),
                ))
            },
        )?;

        let by_extension = count_buckets(
            &connection,
            "SELECT CASE WHEN trim(extension) = '' THEN '(none)' ELSE lower(extension) END,
                    COUNT(*)
             FROM documents
             GROUP BY CASE WHEN trim(extension) = '' THEN '(none)' ELSE lower(extension) END
             ORDER BY COUNT(*) DESC, 1",
        )?;
        let by_folder = folder_buckets(
            &connection,
            "SELECT folders.id, folders.display_name, COUNT(documents.id)
             FROM folders
             LEFT JOIN documents ON documents.folder_id = folders.id
             GROUP BY folders.id, folders.display_name
             HAVING COUNT(documents.id) > 0
             ORDER BY COUNT(documents.id) DESC, folders.display_name",
        )?;
        let by_year = count_buckets(&connection, BY_YEAR_SQL)?;
        let parse_states = count_buckets(
            &connection,
            "SELECT parse_state, COUNT(*)
             FROM documents
             GROUP BY parse_state
             ORDER BY COUNT(*) DESC, parse_state",
        )?;
        let recently_modified = document_summaries(
            &connection,
            "SELECT d.id, d.file_name, d.canonical_path, d.extension, d.size_bytes,
                    d.modified_at, f.display_name, d.parse_state
             FROM documents d
             JOIN folders f ON f.id = d.folder_id
             ORDER BY d.modified_at DESC, d.id
             LIMIT 10",
        )?;
        let largest_documents = document_summaries(
            &connection,
            "SELECT d.id, d.file_name, d.canonical_path, d.extension, d.size_bytes,
                    d.modified_at, f.display_name, d.parse_state
             FROM documents d
             JOIN folders f ON f.id = d.folder_id
             ORDER BY d.size_bytes DESC, d.id
             LIMIT 10",
        )?;
        let (total_searches, unique_search_terms) = connection.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT lower(trim(query)))
             FROM search_history
             WHERE private = 0",
            [],
            |row| {
                Ok((
                    non_negative(row.get::<_, i64>(0)?),
                    non_negative(row.get::<_, i64>(1)?),
                ))
            },
        )?;
        let frequent_searches = search_term_counts(&connection)?;
        let recent_searches = history_rows(&connection, 10, 0)?;

        Ok(DocumentStatistics {
            total_documents,
            indexed_documents,
            total_bytes,
            by_extension,
            by_folder,
            by_year,
            recently_modified,
            largest_documents,
            parse_states,
            total_searches,
            unique_search_terms,
            frequent_searches,
            recent_searches,
        })
    }

    pub fn list_search_history(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<SearchHistoryRecord>, StatisticsError> {
        let connection = self.database.connection();
        history_rows(&connection, limit.min(MAX_HISTORY_PAGE_SIZE), offset)
    }

    pub fn delete_search_history(&self, id: &str) -> Result<bool, StatisticsError> {
        if id.trim().is_empty() {
            return Err(StatisticsError::InvalidInput(
                "history id cannot be empty".into(),
            ));
        }
        Ok(self.database.connection().execute(
            "DELETE FROM search_history WHERE id = ?1 AND private = 0",
            [id],
        )? == 1)
    }

    pub fn clear_search_history(&self) -> Result<u64, StatisticsError> {
        let deleted = self
            .database
            .connection()
            .execute("DELETE FROM search_history", [])?;
        Ok(u64::try_from(deleted).unwrap_or(u64::MAX))
    }

    pub fn run_history_retention(&self, retention_days: u32) -> Result<u64, StatisticsError> {
        validate_retention(retention_days)?;
        if retention_days == 0 {
            return Ok(0);
        }
        let modifier = format!("-{retention_days} days");
        let deleted = self.database.connection().execute(
            "DELETE FROM search_history
             WHERE datetime(searched_at) < datetime('now', ?1)",
            [modifier],
        )?;
        Ok(u64::try_from(deleted).unwrap_or(u64::MAX))
    }

    pub fn run_due_history_retention(&self, retention_days: u32) -> Result<bool, StatisticsError> {
        validate_retention(retention_days)?;
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        let already_ran_today = transaction
            .query_row(
                "SELECT value = strftime('%Y-%m-%d', 'now')
                 FROM maintenance_state
                 WHERE key = 'history_retention_last_run'",
                [],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(false);
        if already_ran_today {
            transaction.commit()?;
            return Ok(false);
        }
        if retention_days != 0 {
            let modifier = format!("-{retention_days} days");
            transaction.execute(
                "DELETE FROM search_history
                 WHERE datetime(searched_at) < datetime('now', ?1)",
                [modifier],
            )?;
        }
        transaction.execute(
            "INSERT INTO maintenance_state (key, value)
             VALUES ('history_retention_last_run', strftime('%Y-%m-%d', 'now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn list_parse_errors(&self) -> Result<Vec<ParseErrorRecord>, StatisticsError> {
        let connection = self.database.connection();
        let mut statement = connection.prepare(
            "SELECT d.id, d.file_name, d.canonical_path, d.parse_error_code,
                    d.modified_at, f.display_name
             FROM documents d
             JOIN folders f ON f.id = d.folder_id
             WHERE d.parse_state = 'failed'
             ORDER BY d.modified_at DESC, d.id",
        )?;
        let records = statement
            .query_map([], |row| {
                Ok(ParseErrorRecord {
                    document_id: row.get(0)?,
                    file_name: row.get(1)?,
                    path: row.get(2)?,
                    error_code: row
                        .get::<_, Option<String>>(3)?
                        .unwrap_or_else(|| "UNKNOWN".into()),
                    modified_at: row.get(4)?,
                    folder_name: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub fn retry_parse(&self, document_id: &str) -> Result<bool, StatisticsError> {
        if document_id.trim().is_empty() {
            return Err(StatisticsError::InvalidInput(
                "document id cannot be empty".into(),
            ));
        }
        let changed = self.database.connection().execute(
            "UPDATE documents
             SET parse_state = 'pending',
                 parse_error_code = NULL,
                 parse_attempt_token = NULL
             WHERE id = ?1
               AND parse_state = 'failed'
               AND parse_attempt_token IS NULL",
            [document_id],
        )?;
        Ok(changed == 1)
    }

    pub fn failed_document_folder(
        &self,
        document_id: &str,
    ) -> Result<Option<String>, StatisticsError> {
        Ok(self
            .database
            .connection()
            .query_row(
                "SELECT folder_id FROM documents
                 WHERE id = ?1 AND parse_state = 'failed'",
                [document_id],
                |row| row.get(0),
            )
            .optional()?)
    }
}

fn validate_retention(retention_days: u32) -> Result<(), StatisticsError> {
    if matches!(retention_days, 0 | 30 | 90 | 365) {
        Ok(())
    } else {
        Err(StatisticsError::InvalidInput(
            "retention must be 30, 90, 365, or 0 for unlimited".into(),
        ))
    }
}

fn count_buckets(
    connection: &rusqlite::Connection,
    sql: &str,
) -> Result<Vec<StatisticsBucket>, StatisticsError> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], |row| {
        Ok(StatisticsBucket {
            label: row.get(0)?,
            count: non_negative(row.get::<_, i64>(1)?),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn folder_buckets(
    connection: &rusqlite::Connection,
    sql: &str,
) -> Result<Vec<FolderStatisticsBucket>, StatisticsError> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], |row| {
        Ok(FolderStatisticsBucket {
            id: row.get(0)?,
            label: row.get(1)?,
            count: non_negative(row.get::<_, i64>(2)?),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn document_summaries(
    connection: &rusqlite::Connection,
    sql: &str,
) -> Result<Vec<DocumentSummary>, StatisticsError> {
    let mut statement = connection.prepare(sql)?;
    let records = statement
        .query_map([], |row| {
            Ok(DocumentSummary {
                document_id: row.get(0)?,
                file_name: row.get(1)?,
                path: row.get(2)?,
                extension: row.get(3)?,
                size_bytes: non_negative(row.get::<_, i64>(4)?),
                modified_at: row.get(5)?,
                folder_name: row.get(6)?,
                parse_state: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(records)
}

fn search_term_counts(
    connection: &rusqlite::Connection,
) -> Result<Vec<SearchTermCount>, StatisticsError> {
    let mut statement = connection.prepare(
        "SELECT trim(query), COUNT(*), MAX(searched_at)
         FROM search_history
         WHERE private = 0 AND trim(query) != ''
         GROUP BY lower(trim(query))
         ORDER BY COUNT(*) DESC, MAX(searched_at) DESC
         LIMIT 20",
    )?;
    let records = statement
        .query_map([], |row| {
            Ok(SearchTermCount {
                query: row.get(0)?,
                count: non_negative(row.get::<_, i64>(1)?),
                last_searched_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(records)
}

fn history_rows(
    connection: &rusqlite::Connection,
    limit: u32,
    offset: u32,
) -> Result<Vec<SearchHistoryRecord>, StatisticsError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut statement = connection.prepare(
        "SELECT id, query, mode, filters_json, result_count, elapsed_ms, searched_at, private
         FROM search_history
         WHERE private = 0
         ORDER BY searched_at DESC, id DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let raw = statement
        .query_map(params![i64::from(limit), i64::from(offset)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, bool>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(
            |(
                id,
                query,
                mode,
                filters_json,
                result_count,
                elapsed_ms,
                searched_at,
                private_search,
            )| {
                Ok(SearchHistoryRecord {
                    id,
                    query,
                    mode,
                    filters: serde_json::from_str(&filters_json)
                        .map_err(StatisticsError::HistoryFilters)?,
                    result_count: non_negative(result_count),
                    elapsed_ms: non_negative(elapsed_ms),
                    searched_at,
                    private_search,
                })
            },
        )
        .collect()
}

fn non_negative(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentStatistics {
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub total_documents: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub indexed_documents: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub total_bytes: u64,
    pub by_extension: Vec<StatisticsBucket>,
    pub by_folder: Vec<FolderStatisticsBucket>,
    pub by_year: Vec<StatisticsBucket>,
    pub recently_modified: Vec<DocumentSummary>,
    pub largest_documents: Vec<DocumentSummary>,
    pub parse_states: Vec<StatisticsBucket>,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub total_searches: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub unique_search_terms: u64,
    pub frequent_searches: Vec<SearchTermCount>,
    pub recent_searches: Vec<SearchHistoryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsBucket {
    pub label: String,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FolderStatisticsBucket {
    pub id: String,
    pub label: String,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub document_id: String,
    pub file_name: String,
    pub path: String,
    pub extension: String,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub size_bytes: u64,
    pub modified_at: String,
    pub folder_name: String,
    pub parse_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchTermCount {
    pub query: String,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub count: u64,
    pub last_searched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHistoryRecord {
    pub id: String,
    pub query: String,
    pub mode: String,
    pub filters: serde_json::Value,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub result_count: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub elapsed_ms: u64,
    pub searched_at: String,
    pub private_search: bool,
}

fn serialize_u64_decimal<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&value.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParseErrorRecord {
    pub document_id: String,
    pub file_name: String,
    pub path: String,
    pub error_code: String,
    pub modified_at: String,
    pub folder_name: String,
}

#[derive(Debug, Error)]
pub enum StatisticsError {
    #[error("statistics database operation failed")]
    Database(#[from] rusqlite::Error),
    #[error("search history filters are invalid")]
    HistoryFilters(#[source] serde_json::Error),
    #[error("invalid statistics request: {0}")]
    InvalidInput(String),
}

#[cfg(test)]
mod tests {
    use super::{count_buckets, BY_YEAR_SQL};

    #[test]
    fn yearly_counts_normalize_iso_and_epoch_timestamps() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE documents (modified_at TEXT NOT NULL);
                 INSERT INTO documents (modified_at) VALUES
                   ('2026-08-01T12:00:00Z'),
                   ('1767225600000000000'),
                   ('1767225600000'),
                   ('1735689600000000000'),
                   ('not-a-date');",
            )
            .unwrap();

        let buckets = count_buckets(&connection, BY_YEAR_SQL).unwrap();

        assert_eq!(buckets[0].label, "2026");
        assert_eq!(buckets[0].count, 3);
        assert_eq!(buckets[1].label, "2025");
        assert_eq!(buckets[1].count, 1);
    }
}

impl StatisticsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "STATISTICS_DATABASE_FAILED",
            Self::HistoryFilters(_) => "HISTORY_FILTERS_INVALID",
            Self::InvalidInput(_) => "STATISTICS_INPUT_INVALID",
        }
    }
}
