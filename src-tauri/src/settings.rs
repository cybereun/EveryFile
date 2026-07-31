use std::sync::Arc;

use rusqlite::params;
use thiserror::Error;

pub use crate::domain::models::AppSettings;
use crate::infrastructure::database::Database;

#[derive(Clone)]
pub struct SettingsRepository {
    database: Arc<Database>,
}

impl SettingsRepository {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub fn load(&self) -> Result<AppSettings, SettingsError> {
        Ok(self.load_with_migration()?.settings)
    }

    pub fn load_with_migration(&self) -> Result<LoadedSettings, SettingsError> {
        let connection = self.database.connection();
        let stored = connection
            .query_row(
                "SELECT settings_json FROM app_settings WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        drop(connection);
        match stored {
            Some(json) => {
                let mut settings = serde_json::from_str::<AppSettings>(&json)
                    .map_err(SettingsError::Deserialize)?;
                let normalized_unsupported_flags = settings.minimize_to_tray
                    || settings.start_with_windows
                    || settings.start_hidden;
                settings.minimize_to_tray = false;
                settings.start_with_windows = false;
                settings.start_hidden = false;
                validate(&settings)?;
                if normalized_unsupported_flags {
                    self.persist(&settings)?;
                }
                Ok(LoadedSettings {
                    settings,
                    normalized_unsupported_flags,
                })
            }
            None => Ok(LoadedSettings {
                settings: AppSettings::default(),
                normalized_unsupported_flags: false,
            }),
        }
    }

    pub fn save(&self, settings: &AppSettings) -> Result<AppSettings, SettingsError> {
        validate(settings)?;
        self.persist(settings)?;
        Ok(settings.clone())
    }

    fn persist(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        let json = serde_json::to_string(settings).map_err(SettingsError::Serialize)?;
        self.database.connection().execute(
            "INSERT INTO app_settings (id, settings_json, updated_at)
             VALUES (1, ?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(id) DO UPDATE SET
               settings_json = excluded.settings_json,
               updated_at = excluded.updated_at",
            params![json],
        )?;
        Ok(())
    }
}

pub struct LoadedSettings {
    pub settings: AppSettings,
    pub normalized_unsupported_flags: bool,
}

fn validate(settings: &AppSettings) -> Result<(), SettingsError> {
    if settings.minimize_to_tray || settings.start_with_windows || settings.start_hidden {
        return Err(SettingsError::Invalid(
            "startup and tray settings are unavailable in this version".into(),
        ));
    }
    if !matches!(settings.language.as_str(), "ko" | "en") {
        return Err(SettingsError::Invalid("language must be ko or en".into()));
    }
    if !matches!(settings.theme.as_str(), "light" | "dark" | "system") {
        return Err(SettingsError::Invalid(
            "theme must be light, dark, or system".into(),
        ));
    }
    if !matches!(settings.file_click_behavior.as_str(), "preview" | "open") {
        return Err(SettingsError::Invalid(
            "file click behavior must be preview or open".into(),
        ));
    }
    if !matches!(settings.date_display.as_str(), "relative" | "absolute") {
        return Err(SettingsError::Invalid(
            "date display must be relative or absolute".into(),
        ));
    }
    if !matches!(
        settings.indexing_intensity.as_str(),
        "low" | "balanced" | "high"
    ) {
        return Err(SettingsError::Invalid(
            "indexing intensity must be low, balanced, or high".into(),
        ));
    }
    if settings.math_ocr_enabled && !settings.ocr_enabled {
        return Err(SettingsError::Invalid(
            "math OCR requires local OCR to be enabled".into(),
        ));
    }
    if !matches!(
        settings.ai_provider.as_str(),
        "ollama" | "gemini" | "openai"
    ) {
        return Err(SettingsError::Invalid(
            "AI provider must be ollama, gemini, or openai".into(),
        ));
    }
    if !settings.ai_temperature.is_finite() || !(0.0..=2.0).contains(&settings.ai_temperature) {
        return Err(SettingsError::Invalid(
            "AI temperature must be between 0 and 2".into(),
        ));
    }
    if !(128..=32768).contains(&settings.ai_max_tokens) {
        return Err(SettingsError::Invalid(
            "AI max tokens must be between 128 and 32768".into(),
        ));
    }
    if settings.ai_enabled && (settings.ai_model.trim().is_empty() || settings.ai_model.len() > 200)
    {
        return Err(SettingsError::Invalid(
            "AI model must contain 1-200 characters".into(),
        ));
    }
    if settings.excluded_path_patterns.len() > 100
        || settings.excluded_path_patterns.iter().any(|pattern| {
            pattern.trim().is_empty()
                || pattern.len() > 260
                || pattern.chars().any(char::is_control)
        })
    {
        return Err(SettingsError::Invalid(
            "excluded path patterns must contain 1-100 non-control characters each".into(),
        ));
    }
    if !matches!(settings.history_retention_days, 0 | 30 | 90 | 365) {
        return Err(SettingsError::Invalid(
            "history retention must be 30, 90, 365, or 0 for unlimited".into(),
        ));
    }
    if settings.max_file_size_bytes == 0 {
        return Err(SettingsError::Invalid(
            "maximum file size must be greater than zero".into(),
        ));
    }
    if !(1..=200).contains(&settings.result_page_size) {
        return Err(SettingsError::Invalid(
            "result page size must be between 1 and 200".into(),
        ));
    }
    Ok(())
}

use rusqlite::OptionalExtension;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("settings database operation failed")]
    Database(#[from] rusqlite::Error),
    #[error("settings serialization failed")]
    Serialize(#[source] serde_json::Error),
    #[error("stored settings are invalid")]
    Deserialize(#[source] serde_json::Error),
    #[error("invalid settings: {0}")]
    Invalid(String),
}

impl SettingsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "SETTINGS_DATABASE_FAILED",
            Self::Serialize(_) => "SETTINGS_SERIALIZE_FAILED",
            Self::Deserialize(_) => "SETTINGS_STORED_INVALID",
            Self::Invalid(_) => "SETTINGS_INVALID",
        }
    }
}
