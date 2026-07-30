use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rand::RngExt;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter};
use serde::Serialize;
use thiserror::Error;

use crate::domain::models::{
    SearchHit, SearchMatchKind, SearchMode, SearchRequest, SearchResponse, TermMode,
};
use crate::indexing::ActivityLimiter;
use crate::infrastructure::database::Database;

use super::query::{validate_date, validate_extension};
use super::{ParsedQuery, SearchLease, SearchRegistry};

const DEFAULT_PAGE_SIZE: u32 = 100;
const MAX_PAGE_SIZE: u32 = 200;

#[derive(Clone)]
pub struct SearchRepository {
    database: Arc<Database>,
    limiter: ActivityLimiter,
    registry: SearchRegistry,
}

impl SearchRepository {
    pub fn new(
        database: Arc<Database>,
        limiter: ActivityLimiter,
        registry: SearchRegistry,
    ) -> Self {
        Self {
            database,
            limiter,
            registry,
        }
    }

    pub fn search(&self, request: &SearchRequest) -> Result<SearchResponse, SearchError> {
        let lease = self.begin_request(&request.request_id)?;
        self.search_registered(request, lease)
    }

    pub fn begin_request(&self, request_id: &str) -> Result<SearchLease, SearchError> {
        self.registry.begin(request_id)
    }

    pub fn search_registered(
        &self,
        request: &SearchRequest,
        lease: SearchLease,
    ) -> Result<SearchResponse, SearchError> {
        if request.request_id != lease.request_id() {
            return Err(SearchError::invalid_request(
                "search lease does not belong to this request",
            ));
        }
        let _foreground = self.limiter.begin_foreground();
        let started = Instant::now();
        let parsed = ParsedQuery::parse(&request.query)?;
        validate_request(request, &parsed)?;
        let limit = if request.limit == 0 {
            DEFAULT_PAGE_SIZE
        } else {
            request.limit.min(MAX_PAGE_SIZE)
        };
        let query = SearchSql::build(request, &parsed)?;
        let connection = self.database.connection();
        let execution = lease.begin_execution(&connection)?;

        let total_result = connection.query_row(
            &query.count_sql,
            params_from_iter(query.filter_values.iter()),
            |row| row.get::<_, i64>(0),
        );
        lease.ensure_current()?;
        let total = total_result?;
        let mut hit_values = query.filter_values.clone();
        hit_values.push(Value::Integer(i64::from(limit)));
        hit_values.push(Value::Integer(i64::from(request.offset)));
        let statement_result = connection.prepare(&query.hits_sql);
        lease.ensure_current()?;
        let mut statement = statement_result?;
        let rows_result = statement.query_map(params_from_iter(hit_values.iter()), |row| {
            let size_bytes = row.get::<_, i64>(4)?;
            Ok(SearchHit {
                document_id: row.get(0)?,
                file_name: row.get(1)?,
                path: row.get(2)?,
                extension: row.get(3)?,
                size_bytes: u64::try_from(size_bytes).unwrap_or_default(),
                modified_at: row.get(5)?,
                snippet: row.get(6)?,
                score: row.get(7)?,
                match_kind: match row.get::<_, String>(8)?.as_str() {
                    "filename" => SearchMatchKind::Filename,
                    "content" => SearchMatchKind::Content,
                    "both" => SearchMatchKind::Both,
                    _ => SearchMatchKind::Metadata,
                },
            })
        });
        lease.ensure_current()?;
        let hits_result = rows_result?.collect::<Result<Vec<_>, _>>();
        lease.ensure_current()?;
        let hits = hits_result?;
        drop(statement);
        drop(execution);
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let total = u64::try_from(total).unwrap_or_default();
        let has_more = u64::from(request.offset).saturating_add(hits.len() as u64) < total;

        let response = SearchResponse {
            request_id: request.request_id.clone(),
            hits,
            total,
            elapsed_ms,
            applied_filters: query.applied_filters,
            has_more,
        };
        lease.finish(|| {
            if !request.private_search && request.offset == 0 && !parsed.is_empty() {
                let filters_json = serde_json::to_string(&HistoryFilters::from(request, &parsed))?;
                connection.execute(
                    "INSERT INTO search_history
                     (id, query, mode, filters_json, result_count, elapsed_ms, searched_at, private)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 0)",
                    params![
                        random_id(),
                        request.query,
                        mode_name(&request.mode),
                        filters_json,
                        i64::try_from(total).unwrap_or(i64::MAX),
                        i64::try_from(elapsed_ms).unwrap_or(i64::MAX),
                    ],
                )?;
            }
            Ok(())
        })?;
        Ok(response)
    }
}

