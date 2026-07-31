pub mod gemini;
pub mod ollama;
pub mod openai;
pub mod retrieval;

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use reqwest::{RequestBuilder, Url};
use rusqlite::params;
use thiserror::Error;
use tokio::sync::watch;

use crate::domain::models::AppSettings;
use crate::infrastructure::database::Database;
use crate::library::repository::LibraryRepository;

#[derive(Default)]
pub struct AiRequestRegistry {
    active: Mutex<HashMap<String, watch::Sender<bool>>>,
}

impl AiRequestRegistry {
    fn register(&self, request_id: &str) -> Result<watch::Receiver<bool>, AiError> {
        let mut active = self.active.lock();
        if active.contains_key(request_id) {
            return Err(AiError::DuplicateRequest);
        }
        let (sender, receiver) = watch::channel(false);
        active.insert(request_id.to_string(), sender);
        Ok(receiver)
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        self.active
            .lock()
            .get(request_id)
            .is_some_and(|sender| sender.send(true).is_ok())
    }

    pub fn cancel_all(&self) {
        for sender in self.active.lock().values() {
            let _ = sender.send(true);
        }
    }

    fn finish(&self, request_id: &str) {
        self.active.lock().remove(request_id);
    }
}

pub struct AiService<'a> {
    database: Arc<Database>,
    requests: &'a AiRequestRegistry,
    client: reqwest::Client,
}

impl<'a> AiService<'a> {
    pub fn new(database: Arc<Database>, requests: &'a AiRequestRegistry) -> Self {
        Self {
            database,
            requests,
            client: reqwest::Client::new(),
        }
    }

    pub async fn run(
        &self,
        request_id: &str,
        document_id: &str,
        question: Option<&str>,
        settings: &AppSettings,
        remote_consent: bool,
    ) -> Result<String, AiError> {
        validate_request_id(request_id)?;
        if !settings.ai_enabled {
            return Err(AiError::Disabled);
        }
        let provider = settings.ai_provider.as_str();
        let remote = matches!(provider, "gemini" | "openai");
        if remote {
            require_remote_consent(remote_consent)?;
        }
        let endpoint = validated_provider_url(provider, &settings.ai_base_url)?;
        let cancellation = self.requests.register(request_id)?;
        let operation = if question.is_some_and(|value| !value.trim().is_empty()) {
            "question"
        } else {
            "summary"
        };
        self.begin_request(
            request_id,
            document_id,
            provider,
            operation,
            remote && remote_consent,
        )?;

        let result = self
            .run_registered(document_id, question, settings, endpoint, cancellation)
            .await;
        self.requests.finish(request_id);
        match &result {
            Ok(_) => self.finish_request(request_id, "completed", None)?,
            Err(AiError::Cancelled) => {
                self.finish_request(request_id, "cancelled", Some("AI_CANCELLED"))?
            }
            Err(error) => self.finish_request(request_id, "failed", Some(error.code()))?,
        }
        result
    }

    async fn run_registered(
        &self,
        document_id: &str,
        question: Option<&str>,
        settings: &AppSettings,
        endpoint: Url,
        cancellation: watch::Receiver<bool>,
    ) -> Result<String, AiError> {
        let preview = LibraryRepository::new(Arc::clone(&self.database))
            .get_preview(document_id)
            .map_err(|error| AiError::Document(error.to_string()))?;
        if preview.markdown.trim().is_empty() {
            return Err(AiError::Document("document has no parsed text".into()));
        }
        let retrieved = retrieval::build_cited_prompt(&preview.markdown, question);
        if retrieved.chunk_count == 0 {
            return Err(AiError::Document("document has no retrievable text".into()));
        }
        match settings.ai_provider.as_str() {
            "ollama" => {
                ollama::chat(
                    &self.client,
                    endpoint,
                    &settings.ai_model,
                    &retrieved.prompt,
                    settings.ai_temperature,
                    settings.ai_max_tokens,
                    cancellation,
                )
                .await
            }
            "gemini" => {
                gemini::chat(
                    &self.client,
                    endpoint,
                    &self.secret("gemini")?,
                    &settings.ai_model,
                    &retrieved.prompt,
                    settings.ai_temperature,
                    settings.ai_max_tokens,
                    cancellation,
                )
                .await
            }
            "openai" => {
                openai::chat(
                    &self.client,
                    endpoint,
                    &self.secret("openai")?,
                    &settings.ai_model,
                    &retrieved.prompt,
                    settings.ai_temperature,
                    settings.ai_max_tokens,
                    cancellation,
                )
                .await
            }
            _ => Err(AiError::Provider),
        }
    }

    fn secret(&self, provider: &str) -> Result<String, AiError> {
        crate::ai::secrets::read(&self.database, provider)
    }

