use everyfile_lib::domain::models::{SearchMode, SearchRequest, TermMode};
use everyfile_lib::settings::AppSettings;

#[test]
fn search_request_serializes_with_camel_case_keys() {
    let request = SearchRequest {
        request_id: "search-contract".into(),
        query: "검색어".into(),
        mode: SearchMode::Keyword,
        folder_ids: vec!["folder-1".into()],
        extensions: vec!["hwp".into(), "pdf".into()],
        modified_after: None,
        modified_before: None,
        include_filename: true,
        term_mode: TermMode::All,
        private_search: false,
        sort: "relevance".into(),
        limit: 100,
        offset: 0,
    };

    let value = serde_json::to_value(request).unwrap();

    assert_eq!(value["mode"], "keyword");
    assert_eq!(value["requestId"], "search-contract");
    assert_eq!(value["folderIds"][0], "folder-1");
    assert_eq!(value["includeFilename"], true);
    assert_eq!(value["termMode"], "all");
}

#[test]
fn default_settings_use_everyfile_defaults() {
    assert_eq!(AppSettings::default().language, "ko");
    assert_eq!(AppSettings::default().theme, "light");
    assert_eq!(AppSettings::default().history_retention_days, 90);
    assert!(!AppSettings::default().minimize_to_tray);
    assert!(!AppSettings::default().start_with_windows);
}
