use std::sync::Arc;

use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use everyfile_lib::library::repository::{LibraryError, LibraryRepository};
use rusqlite::params;
use tempfile::TempDir;
use zeroize::Zeroizing;

#[test]
fn bookmark_updates_note_without_duplicating_the_document() {
    let fixture = Fixture::new();
    fixture.seed_document("doc-1", "report.pdf");

    fixture.library.set_bookmark("doc-1", "첫 메모").unwrap();
    fixture.library.set_bookmark("doc-1", "수정 메모").unwrap();

    let bookmarks = fixture.library.list_bookmarks().unwrap();
    assert_eq!(bookmarks.len(), 1);
    assert_eq!(bookmarks[0].document_id, "doc-1");
    assert_eq!(bookmarks[0].note, "수정 메모");
}

#[test]
fn metadata_only_images_have_a_preview_shell_for_binary_loading() {
    let fixture = Fixture::new();
    fixture.seed_document("doc-image", "photo.png");
    fixture
        .database
        .connection()
        .execute(
            "UPDATE documents
             SET extension = 'png', parse_state = 'metadata_only'
             WHERE id = 'doc-image'",
            [],
        )
        .unwrap();

    let preview = fixture.library.get_preview("doc-image").unwrap();

    assert_eq!(preview.file_name, "photo.png");
    assert_eq!(preview.extension, "png");
    assert!(preview.markdown.is_empty());
    assert!(preview.blocks.is_empty());
    assert!(preview.warnings.is_empty());
}

#[test]
fn tags_are_case_insensitively_unique_and_reject_unapproved_colors() {
    let fixture = Fixture::new();
    let tag = fixture.library.create_tag("Work", "terracotta").unwrap();
    let duplicate = fixture.library.create_tag(" work ", "amber").unwrap();

    assert_eq!(duplicate.id, tag.id);
    assert_eq!(duplicate.name, "Work");
    assert!(matches!(
        fixture.library.create_tag("Private", "#fff"),
        Err(LibraryError::InvalidTagColor)
    ));
}

#[test]
fn document_tags_are_replaced_atomically_and_cascade_with_documents() {
    let fixture = Fixture::new();
    fixture.seed_document("doc-1", "report.pdf");
    let work = fixture.library.create_tag("Work", "terracotta").unwrap();
    let review = fixture.library.create_tag("Review", "amber").unwrap();

    fixture
        .library
        .set_document_tags("doc-1", &[work.id.clone(), review.id.clone()])
        .unwrap();
    let tags = fixture
        .library
        .set_document_tags("doc-1", std::slice::from_ref(&review.id))
        .unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].id, review.id);

    fixture
        .database
        .connection()
        .execute("DELETE FROM documents WHERE id = 'doc-1'", [])
        .unwrap();
    let relation_count: i64 = fixture
        .database
        .connection()
        .query_row("SELECT COUNT(*) FROM document_tags", [], |row| row.get(0))
        .unwrap();
    assert_eq!(relation_count, 0);
}

#[test]
fn bookmarks_notes_and_tags_survive_restart_then_cascade_with_the_folder() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("restart.db");
    let database = Arc::new(
        Database::open(&path, &SecretKey::from_bytes(Zeroizing::new([71_u8; 32]))).unwrap(),
    );
    database.migrate().unwrap();
    database
        .connection()
        .execute_batch(
            "INSERT INTO folders
             (id, canonical_path, display_name, created_at, enabled)
             VALUES ('folder-restart', 'C:\\restart', 'Restart', 'now', 1);
             INSERT INTO documents
             (id, folder_id, canonical_path, file_name, extension, size_bytes,
              modified_at, parse_state)
             VALUES ('doc-restart', 'folder-restart', 'C:\\restart\\report.pdf',
                     'report.pdf', 'pdf', 8, 'now', 'parsed');
             INSERT INTO document_content
             (document_id, title, body, markdown, blocks_json, warnings_json)
             VALUES ('doc-restart', 'Restart', 'Body', 'Body', '[]', '[]');",
        )
        .unwrap();
    let library = LibraryRepository::new(Arc::clone(&database));
    library
        .set_bookmark("doc-restart", "재시작 후에도 남는 메모")
        .unwrap();
    let tag = library.create_tag("Restart", "terracotta").unwrap();
    library
        .set_document_tags("doc-restart", std::slice::from_ref(&tag.id))
        .unwrap();
    drop(library);
    drop(database);

    let reopened = Arc::new(
        Database::open(&path, &SecretKey::from_bytes(Zeroizing::new([71_u8; 32]))).unwrap(),
    );
    reopened.migrate().unwrap();
    let library = LibraryRepository::new(Arc::clone(&reopened));
    assert_eq!(
        library.list_bookmarks().unwrap()[0].note,
        "재시작 후에도 남는 메모"
    );
    assert_eq!(
        library.get_preview("doc-restart").unwrap().tags[0].name,
        "Restart"
    );

    reopened
        .connection()
        .execute("DELETE FROM folders WHERE id = 'folder-restart'", [])
        .unwrap();
    let remaining: i64 = reopened
        .connection()
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM documents) +
               (SELECT COUNT(*) FROM bookmarks) +
               (SELECT COUNT(*) FROM document_tags)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0);
}

