use everyfile_lib::ai::{gemini, ollama, openai, retrieval};

#[test]
fn ollama_ndjson_stream_contract_accumulates_message_content() {
    let body = concat!(
        "{\"message\":{\"content\":\"첫째\"},\"done\":false}\n",
        "{\"message\":{\"content\":\" 둘째\"},\"done\":true}\n"
    );
    assert_eq!(ollama::parse_stream(body).unwrap(), "첫째 둘째");
}

#[test]
fn gemini_sse_stream_contract_accumulates_candidate_parts() {
    let body = concat!(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"요약\"}]}}]}\n\n",
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" 완료\"}]}}]}\n\n"
    );
    assert_eq!(gemini::parse_stream(body).unwrap(), "요약 완료");
}

#[test]
fn openai_responses_sse_contract_accepts_only_output_text_deltas() {
    let body = concat!(
        "data: {\"type\":\"response.created\"}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"답\"}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"변\"}\n\n",
        "data: [DONE]\n\n"
    );
    assert_eq!(openai::parse_stream(body).unwrap(), "답변");
}

#[test]
fn retrieval_is_bounded_and_includes_source_labels() {
    let source = format!(
        "{}\n\n매출 증가율은 42%입니다.\n\n{}",
        "앞 문단 ".repeat(3_000),
        "뒤 문단 ".repeat(3_000)
    );
    let result = retrieval::build_cited_prompt(&source, Some("매출 증가율"));
    assert!(result.prompt.contains("42%"));
    assert!(result.prompt.contains("[문서 근거"));
    assert!(result.prompt.chars().count() < 30_000);
}