    fn begin_request(
        &self,
        request_id: &str,
        document_id: &str,
        provider: &str,
        operation: &str,
        remote_consent: bool,
    ) -> Result<(), AiError> {
        self.database.connection().execute(
            "INSERT INTO ai_requests (
               request_id, document_id, provider, operation, state,
               remote_consent, error_code, created_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, 'running', ?5, NULL,
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), NULL)",
            params![request_id, document_id, provider, operation, remote_consent],
        )?;
        Ok(())
    }

    fn finish_request(
        &self,
        request_id: &str,
        state: &str,
        error_code: Option<&str>,
    ) -> Result<(), AiError> {
        self.database.connection().execute(
            "UPDATE ai_requests
             SET state = ?2, error_code = ?3,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE request_id = ?1 AND state = 'running'",
            params![request_id, state, error_code],
        )?;
        Ok(())
    }
}

pub mod secrets {
    use std::sync::Arc;

    use rusqlite::{params, OptionalExtension};
    use zeroize::Zeroizing;

    use crate::infrastructure::database::Database;

    use super::AiError;

    pub fn read(database: &Arc<Database>, provider: &str) -> Result<String, AiError> {
        let secret = database
            .connection()
            .query_row(
                "SELECT secret FROM ai_secrets WHERE provider = ?1",
                params![provider],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(AiError::MissingSecret)?;
        Ok(Zeroizing::new(secret).to_string())
    }
}

async fn await_response_text(
    request: RequestBuilder,
    mut cancellation: watch::Receiver<bool>,
) -> Result<String, AiError> {
    if *cancellation.borrow() {
        return Err(AiError::Cancelled);
    }
    let response = tokio::select! {
        changed = cancellation.changed() => {
            let _ = changed;
            return Err(AiError::Cancelled);
        }
        response = request.send() => response?.error_for_status()?,
    };
    tokio::select! {
        changed = cancellation.changed() => {
            let _ = changed;
            Err(AiError::Cancelled)
        }
        body = response.text() => Ok(body?),
    }
}

fn validate_request_id(value: &str) -> Result<(), AiError> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        Err(AiError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn require_remote_consent(consent: bool) -> Result<(), AiError> {
    consent.then_some(()).ok_or(AiError::ConsentRequired)
}

fn validated_provider_url(provider: &str, value: &str) -> Result<Url, AiError> {
    let url = Url::parse(value).map_err(|_| AiError::Endpoint)?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let valid = match provider {
        "ollama" => {
            matches!(url.scheme(), "http" | "https")
                && matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1")
        }
        "gemini" => url.scheme() == "https" && host == "generativelanguage.googleapis.com",
        "openai" => url.scheme() == "https" && host == "api.openai.com",
        _ => return Err(AiError::Provider),
    };
    valid.then_some(url).ok_or(AiError::Endpoint)
}

fn nonempty(text: String) -> Result<String, AiError> {
    if text.trim().is_empty() {
        Err(AiError::EmptyResponse)
    } else {
        Ok(text)
    }
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
    #[error("AI request was cancelled")]
    Cancelled,
    #[error("AI request id is invalid")]
    InvalidRequest,
    #[error("AI request id is already active")]
    DuplicateRequest,
    #[error("AI provider request failed")]
    Http(#[from] reqwest::Error),
    #[error("AI secret or request storage failed")]
    Database(#[from] rusqlite::Error),
}

impl AiError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "AI_DISABLED",
            Self::Provider => "AI_PROVIDER_INVALID",
            Self::Endpoint => "AI_ENDPOINT_INVALID",
            Self::ConsentRequired => "AI_CONSENT_REQUIRED",
            Self::MissingSecret => "AI_SECRET_MISSING",
            Self::Document(_) => "AI_DOCUMENT_UNAVAILABLE",
            Self::EmptyResponse => "AI_EMPTY_RESPONSE",
            Self::Cancelled => "AI_CANCELLED",
            Self::InvalidRequest => "AI_REQUEST_INVALID",
            Self::DuplicateRequest => "AI_REQUEST_DUPLICATE",
            Self::Http(_) => "AI_PROVIDER_FAILED",
            Self::Database(_) => "AI_STORAGE_FAILED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{gemini, ollama, openai, require_remote_consent, validated_provider_url};

    #[test]
    fn provider_endpoints_are_pinned_to_the_selected_destination() {
        assert!(validated_provider_url("ollama", "http://127.0.0.1:11434").is_ok());
        assert!(validated_provider_url("ollama", "http://192.168.0.2:11434").is_err());
        assert!(
            validated_provider_url("gemini", "https://generativelanguage.googleapis.com").is_ok()
        );
        assert!(validated_provider_url("openai", "https://api.openai.com").is_ok());
        assert!(validated_provider_url("openai", "https://example.com").is_err());
        assert!(require_remote_consent(false).is_err());
    }

    #[test]
    fn provider_stream_parsers_accumulate_deltas() {
        assert_eq!(
            ollama::parse_stream(
                "{\"message\":{\"content\":\"가\"}}\n{\"message\":{\"content\":\"나\"}}"
            )
            .unwrap(),
            "가나"
        );
        assert_eq!(
            gemini::parse_stream(
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"답\"}]}}]}\n"
            )
            .unwrap(),
            "답"
        );
        assert_eq!(
            openai::parse_stream(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"결과\"}\n"
            )
            .unwrap(),
            "결과"
        );
    }
}
