use reqwest::{Client, Url};
use serde_json::{json, Value};
use tokio::sync::watch;

use super::{await_response_text, nonempty, AiError, GenerationRequest};

pub(crate) async fn chat(
    client: &Client,
    mut url: Url,
    api_key: &str,
    generation: GenerationRequest<'_>,
    cancellation: watch::Receiver<bool>,
) -> Result<String, AiError> {
    url.set_path(&format!(
        "/v1beta/models/{}:streamGenerateContent",
        generation.model
    ));
    url.query_pairs_mut().append_pair("alt", "sse");
    let request = client
        .post(url)
        .header("x-goog-api-key", api_key)
        .json(&json!({
            "contents": [{"parts": [{"text": generation.prompt}]}],
            "generationConfig": {
                "temperature": generation.temperature,
                "maxOutputTokens": generation.max_tokens
            }
        }));
    let body = await_response_text(request, cancellation).await?;
    parse_stream(&body)
}

pub fn parse_stream(body: &str) -> Result<String, AiError> {
    let text = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "[DONE]")
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .flat_map(|value| {
            value
                .get("candidates")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .flat_map(|candidate| {
            candidate
                .get("content")
                .and_then(|content| content.get("parts"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|part| part.get("text").and_then(Value::as_str).map(str::to_owned))
        .collect::<String>();
    nonempty(text)
}