#[test]
fn preview_normalizes_untrusted_parser_blocks_and_warnings() {
    let fixture = Fixture::new();
    fixture.seed_document("doc-1", "unsafe.pdf");
    fixture
        .database
        .connection()
        .execute(
            "INSERT INTO document_content
             (document_id, title, body, markdown, blocks_json, warnings_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "doc-1",
                "<img onerror=alert(1)>",
                "body",
                "# title",
                r#"[{"type":"heading","text":"<script>x</script>","level":99,"href":"javascript:alert(1)"},{"type":"table","table":{"rows":1,"cols":1,"hasHeader":true,"cells":[[{"text":"cell","colSpan":1,"rowSpan":1}]]}}]"#,
                r#"[{"code":"PARTIAL_PARSE","message":"warning","page":1}]"#,
            ],
        )
        .unwrap();

    let preview = fixture.library.get_preview("doc-1").unwrap();

    assert_eq!(preview.document_id, "doc-1");
    assert_eq!(preview.blocks[0].kind, "heading");
    assert_eq!(preview.blocks[0].level, Some(6));
    assert_eq!(preview.blocks[0].href, None);
    assert_eq!(
        preview.blocks[1].table.as_ref().unwrap().cells[0][0].text,
        "cell"
    );
    assert_eq!(preview.warnings[0].code, "PARTIAL_PARSE");
}

#[test]
fn preview_rejects_stored_json_above_the_strict_input_cap() {
    let fixture = Fixture::new();
    fixture.seed_document("doc-large", "large.pdf");
    let oversized = format!(
        r#"[{{"type":"paragraph","text":"{}"}}]"#,
        "x".repeat(9 * 1024 * 1024)
    );
    fixture
        .database
        .connection()
        .execute(
            "INSERT INTO document_content
             (document_id, title, body, markdown, blocks_json, warnings_json)
             VALUES ('doc-large', NULL, '', '', ?1, '[]')",
            [oversized],
        )
        .unwrap();

    assert!(matches!(
        fixture.library.get_preview("doc-large"),
        Err(LibraryError::PreviewTooLarge)
    ));
}

#[test]
fn preview_uses_one_aggregate_budget_for_every_returned_string() {
    let fixture = Fixture::new();
    fixture.seed_document("doc-budget", "budget.pdf");
    let tag = fixture
        .library
        .create_tag("중요 문서", "terracotta")
        .unwrap();
    fixture
        .library
        .set_document_tags("doc-budget", &[tag.id])
        .unwrap();
    fixture
        .library
        .set_bookmark("doc-budget", &"메".repeat(600_000))
        .unwrap();
    let blocks = serde_json::json!([
        {
            "type": "paragraph",
            "text": "가".repeat(400_000),
            "href": "https://example.com/document",
            "listType": "ordered",
            "children": [{
                "type": "table",
                "table": {
                    "hasHeader": false,
                    "cells": [[
                        {"text": "나".repeat(400_000), "colSpan": 1, "rowSpan": 1}
                    ]]
                }
            }]
        }
    ]);
    let warnings = serde_json::json!([{
        "code": "PARTIAL_PARSE",
        "message": "경".repeat(300_000),
        "page": 1
    }]);
    fixture
        .database
        .connection()
        .execute(
            "INSERT INTO document_content
             (document_id, title, body, markdown, blocks_json, warnings_json)
             VALUES ('doc-budget', NULL, '', ?1, ?2, ?3)",
            params![
                "라".repeat(500_000),
                blocks.to_string(),
                warnings.to_string()
            ],
        )
        .unwrap();

    let preview = fixture.library.get_preview("doc-budget").unwrap();
    assert_eq!(preview.tags.len(), 1);
    assert_eq!(count_preview_document_chars(&preview), 2_000_000);
    assert!(!preview.bookmark_note.is_empty());
    assert!(preview.truncated);
}

