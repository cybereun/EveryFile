use reqwest::{Client, Url};
use serde_json::{json, Value};
use tokio::sync::watch;

use super::{await_response_text, nonempty, AiError, GenerationRequest};

pub(crate) async fn chat(
    client: &Client,
    mut url: Url,
    generation: GenerationRequest<'_>,
    cancellation: watch::Receiver<bool>,
) -> Result<String, AiError> {
    url.set_path("/api/chat");
    let request = client.post(url).json(&json!({
            "model": generation.model,
        "stream": true,
            "messages": [{"role": "user", "content": generation.prompt}],
            "options": {
                "temperature": generation.temperature,
                "num_predict": generation.max_tokens
            }
    }));
    let body = await_response_text(request, cancellation).await?;
    parse_stream(&body)
}

pub fn parse_stream(body: &str) -> Result<String, AiError> {
    let text = body
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|value| {
            value
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<String>();
    nonempty(text)
}