struct SearchSql {
    count_sql: String,
    hits_sql: String,
    filter_values: Vec<Value>,
    applied_filters: Vec<String>,
}

impl SearchSql {
    fn build(request: &SearchRequest, parsed: &ParsedQuery) -> Result<Self, SearchError> {
        let mut conditions = Vec::new();
        let mut values = Vec::new();
        let mut applied_filters = Vec::new();
        let mut fts = false;

        match request.mode {
            SearchMode::Keyword => {
                if let Some(expression) =
                    parsed.fts_match_expression(request.include_filename, request.term_mode)
                {
                    conditions.push("document_fts MATCH ?".to_owned());
                    values.push(expression.into());
                    applied_filters.push("query".into());
                    fts = true;
                } else {
                    for expression in
                        parsed.excluded_fts_expressions(request.include_filename, request.term_mode)
                    {
                        conditions.push(
                            "NOT EXISTS (
                               SELECT 1 FROM document_fts
                               WHERE document_fts.document_id = d.id
                                 AND document_fts MATCH ?
                             )"
                            .into(),
                        );
                        values.push(expression.into());
                    }
                    if !parsed.excluded_terms.is_empty()
                        || (request.term_mode == TermMode::Exclude
                            && (!parsed.terms.is_empty() || !parsed.phrases.is_empty()))
                    {
                        conditions.push(
                            "EXISTS (
                               SELECT 1 FROM document_fts indexed_fts
                               WHERE indexed_fts.document_id = d.id
                             )"
                            .into(),
                        );
                        applied_filters.push("query".into());
                    }
                }
                if !request.include_filename {
                    applied_filters.push("contentOnly".into());
                }
            }
            SearchMode::Filename => {
                let positive = parsed.positive_groups.iter().flatten().collect::<Vec<_>>();
                if parsed.match_any {
                    let mut alternatives = Vec::new();
                    for group in &parsed.positive_groups {
                        let mut members = Vec::new();
                        for value in group {
                            values.push(format!("%{}%", escape_like(value)).into());
                            members.push(format!(
                                "LOWER(d.file_name) LIKE LOWER(?{}) ESCAPE '\\'",
                                values.len()
                            ));
                        }
                        alternatives.push(format!("({})", members.join(" AND ")));
                    }
                    conditions.push(format!("({})", alternatives.join(" OR ")));
                } else {
                    let selected = match request.term_mode {
                        TermMode::Exact if !positive.is_empty() => vec![positive
                            .iter()
                            .map(|value| value.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")],
                        _ => positive.iter().map(|value| (*value).clone()).collect(),
                    };
                    let mut selected_conditions = Vec::new();
                    for value in selected {
                        values.push(format!("%{}%", escape_like(&value)).into());
                        let comparison = if request.term_mode == TermMode::Exclude {
                            "NOT LIKE"
                        } else {
                            "LIKE"
                        };
                        selected_conditions.push(format!(
                            "LOWER(d.file_name) {comparison} LOWER(?{}) ESCAPE '\\'",
                            values.len()
                        ));
                    }
                    if request.term_mode == TermMode::Any && !selected_conditions.is_empty() {
                        conditions.push(format!("({})", selected_conditions.join(" OR ")));
                    } else {
                        conditions.extend(selected_conditions);
                    }
                }
                for value in &parsed.excluded_terms {
                    conditions.push("LOWER(d.file_name) NOT LIKE LOWER(?) ESCAPE '\\'".into());
                    values.push(format!("%{}%", escape_like(value)).into());
                }
                if !parsed.terms.is_empty()
                    || !parsed.phrases.is_empty()
                    || !parsed.excluded_terms.is_empty()
                {
                    applied_filters.push("query".into());
                }
            }
        }

        add_list_filter(
            &mut conditions,
            &mut values,
            &mut applied_filters,
            "d.folder_id",
            &request.folder_ids,
            "folder",
        );
        add_list_filter(
            &mut conditions,
            &mut values,
            &mut applied_filters,
            "LOWER(d.extension)",
            &request
                .extensions
                .iter()
                .map(|extension| validate_extension(extension))
                .collect::<Result<Vec<_>, _>>()?,
            "extension",
        );
        if request.extensionless {
            conditions.push("TRIM(d.extension) = ''".into());
            applied_filters.push("extensionless".into());
        }
        add_list_filter(
            &mut conditions,
            &mut values,
            &mut applied_filters,
            "LOWER(d.extension)",
            &parsed.extensions,
            "extension",
        );
        for path_term in &parsed.path_terms {
            conditions.push("d.canonical_path LIKE ? ESCAPE '\\'".into());
            values.push(format!("%{}%", escape_like(path_term)).into());
        }
        if !parsed.path_terms.is_empty() {
            applied_filters.push("path".into());
        }
        add_date_filter(
            &mut conditions,
            &mut values,
            &mut applied_filters,
            request.modified_after.as_deref(),
            ">=",
            "after",
        )?;
        add_date_filter(
            &mut conditions,
            &mut values,
            &mut applied_filters,
            parsed.after.as_deref(),
            ">=",
            "after",
        )?;
        add_date_filter(
            &mut conditions,
            &mut values,
            &mut applied_filters,
            request.modified_before.as_deref(),
            "<=",
            "before",
        )?;
        add_date_filter(
            &mut conditions,
            &mut values,
            &mut applied_filters,
            parsed.before.as_deref(),
            "<=",
            "before",
        )?;

        let from = if fts {
            "FROM document_fts JOIN documents d ON d.id = document_fts.document_id"
        } else {
            "FROM documents d"
        };
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        let score = if fts { "bm25(document_fts)" } else { "0.0" };
        let snippet = if fts {
            "snippet(document_fts, -1, '<mark>', '</mark>', '…', 24)"
        } else {
            "NULL"
        };
        let match_kind = if fts {
            "CASE
               WHEN highlight(document_fts, 1, '<mark>', '</mark>') != document_fts.file_name
                AND (highlight(document_fts, 2, '<mark>', '</mark>') != document_fts.title
                  OR highlight(document_fts, 3, '<mark>', '</mark>') != document_fts.body)
                 THEN 'both'
               WHEN highlight(document_fts, 1, '<mark>', '</mark>') != document_fts.file_name
                 THEN 'filename'
               ELSE 'content'
             END"
        } else if matches!(request.mode, SearchMode::Filename) {
            "'filename'"
        } else {
            "'metadata'"
        };
        let order = sort_clause(&request.sort, fts)?;
        let count_sql = format!("SELECT COUNT(*) {from}{where_clause}");
        let limit_parameter = values.len() + 1;
        let offset_parameter = values.len() + 2;
        let hits_sql = format!(
            "SELECT d.id, d.file_name, d.canonical_path, d.extension, d.size_bytes,
                    d.modified_at, {snippet}, {score} AS score, {match_kind}
             {from}{where_clause}
             ORDER BY {order}
             LIMIT ?{limit_parameter} OFFSET ?{offset_parameter}"
        );
        Ok(Self {
            count_sql,
            hits_sql,
            filter_values: values,
            applied_filters,
        })
    }
}

