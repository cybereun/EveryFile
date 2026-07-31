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
    url.set_path("/v1/responses");
    let request = client.post(url).bearer_auth(api_key).json(&json!({
            "model": generation.model,
            "input": generation.prompt,
        "stream": true,
            "temperature": generation.temperature,
            "max_output_tokens": generation.max_tokens
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
        .filter(|value| {
            value.get("type").and_then(Value::as_str) == Some("response.output_text.delta")
        })
        .filter_map(|value| {
            value
                .get("delta")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<String>();
    nonempty(text)
}