#[test]
fn preview_does_not_report_truncation_when_the_last_node_exactly_fits() {
    let fixture = Fixture::new();
    fixture.seed_document("doc-nodes", "nodes.pdf");
    let blocks = vec![serde_json::json!({"type": "separator"}); 20_000];
    fixture
        .database
        .connection()
        .execute(
            "INSERT INTO document_content
             (document_id, title, body, markdown, blocks_json, warnings_json)
             VALUES ('doc-nodes', NULL, '', '', ?1, '[]')",
            [serde_json::to_string(&blocks).unwrap()],
        )
        .unwrap();

    let preview = fixture.library.get_preview("doc-nodes").unwrap();

    assert_eq!(preview.blocks.len(), 20_000);
    assert!(!preview.truncated);
}

#[test]
fn preview_reports_truncation_when_one_node_is_omitted() {
    let fixture = Fixture::new();
    fixture.seed_document("doc-nodes-over", "nodes-over.pdf");
    let blocks = vec![serde_json::json!({"type": "separator"}); 20_001];
    fixture
        .database
        .connection()
        .execute(
            "INSERT INTO document_content
             (document_id, title, body, markdown, blocks_json, warnings_json)
             VALUES ('doc-nodes-over', NULL, '', '', ?1, '[]')",
            [serde_json::to_string(&blocks).unwrap()],
        )
        .unwrap();

    let preview = fixture.library.get_preview("doc-nodes-over").unwrap();

    assert_eq!(preview.blocks.len(), 20_000);
    assert!(preview.truncated);
}

fn count_preview_document_chars(preview: &everyfile_lib::domain::models::PreviewDocument) -> usize {
    preview.document_id.chars().count()
        + preview.file_name.chars().count()
        + preview.path.chars().count()
        + preview.extension.chars().count()
        + preview.markdown.chars().count()
        + preview
            .blocks
            .iter()
            .map(count_preview_block_chars)
            .sum::<usize>()
        + preview
            .warnings
            .iter()
            .map(|warning| warning.code.chars().count() + warning.message.chars().count())
            .sum::<usize>()
        + preview.bookmark_note.chars().count()
        + preview
            .tags
            .iter()
            .map(|tag| {
                tag.id.chars().count() + tag.name.chars().count() + tag.color.chars().count()
            })
            .sum::<usize>()
}

fn count_preview_block_chars(block: &everyfile_lib::domain::models::PreviewBlock) -> usize {
    block.kind.chars().count()
        + block.text.chars().count()
        + block
            .href
            .as_ref()
            .map(|href| href.chars().count())
            .unwrap_or_default()
        + block
            .list_type
            .as_ref()
            .map(|list_type| list_type.chars().count())
            .unwrap_or_default()
        + block
            .table
            .as_ref()
            .map(|table| {
                table
                    .cells
                    .iter()
                    .flatten()
                    .map(|cell| cell.text.chars().count())
                    .sum::<usize>()
            })
            .unwrap_or_default()
        + block
            .children
            .iter()
            .map(count_preview_block_chars)
            .sum::<usize>()
}

struct Fixture {
    _temp: TempDir,
    database: Arc<Database>,
    library: LibraryRepository,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let key = SecretKey::from_bytes(Zeroizing::new([61_u8; 32]));
        let database = Arc::new(Database::open(&temp.path().join("library.db"), &key).unwrap());
        database.migrate().unwrap();
        let library = LibraryRepository::new(Arc::clone(&database));
        Self {
            _temp: temp,
            database,
            library,
        }
    }

    fn seed_document(&self, id: &str, file_name: &str) {
        let root = self._temp.path().join("documents");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(file_name);
        std::fs::write(&path, b"%PDF-1.7").unwrap();
        let connection = self.database.connection();
        connection
            .execute(
                "INSERT OR IGNORE INTO folders
                 (id, canonical_path, display_name, created_at, enabled)
                 VALUES ('folder-1', ?1, 'Documents', '2026-01-01T00:00:00Z', 1)",
                [root.to_string_lossy().as_ref()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO documents
                 (id, folder_id, canonical_path, file_name, extension, size_bytes,
                  modified_at, parse_state)
                 VALUES (?1, 'folder-1', ?2, ?3, 'pdf', 8,
                         '2026-01-01T00:00:00Z', 'parsed')",
                params![id, path.to_string_lossy().as_ref(), file_name],
            )
            .unwrap();
    }
}