fn validate_request(request: &SearchRequest, parsed: &ParsedQuery) -> Result<(), SearchError> {
    sort_clause(&request.sort, matches!(request.mode, SearchMode::Keyword))?;
    if parsed.match_any && request.term_mode != TermMode::Any {
        return Err(SearchError::invalid_request(
            "explicit OR syntax requires the any term mode",
        ));
    }
    if parsed.near.is_some() && request.term_mode != TermMode::Near {
        return Err(SearchError::invalid_request(
            "explicit near syntax requires the near term mode",
        ));
    }
    if parsed.positive_groups.is_empty()
        && !parsed.excluded_terms.is_empty()
        && request.term_mode != TermMode::Exclude
    {
        return Err(SearchError::invalid_request(
            "an exclusion-only query requires the exclude term mode",
        ));
    }
    if matches!(request.mode, SearchMode::Filename) {
        if !request.include_filename {
            return Err(SearchError::invalid_request(
                "filename mode always includes the filename",
            ));
        }
        if request.term_mode == TermMode::Near || parsed.near.is_some() {
            return Err(SearchError::invalid_request(
                "near search is available only in keyword mode",
            ));
        }
        if request.sort == "confidence" {
            return Err(SearchError::invalid_request(
                "confidence sort is available only in keyword mode",
            ));
        }
    }
    if request.term_mode == TermMode::Near && parsed.terms.len() + parsed.phrases.len() < 2 {
        return Err(SearchError::invalid_request(
            "near search requires at least two positive terms or phrases",
        ));
    }
    if let (Some(after), Some(before)) = (
        request.modified_after.as_deref(),
        request.modified_before.as_deref(),
    ) {
        validate_date(after)?;
        validate_date(before)?;
        if after > before {
            return Err(SearchError::invalid_request(
                "modified-after date cannot follow modified-before date",
            ));
        }
    }
    Ok(())
}

