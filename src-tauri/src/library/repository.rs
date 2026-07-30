use std::sync::Arc;

use rand::RngExt;
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use thiserror::Error;

use crate::domain::models::{
    BookmarkRecord, PreviewBlock, PreviewCell, PreviewDocument, PreviewTable, PreviewWarning,
    TagRecord,
};
use crate::infrastructure::database::Database;

const APPROVED_COLORS: &[&str] = &[
    "terracotta",
    "amber",
    "brown",
    "sand",
    "rose",
    "slate",
    "blue",
    "violet",
];
const MAX_PREVIEW_NODES: usize = 20_000;
const MAX_PREVIEW_CHARS: usize = 2_000_000;
const MAX_STORED_FIELD_BYTES: usize = 8 * 1024 * 1024;
const MAX_STORED_PREVIEW_BYTES: usize = 12 * 1024 * 1024;
const MAX_TABLE_ROWS: usize = 2_000;
const MAX_TABLE_COLS: usize = 200;

#[derive(Clone)]
pub struct LibraryRepository {
    database: Arc<Database>,
}

impl LibraryRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn get_preview(&self, document_id: &str) -> Result<PreviewDocument, LibraryError> {
        validate_document_id(document_id)?;
        let connection = self.database.connection();
        let row = connection
            .query_row(
                "SELECT d.file_name, d.canonical_path, d.extension,
                        c.markdown, c.blocks_json, c.warnings_json,
                        COALESCE(b.note, '')
                 FROM documents d
                 JOIN folders f ON f.id = d.folder_id AND f.enabled = 1
                 JOIN document_content c ON c.document_id = d.id
                 LEFT JOIN bookmarks b ON b.document_id = d.id
                 WHERE d.id = ?1",
                [document_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()?
            .ok_or(LibraryError::DocumentNotFound)?;

        if row.3.len() > MAX_STORED_FIELD_BYTES
            || row.4.len() > MAX_STORED_FIELD_BYTES
            || row.5.len() > MAX_STORED_FIELD_BYTES
            || row
                .3
                .len()
                .saturating_add(row.4.len())
                .saturating_add(row.5.len())
                > MAX_STORED_PREVIEW_BYTES
        {
            return Err(LibraryError::PreviewTooLarge);
        }
        let blocks_value: Value =
            serde_json::from_str(&row.4).map_err(|_| LibraryError::InvalidPreviewData)?;
        let warnings_value: Value =
            serde_json::from_str(&row.5).map_err(|_| LibraryError::InvalidPreviewData)?;
        let mut budget = PreviewBudget::new();
        let markdown = budget.take_text(&row.3);
        let blocks = normalize_blocks(blocks_value, &mut budget)?;
        let warnings = normalize_warnings(warnings_value, &mut budget)?;
        let tags = query_document_tags(&connection, document_id)?;

        Ok(PreviewDocument {
            document_id: document_id.to_owned(),
            file_name: bounded_text(&row.0),
            path: row.1,
            extension: row.2.to_ascii_lowercase(),
            markdown,
            blocks,
            warnings,
            bookmarked: !row.6.is_empty()
                || connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM bookmarks WHERE document_id = ?1)",
                    [document_id],
                    |result| result.get(0),
                )?,
            bookmark_note: bounded_text(&row.6),
            tags,
            truncated: budget.truncated,
        })
    }

    pub fn set_bookmark(
        &self,
        document_id: &str,
        note: &str,
    ) -> Result<BookmarkRecord, LibraryError> {
        validate_document_id(document_id)?;
        let note = bounded_text(note.trim());
        let connection = self.database.connection();
        ensure_document(&connection, document_id)?;
        connection.execute(
            "INSERT INTO bookmarks (document_id, note, created_at)
             VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(document_id) DO UPDATE SET note = excluded.note",
            params![document_id, note],
        )?;
        connection
            .query_row(
                "SELECT document_id, note, created_at FROM bookmarks WHERE document_id = ?1",
                [document_id],
                |row| {
                    Ok(BookmarkRecord {
                        document_id: row.get(0)?,
                        note: row.get(1)?,
                        created_at: row.get(2)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn remove_bookmark(&self, document_id: &str) -> Result<(), LibraryError> {
        validate_document_id(document_id)?;
        let connection = self.database.connection();
        ensure_document(&connection, document_id)?;
        connection.execute(
            "DELETE FROM bookmarks WHERE document_id = ?1",
            [document_id],
        )?;
        Ok(())
    }

    pub fn list_bookmarks(&self) -> Result<Vec<BookmarkRecord>, LibraryError> {
        let connection = self.database.connection();
        let mut statement = connection.prepare(
            "SELECT b.document_id, b.note, b.created_at
             FROM bookmarks b JOIN documents d ON d.id = b.document_id
             ORDER BY b.created_at DESC, b.document_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(BookmarkRecord {
                document_id: row.get(0)?,
                note: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn create_tag(&self, name: &str, color: &str) -> Result<TagRecord, LibraryError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(LibraryError::InvalidTagName);
        }
        if !APPROVED_COLORS.contains(&color) {
            return Err(LibraryError::InvalidTagColor);
        }
        let connection = self.database.connection();
        if let Some(existing) = query_tag_by_name(&connection, name)? {
            return Ok(existing);
        }
        let id = random_id("tag");
        match connection.execute(
            "INSERT INTO tags (id, name, color) VALUES (?1, ?2, ?3)",
            params![id, name, color],
        ) {
            Ok(_) => {}
            Err(error)
                if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) =>
            {
                return query_tag_by_name(&connection, name)?.ok_or(LibraryError::Database(error));
            }
            Err(error) => return Err(error.into()),
        }
        query_tag_by_name(&connection, name)?.ok_or(LibraryError::InvalidTagName)
    }

    pub fn set_document_tags(
        &self,
        document_id: &str,
        tag_ids: &[String],
    ) -> Result<Vec<TagRecord>, LibraryError> {
        validate_document_id(document_id)?;
        if tag_ids.len() > 100 {
            return Err(LibraryError::TooManyTags);
        }
        let mut deduplicated = tag_ids.to_vec();
        deduplicated.sort();
        deduplicated.dedup();
        let mut connection = self.database.connection();
        let transaction = connection.transaction()?;
        ensure_document(&transaction, document_id)?;
        if !deduplicated.is_empty() {
            let found: i64 = transaction.query_row(
                &format!(
                    "SELECT COUNT(*) FROM tags WHERE id IN ({})",
                    std::iter::repeat_n("?", deduplicated.len())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                rusqlite::params_from_iter(deduplicated.iter()),
                |row| row.get(0),
            )?;
            if found as usize != deduplicated.len() {
                return Err(LibraryError::TagNotFound);
            }
        }
        transaction.execute(
            "DELETE FROM document_tags WHERE document_id = ?1",
            [document_id],
        )?;
        for tag_id in &deduplicated {
            transaction.execute(
                "INSERT INTO document_tags (document_id, tag_id) VALUES (?1, ?2)",
                params![document_id, tag_id],
            )?;
        }
        transaction.commit()?;
        query_document_tags(&connection, document_id)
    }

    pub fn markdown(&self, document_id: &str) -> Result<(String, String), LibraryError> {
        validate_document_id(document_id)?;
        self.database
            .connection()
            .query_row(
                "SELECT d.file_name, c.markdown
                 FROM documents d
                 JOIN folders f ON f.id = d.folder_id AND f.enabled = 1
                 JOIN document_content c ON c.document_id = d.id
                 WHERE d.id = ?1",
                [document_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or(LibraryError::DocumentNotFound)
    }
}

fn validate_document_id(document_id: &str) -> Result<(), LibraryError> {
    if document_id.trim().is_empty() || document_id.len() > 256 {
        Err(LibraryError::DocumentNotFound)
    } else {
        Ok(())
    }
}

fn ensure_document(
    connection: &rusqlite::Connection,
    document_id: &str,
) -> Result<(), LibraryError> {
    let exists = connection.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM documents d
           JOIN folders f ON f.id = d.folder_id
           WHERE d.id = ?1 AND f.enabled = 1
         )",
        [document_id],
        |row| row.get::<_, bool>(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(LibraryError::DocumentNotFound)
    }
}

fn query_tag_by_name(
    connection: &rusqlite::Connection,
    name: &str,
) -> Result<Option<TagRecord>, LibraryError> {
    connection
        .query_row(
            "SELECT id, name, color FROM tags WHERE name = ?1 COLLATE NOCASE",
            [name],
            |row| {
                Ok(TagRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

fn query_document_tags(
    connection: &rusqlite::Connection,
    document_id: &str,
) -> Result<Vec<TagRecord>, LibraryError> {
    let mut statement = connection.prepare(
        "SELECT t.id, t.name, t.color
         FROM tags t
         JOIN document_tags dt ON dt.tag_id = t.id
         WHERE dt.document_id = ?1
         ORDER BY t.name COLLATE NOCASE, t.id",
    )?;
    let rows = statement.query_map([document_id], |row| {
        Ok(TagRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn normalize_blocks(
    value: Value,
    budget: &mut PreviewBudget,
) -> Result<Vec<PreviewBlock>, LibraryError> {
    let values = value.as_array().ok_or(LibraryError::InvalidPreviewData)?;
    let mut blocks = Vec::new();
    for block in values {
        if let Some(block) = normalize_block(block, 0, budget) {
            blocks.push(block);
        }
        if budget.exhausted() {
            budget.truncated = true;
            break;
        }
    }
    Ok(blocks)
}

fn normalize_block(
    value: &Value,
    depth: usize,
    budget: &mut PreviewBudget,
) -> Option<PreviewBlock> {
    if depth > 16 {
        budget.truncated = true;
        return None;
    }
    if !budget.consume_node() {
        return None;
    }
    let object = value.as_object()?;
    let kind = object.get("type")?.as_str()?;
    if !matches!(
        kind,
        "paragraph" | "heading" | "list" | "table" | "image" | "separator"
    ) {
        return None;
    }
    let text = budget.take_text(
        object
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let level = object
        .get("level")
        .and_then(Value::as_u64)
        .map(|level| level.clamp(1, 6) as u8);
    let page_number = object
        .get("pageNumber")
        .and_then(Value::as_u64)
        .and_then(|page| u32::try_from(page).ok())
        .filter(|page| *page > 0);
    let href = object
        .get("href")
        .and_then(Value::as_str)
        .and_then(safe_href)
        .map(|href| budget.take_text(&href));
    let list_type = object
        .get("listType")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "ordered" | "unordered"))
        .map(str::to_owned);
    let children = object
        .get("children")
        .and_then(Value::as_array)
        .map(|children| {
            let mut normalized = Vec::new();
            for child in children {
                if let Some(child) = normalize_block(child, depth + 1, budget) {
                    normalized.push(child);
                }
                if budget.exhausted() {
                    budget.truncated = true;
                    break;
                }
            }
            normalized
        })
        .unwrap_or_default();
    let table = object
        .get("table")
        .and_then(|table| normalize_table(table, budget));
    Some(PreviewBlock {
        kind: kind.to_owned(),
        text,
        level,
        page_number,
        href,
        list_type,
        children,
        table,
    })
}

fn normalize_table(value: &Value, budget: &mut PreviewBudget) -> Option<PreviewTable> {
    let table = value.as_object()?;
    let source_rows = table.get("cells")?.as_array()?;
    let mut cells = Vec::new();
    for row in source_rows.iter().take(MAX_TABLE_ROWS) {
        let Some(row) = row.as_array() else { continue };
        if row.len() > MAX_TABLE_COLS {
            budget.truncated = true;
        }
        let mut normalized_row = Vec::new();
        for cell in row.iter().take(MAX_TABLE_COLS) {
            if !budget.consume_node() {
                break;
            }
            let Some(cell) = cell.as_object() else {
                continue;
            };
            normalized_row.push(PreviewCell {
                text: budget
                    .take_text(cell.get("text").and_then(Value::as_str).unwrap_or_default()),
                col_span: cell
                    .get("colSpan")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .clamp(1, MAX_TABLE_COLS as u64) as u32,
                row_span: cell
                    .get("rowSpan")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .clamp(1, MAX_TABLE_ROWS as u64) as u32,
            });
            if budget.exhausted() {
                break;
            }
        }
        cells.push(normalized_row);
        if budget.exhausted() {
            break;
        }
    }
    if source_rows.len() > cells.len() {
        budget.truncated = true;
    }
    let rows = cells.len() as u32;
    let cols = cells.iter().map(Vec::len).max().unwrap_or_default() as u32;
    Some(PreviewTable {
        rows,
        cols,
        has_header: table
            .get("hasHeader")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        cells,
    })
}

fn normalize_warnings(
    value: Value,
    budget: &mut PreviewBudget,
) -> Result<Vec<PreviewWarning>, LibraryError> {
    let values = value.as_array().ok_or(LibraryError::InvalidPreviewData)?;
    let mut warnings = Vec::new();
    for warning in values {
        if !budget.consume_node() {
            break;
        }
        let Some(warning) = warning.as_object() else {
            continue;
        };
        let (Some(code), Some(message)) = (
            warning.get("code").and_then(Value::as_str),
            warning.get("message").and_then(Value::as_str),
        ) else {
            continue;
        };
        warnings.push(PreviewWarning {
            code: budget.take_text(code),
            message: budget.take_text(message),
            page: warning
                .get("page")
                .and_then(Value::as_u64)
                .and_then(|page| u32::try_from(page).ok()),
        });
        if budget.exhausted() {
            break;
        }
    }
    if warnings.len() < values.len() {
        budget.truncated = true;
    }
    Ok(warnings)
}

fn safe_href(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    let (scheme, _) = value.split_once(':')?;
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "mailto"
    )
    .then(|| bounded_text(value))
}

fn bounded_text(value: &str) -> String {
    value.chars().take(MAX_PREVIEW_CHARS).collect()
}

struct PreviewBudget {
    remaining_nodes: usize,
    remaining_chars: usize,
    truncated: bool,
}

impl PreviewBudget {
    fn new() -> Self {
        Self {
            remaining_nodes: MAX_PREVIEW_NODES,
            remaining_chars: MAX_PREVIEW_CHARS,
            truncated: false,
        }
    }

    fn consume_node(&mut self) -> bool {
        if self.remaining_nodes == 0 {
            self.truncated = true;
            false
        } else {
            self.remaining_nodes -= 1;
            true
        }
    }

    fn take_text(&mut self, value: &str) -> String {
        let mut chars = value.chars();
        let text = chars
            .by_ref()
            .take(self.remaining_chars)
            .collect::<String>();
        let used = text.chars().count();
        self.remaining_chars -= used;
        if chars.next().is_some() {
            self.truncated = true;
        }
        text
    }

    fn exhausted(&self) -> bool {
        self.remaining_nodes == 0 || self.remaining_chars == 0
    }
}

fn random_id(prefix: &str) -> String {
    format!("{prefix}-{:032x}", rand::rng().random::<u128>())
}

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("indexed document was not found")]
    DocumentNotFound,
    #[error("tag name is invalid")]
    InvalidTagName,
    #[error("tag color is not in the approved palette")]
    InvalidTagColor,
    #[error("one or more tags were not found")]
    TagNotFound,
    #[error("too many tags were supplied")]
    TooManyTags,
    #[error("stored preview data is invalid")]
    InvalidPreviewData,
    #[error("stored preview data exceeds the safe preview limit")]
    PreviewTooLarge,
    #[error("library database operation failed")]
    Database(#[from] rusqlite::Error),
}

impl LibraryError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DocumentNotFound => "LIBRARY_DOCUMENT_NOT_FOUND",
            Self::InvalidTagName => "TAG_NAME_INVALID",
            Self::InvalidTagColor => "TAG_COLOR_INVALID",
            Self::TagNotFound => "TAG_NOT_FOUND",
            Self::TooManyTags => "TOO_MANY_TAGS",
            Self::InvalidPreviewData => "PREVIEW_DATA_INVALID",
            Self::PreviewTooLarge => "PREVIEW_DATA_TOO_LARGE",
            Self::Database(_) => "LIBRARY_DATABASE_FAILED",
        }
    }
}
