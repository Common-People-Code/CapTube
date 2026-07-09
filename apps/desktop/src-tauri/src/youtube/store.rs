use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use tauri::{AppHandle, Wry};
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "lowercase")]
pub enum YouTubePrivacy {
    #[default]
    Unlisted,
    Private,
    Public,
}

impl YouTubePrivacy {
    pub fn as_api_value(&self) -> &'static str {
        match self {
            YouTubePrivacy::Unlisted => "unlisted",
            YouTubePrivacy::Private => "private",
            YouTubePrivacy::Public => "public",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeStore {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default)]
    #[specta(type = Option<f64>)]
    pub access_token_expires_at: Option<i64>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub channel_title: Option<String>,
    #[serde(default)]
    pub auto_upload: bool,
    #[serde(default)]
    pub default_privacy: YouTubePrivacy,
}

impl YouTubeStore {
    pub fn get(app: &AppHandle<Wry>) -> Result<Option<Self>, String> {
        match app.store("store").map(|s| s.get("youtube")) {
            Ok(Some(value)) => match serde_json::from_value(value) {
                Ok(store) => Ok(Some(store)),
                Err(e) => Err(format!("Failed to deserialize youtube store: {e}")),
            },
            _ => Ok(None),
        }
    }

    pub fn update(app: &AppHandle, update: impl FnOnce(&mut Self)) -> Result<Self, String> {
        let Ok(store) = app.store("store") else {
            return Err("Store not found".to_string());
        };

        let mut settings = Self::get(app)?.unwrap_or_default();
        update(&mut settings);
        store.set("youtube", json!(settings));
        store.save().map_err(|e| e.to_string())?;

        Ok(settings)
    }

    /// Resolve the OAuth client id, preferring the user-supplied value and falling back to an
    /// optional build-time default so a distributor can prefill their own project without shipping
    /// one by default. BYOK stays the default: no env set → no credentials in the binary.
    pub fn effective_client_id(&self) -> Option<String> {
        self.client_id
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| option_env!("CAP_YOUTUBE_CLIENT_ID").map(str::to_string))
    }

    pub fn effective_client_secret(&self) -> Option<String> {
        self.client_secret
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| option_env!("CAP_YOUTUBE_CLIENT_SECRET").map(str::to_string))
    }

    pub fn is_connected(&self) -> bool {
        self.refresh_token
            .as_ref()
            .is_some_and(|t| !t.trim().is_empty())
    }
}