fn add_list_filter(
    conditions: &mut Vec<String>,
    values: &mut Vec<Value>,
    applied_filters: &mut Vec<String>,
    column: &str,
    items: &[String],
    label: &str,
) {
    if items.is_empty() {
        return;
    }
    let placeholders = items
        .iter()
        .map(|item| {
            values.push(item.clone().into());
            format!("?{}", values.len())
        })
        .collect::<Vec<_>>();
    conditions.push(format!("{column} IN ({})", placeholders.join(", ")));
    if !applied_filters.iter().any(|filter| filter == label) {
        applied_filters.push(label.to_owned());
    }
}

fn add_date_filter(
    conditions: &mut Vec<String>,
    values: &mut Vec<Value>,
    applied_filters: &mut Vec<String>,
    value: Option<&str>,
    operator: &str,
    label: &str,
) -> Result<(), SearchError> {
    if let Some(value) = value {
        validate_date(value)?;
        values.push(value.to_owned().into());
        conditions.push(format!(
            "substr(d.modified_at, 1, 10) {operator} ?{}",
            values.len()
        ));
        if !applied_filters.iter().any(|filter| filter == label) {
            applied_filters.push(label.to_owned());
        }
    }
    Ok(())
}

fn sort_clause(sort: &str, has_score: bool) -> Result<&'static str, SearchError> {
    match sort {
        "relevance" if has_score => Ok("score ASC, d.modified_at DESC, d.id ASC"),
        "confidence" if has_score => Ok("score ASC, d.id ASC"),
        "relevance" => Ok("d.file_name COLLATE NOCASE ASC, d.id ASC"),
        "newest" => Ok("d.modified_at DESC, d.id ASC"),
        "oldest" => Ok("d.modified_at ASC, d.id ASC"),
        "name" => Ok("d.file_name COLLATE NOCASE ASC, d.id ASC"),
        "size" => Ok("d.size_bytes DESC, d.id ASC"),
        _ => Err(SearchError::invalid_request("unsupported search sort")),
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn mode_name(mode: &SearchMode) -> &'static str {
    match mode {
        SearchMode::Keyword => "keyword",
        SearchMode::Filename => "filename",
    }
}

fn random_id() -> String {
    let random = rand::rng().random::<[u8; 16]>();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("search-{timestamp:x}-{suffix}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryFilters<'a> {
    folder_ids: &'a [String],
    extensions: &'a [String],
    parsed_extensions: &'a [String],
    path_terms: &'a [String],
    modified_after: Option<&'a str>,
    modified_before: Option<&'a str>,
    include_filename: bool,
    term_mode: &'a str,
    sort: &'a str,
}

impl<'a> HistoryFilters<'a> {
    fn from(request: &'a SearchRequest, parsed: &'a ParsedQuery) -> Self {
        Self {
            folder_ids: &request.folder_ids,
            extensions: &request.extensions,
            parsed_extensions: &parsed.extensions,
            path_terms: &parsed.path_terms,
            modified_after: request
                .modified_after
                .as_deref()
                .or(parsed.after.as_deref()),
            modified_before: request
                .modified_before
                .as_deref()
                .or(parsed.before.as_deref()),
            include_filename: request.include_filename,
            term_mode: term_mode_name(request.term_mode),
            sort: &request.sort,
        }
    }
}

fn term_mode_name(mode: TermMode) -> &'static str {
    match mode {
        TermMode::All => "all",
        TermMode::Any => "any",
        TermMode::Exact => "exact",
        TermMode::Exclude => "exclude",
        TermMode::Near => "near",
    }
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("invalid search query: {0}")]
    InvalidQuery(String),
    #[error("invalid search request: {0}")]
    InvalidRequest(String),
    #[error("search database operation failed")]
    Database(#[from] rusqlite::Error),
    #[error("search history serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("search request was cancelled or superseded")]
    Cancelled,
}

impl SearchError {
    pub(crate) fn invalid_query(message: impl Into<String>) -> Self {
        Self::InvalidQuery(message.into())
    }

    pub(crate) fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest(message.into())
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidQuery(_) => "SEARCH_QUERY_INVALID",
            Self::InvalidRequest(_) => "SEARCH_REQUEST_INVALID",
            Self::Database(_) => "SEARCH_DATABASE_FAILED",
            Self::Serialization(_) => "SEARCH_HISTORY_FAILED",
            Self::Cancelled => "SEARCH_CANCELLED",
        }
    }
}
