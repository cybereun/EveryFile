use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    Keyword,
    Filename,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub mode: SearchMode,
    pub folder_ids: Vec<String>,
    pub extensions: Vec<String>,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub include_filename: bool,
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
    pub hits: Vec<SearchHit>,
    pub total: u64,
    pub elapsed_ms: u64,
    pub applied_filters: Vec<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBlock {
    pub kind: String,
    pub text: String,
    pub level: Option<u8>,
    pub page_number: Option<u32>,
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
    pub warnings: Vec<String>,
    pub bookmarked: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub job_id: String,
    pub state: String,
    pub total_files: u64,
    pub completed_files: u64,
    pub current_file_name: Option<String>,
    pub error_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub language: String,
    pub theme: String,
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
            history_retention_days: 90,
            minimize_to_tray: false,
            start_with_windows: false,
            start_hidden: false,
            max_file_size_bytes: 200 * 1024 * 1024,
            result_page_size: 100,
        }
    }
}
