use std::sync::Arc;

use reqwest::Url;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;

use crate::domain::models::AppSettings;
use crate::infrastructure::database::Database;
use crate::library::repository::LibraryRepository;

const MAX_CONTEXT_CHARS: usize = 40_000;

pub struct AiService {
    database: Arc<Database>,
    client: reqwest::Client,
}

impl AiService {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            client: reqwest::Client::new(),
        }
    }

    pub async fn run(
        &self,
        document_id: &str,
        question: Option<&str>,
        settings: &AppSettings,
        remote_consent: bool,
    ) -> Result<String, AiError> {
        if !settings.ai_enabled {
            return Err(AiError::Disabled);
        }
        let preview = LibraryRepository::new(Arc::clone(&self.database))
            .get_preview(document_id)
            .map_err(|error| AiError::Document(error.to_string()))?;
        let context: String = preview.markdown.chars().take(MAX_CONTEXT_CHARS).collect();
        if context.trim().is_empty() {
            return Err(AiError::Document("document has no parsed text".into()));
        }
        let instruction = match question {
            Some(value) if !value.trim().is_empty() => {
                format!("문서에 근거해서만 답하세요. 질문: {}", value.trim())
            }
            _ => "문서의 핵심을 한국어로 간결하게 요약하세요.".into(),
        };
        let prompt = format!("{instruction}\n\n[문서]\n{context}");

        match settings.ai_provider.as_str() {
            "ollama" => self.ollama(settings, &prompt).await,
            "gemini" => {
                require_remote_consent(remote_consent)?;
                self.gemini(settings, &prompt).await
            }
            "openai" => {
                require_remote_consent(remote_consent)?;
                self.openai(settings, &prompt).await
            }
            _ => Err(AiError::Provider),
        }
    }

    async fn ollama(&self, settings: &AppSettings, prompt: &str) -> Result<String, AiError> {
        let mut url = validated_base_url(&settings.ai_base_url, true)?;
        url.set_path("/api/chat");
        let response = self
            .client
            .post(url)
            .json(&json!({
                "model": settings.ai_model,
                "stream": false,
                "messages": [{"role": "user", "content": prompt}],
                "options": {
                    "temperature": settings.ai_temperature,
                    "num_predict": settings.ai_max_tokens
                }
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<OllamaResponse>()
            .await?;
        nonempty(response.message.content)
    }

    async fn gemini(&self, settings: &AppSettings, prompt: &str) -> Result<String, AiError> {
        let key = self.secret("gemini")?;
        let mut url = validated_base_url(&settings.ai_base_url, false)?;
        url.set_path(&format!(
            "/v1beta/models/{}:generateContent",
            settings.ai_model
        ));
        url.query_pairs_mut().append_pair("key", &key);
        let response = self
            .client
            .post(url)
            .json(&json!({
                "contents": [{"parts": [{"text": prompt}]}],
                "generationConfig": {
                    "temperature": settings.ai_temperature,
                    "maxOutputTokens": settings.ai_max_tokens
                }
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<GeminiResponse>()
            .await?;
        let text = response
            .candidates
            .into_iter()
            .flat_map(|candidate| candidate.content.parts)
            .map(|part| part.text)
            .collect::<Vec<_>>()
            .join("\n");
        nonempty(text)
    }

    async fn openai(&self, settings: &AppSettings, prompt: &str) -> Result<String, AiError> {
        let key = self.secret("openai")?;
        let mut url = validated_base_url(&settings.ai_base_url, false)?;
        url.set_path("/v1/responses");
        let value = self
            .client
            .post(url)
            .bearer_auth(key)
            .json(&json!({
                "model": settings.ai_model,
                "input": prompt,
                "temperature": settings.ai_temperature,
                "max_output_tokens": settings.ai_max_tokens
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        if let Some(text) = value.get("output_text").and_then(|value| value.as_str()) {
            return nonempty(text.to_string());
        }
        let text = value
            .get("output")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .flat_map(|item| {
                item.get("content")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
            })
            .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        nonempty(text)
    }

    fn secret(&self, provider: &str) -> Result<String, AiError> {
        self.database
            .connection()
            .query_row(
                "SELECT secret FROM ai_secrets WHERE provider = ?1",
                params![provider],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(AiError::MissingSecret)
    }
}

fn require_remote_consent(consent: bool) -> Result<(), AiError> {
    consent.then_some(()).ok_or(AiError::ConsentRequired)
}

fn validated_base_url(value: &str, local_only: bool) -> Result<Url, AiError> {
    let url = Url::parse(value).map_err(|_| AiError::Endpoint)?;
    let host = url.host_str().unwrap_or_default();
    if local_only {
        if url.scheme() != "http" || !matches!(host, "127.0.0.1" | "localhost" | "::1") {
            return Err(AiError::Endpoint);
        }
    } else if url.scheme() != "https" {
        return Err(AiError::Endpoint);
    }
    Ok(url)
}

fn nonempty(text: String) -> Result<String, AiError> {
    if text.trim().is_empty() {
        Err(AiError::EmptyResponse)
    } else {
        Ok(text)
    }
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiPart {
    #[serde(default)]
    text: String,
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI features are disabled")]
    Disabled,
    #[error("selected AI provider is invalid")]
    Provider,
    #[error("AI endpoint is invalid or violates the provider policy")]
    Endpoint,
    #[error("explicit consent is required before sending document text")]
    ConsentRequired,
    #[error("the selected provider API key is not saved")]
    MissingSecret,
    #[error("document is unavailable: {0}")]
    Document(String),
    #[error("AI provider returned an empty response")]
    EmptyResponse,
    #[error("AI provider request failed")]
    Http(#[from] reqwest::Error),
    #[error("AI secret storage failed")]
    Database(#[from] rusqlite::Error),
}

#[cfg(test)]
mod tests {
    use super::{require_remote_consent, validated_base_url};

    #[test]
    fn ollama_accepts_only_loopback_http() {
        assert!(validated_base_url("http://127.0.0.1:11434", true).is_ok());
        assert!(validated_base_url("http://localhost:11434", true).is_ok());
        assert!(validated_base_url("http://192.168.0.2:11434", true).is_err());
        assert!(validated_base_url("https://example.com", true).is_err());
    }

    #[test]
    fn remote_providers_require_https_and_explicit_consent() {
        assert!(validated_base_url("https://api.openai.com", false).is_ok());
        assert!(validated_base_url("http://api.openai.com", false).is_err());
        assert!(require_remote_consent(false).is_err());
        assert!(require_remote_consent(true).is_ok());
    }
}
