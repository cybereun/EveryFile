use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use everyfile_lib::domain::models::AppSettings;
use everyfile_lib::folders::repository::FolderRepository;
use everyfile_lib::indexing::{DocumentOcr, DocumentParser, IndexCoordinator, JobState};
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use everyfile_lib::ocr::{OcrError, OcrMode};
use everyfile_lib::parsing::{ParseErrorCode, ParsedDocument, ParserError};
use tempfile::TempDir;
use zeroize::Zeroizing;

struct ScanAwareParser;

impl DocumentParser for ScanAwareParser {
    fn parse(&self, path: &Path, _max_bytes: u64) -> Result<ParsedDocument, ParserError> {
        if path.extension().and_then(|value| value.to_str()) == Some("pdf") {
            return Ok(ParsedDocument {
                title: Some("Text PDF".into()),
                markdown: "This PDF already has a sufficiently meaningful embedded text layer."
                    .into(),
                plain_text: "This PDF already has a sufficiently meaningful embedded text layer."
                    .into(),
                blocks: vec![],
                metadata: serde_json::json!({}),
                warnings: vec![],
            });
        }
        Err(ParserError::Protocol {
            code: ParseErrorCode::Unsupported,
            message: "image is handled by OCR".into(),
        })
    }
}

#[derive(Default)]
struct FakeLocalOcr {
    calls: AtomicUsize,
}

impl DocumentOcr for FakeLocalOcr {
    fn recognize(
        &self,
        _path: &Path,
        mode: OcrMode,
        _max_bytes: u64,
    ) -> Result<ParsedDocument, OcrError> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Ok(ParsedDocument {
            title: None,
            markdown: "스캔 이미지 고유 본문".into(),
            plain_text: "스캔 이미지 고유 본문".into(),
            blocks: vec![serde_json::json!({"ocr": true})],
            metadata: serde_json::json!({
                "engine": "PaddleOCR",
                "localOnly": true,
                "math": mode == OcrMode::Math
            }),
            warnings: vec![],
        })
    }
}

async fn wait_finished(coordinator: &IndexCoordinator, job_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = coordinator.status(job_id).await.unwrap();
        if matches!(
            status.state,
            JobState::Completed | JobState::Failed | JobState::Cancelled
        ) {
            assert_eq!(status.state, JobState::Completed);
            return;
        }
        assert!(Instant::now() < deadline, "OCR indexing did not finish");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn indexes_images_with_local_ocr_and_skips_text_pdf_ocr() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("registered");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("scan.png"), b"local image fixture").unwrap();
    fs::write(root.join("normal.pdf"), b"local pdf fixture").unwrap();

    let key = SecretKey::from_bytes(Zeroizing::new([41_u8; 32]));
    let database = Arc::new(Database::open(&temp.path().join("ocr.db"), &key).unwrap());
    database.migrate().unwrap();
    let folder = FolderRepository::new(Arc::clone(&database))
        .register(&root)
        .unwrap();
    let ocr = Arc::new(FakeLocalOcr::default());
    let coordinator = IndexCoordinator::with_parser_ocr_and_sink(
        Arc::clone(&database),
        Arc::new(ScanAwareParser),
        Arc::clone(&ocr),
        10 * 1024 * 1024,
        None,
    );
    let settings = AppSettings {
        ocr_enabled: true,
        math_ocr_enabled: false,
        ..AppSettings::default()
    };
    coordinator.apply_runtime_settings(&settings).unwrap();

    let job = coordinator.start(&folder.id).await.unwrap();
    wait_finished(&coordinator, &job).await;

    assert_eq!(ocr.calls.load(Ordering::Acquire), 1);
    let connection = database.connection();
    let image_body: String = connection
        .query_row(
            "SELECT c.body FROM document_content c
             JOIN documents d ON d.id = c.document_id
             WHERE d.file_name = 'scan.png'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let pdf_body: String = connection
        .query_row(
            "SELECT c.body FROM document_content c
             JOIN documents d ON d.id = c.document_id
             WHERE d.file_name = 'normal.pdf'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(image_body.contains("스캔 이미지"));
    assert!(pdf_body.contains("embedded text layer"));
}
