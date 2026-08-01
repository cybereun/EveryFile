use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    Keyword,
    Filename,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TermMode {
    All,
    Any,
    Exact,
    Exclude,
    Near,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchMatchKind {
    Filename,
    Content,
    Both,
    Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub request_id: String,
    pub query: String,
    pub mode: SearchMode,
    pub folder_ids: Vec<String>,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub extensionless: bool,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub include_filename: bool,
    pub term_mode: TermMode,
    pub private_search: bool,
    pub sort: String,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub document_id: String,
    pub file_name: String,
    pub path: String,
    pub extension: String,
    pub size_bytes: u64,
    pub modified_at: String,
    pub snippet: Option<String>,
    pub score: f64,
    pub match_kind: SearchMatchKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRecord {
    pub id: String,
    pub canonical_path: String,
    pub display_name: String,
    pub document_count: u64,
    pub index_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRecord {
    pub id: String,
    pub folder_id: String,
    pub canonical_path: String,
    pub file_name: String,
    pub extension: String,
    pub size_bytes: u64,
    pub modified_at: String,
    pub content_hash: Option<String>,
    pub parser_kind: Option<String>,
    pub parse_state: String,
    pub parse_error_code: Option<String>,
    pub indexed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub request_id: String,
    pub hits: Vec<SearchHit>,
    pub total: u64,
    pub elapsed_ms: u64,
    pub applied_filters: Vec<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBlock {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    pub level: Option<u8>,
    pub page_number: Option<u32>,
    pub href: Option<String>,
    pub list_type: Option<String>,
    pub children: Vec<PreviewBlock>,
    pub table: Option<PreviewTable>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTable {
    pub rows: u32,
    pub cols: u32,
    pub has_header: bool,
    pub cells: Vec<Vec<PreviewCell>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCell {
    pub text: String,
    pub col_span: u32,
    pub row_span: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewWarning {
    pub code: String,
    pub message: String,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TagRecord {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkRecord {
    pub document_id: String,
    pub note: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDocument {
    pub document_id: String,
    pub file_name: String,
    pub path: String,
    pub extension: String,
    pub markdown: String,
    pub blocks: Vec<PreviewBlock>,
    pub warnings: Vec<PreviewWarning>,
    pub bookmarked: bool,
    pub bookmark_note: String,
    pub tags: Vec<TagRecord>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub job_id: String,
    pub state: JobState,
    pub total_files: u64,
    pub completed_files: u64,
    pub current_path: Option<String>,
    pub error_count: u64,
    pub errors: Vec<IndexFailure>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Queued,
    Discovering,
    Parsing,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexFailure {
    pub code: String,
    pub file_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppSettings {
    pub language: String,
    pub theme: String,
    pub file_click_behavior: String,
    pub date_display: String,
    pub excluded_path_patterns: Vec<String>,
    pub indexing_intensity: String,
    pub ocr_enabled: bool,
    pub math_ocr_enabled: bool,
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_base_url: String,
    pub ai_model: String,
    pub ai_temperature: f32,
    pub ai_max_tokens: u32,
    pub history_retention_days: u32,
    pub minimize_to_tray: bool,
    pub start_with_windows: bool,
    pub start_hidden: bool,
    pub max_file_size_bytes: u64,
    pub result_page_size: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "ko".into(),
            theme: "light".into(),
            file_click_behavior: "preview".into(),
            date_display: "relative".into(),
            excluded_path_patterns: Vec::new(),
            indexing_intensity: "balanced".into(),
            ocr_enabled: false,
            math_ocr_enabled: false,
            ai_enabled: false,
            ai_provider: "ollama".into(),
            ai_base_url: "http://127.0.0.1:11434".into(),
            ai_model: "gemma3:4b".into(),
            ai_temperature: 0.2,
            ai_max_tokens: 2048,
            history_retention_days: 90,
            minimize_to_tray: false,
            start_with_windows: false,
            start_hidden: false,
            max_file_size_bytes: 200 * 1024 * 1024,
            result_page_size: 100,
        }
    }
}
