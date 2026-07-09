mod api;
mod oauth;
mod store;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use cap_project::{RecordingMeta, YouTubeSharingMeta};
use clipboard_rs::{Clipboard, ClipboardContext};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, ipc::Channel};
use tracing::error;

pub use api::YouTubeChannel;
pub use store::{YouTubePrivacy, YouTubeStore};

use crate::{ArcLock, UploadProgress, notifications::NotificationType};

#[derive(Debug, Serialize, Type, thiserror::Error)]
#[serde(tag = "type", content = "message")]
pub enum YouTubeError {
    #[error("No YouTube OAuth client configured. Add your Client ID and secret first.")]
    MissingCredentials,
    #[error("YouTube is not connected.")]
    NotConnected,
    #[error("YouTube authorization was cancelled.")]
    OAuthCancelled,
    #[error("YouTube sign-in needs to be reconnected.")]
    NeedsReconnect,
    #[error("YouTube OAuth error: {0}")]
    Oauth(String),
    #[error("Failed to exchange YouTube token: {0}")]
    TokenExchange(String),
    #[error("YouTube daily upload quota reached. Try again tomorrow.")]
    QuotaExceeded,
    #[error("Rendered video file not found.")]
    FileNotFound,
    #[error("YouTube API error ({code}): {message}")]
    Api { code: u16, message: String },
    #[error("Network error: {0}")]
    Http(String),
    #[error("{0}")]
    Store(String),
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeStatus {
    pub connected: bool,
    pub has_credentials: bool,
    pub channel_id: Option<String>,
    pub channel_title: Option<String>,
    pub auto_upload: bool,
    pub default_privacy: YouTubePrivacy,
}

impl From<&YouTubeStore> for YouTubeStatus {
    fn from(store: &YouTubeStore) -> Self {
        Self {
            connected: store.is_connected(),
            has_credentials: store.effective_client_id().is_some()
                && store.effective_client_secret().is_some(),
            channel_id: store.channel_id.clone(),
            channel_title: store.channel_title.clone(),
            auto_upload: store.auto_upload,
            default_privacy: store.default_privacy.clone(),
        }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn load_status(app: &AppHandle) -> Result<YouTubeStatus, YouTubeError> {
    let store = YouTubeStore::get(app)
        .map_err(YouTubeError::Store)?
        .unwrap_or_default();
    Ok(YouTubeStatus::from(&store))
}

#[tauri::command]
#[specta::specta]
pub async fn youtube_get_status(app: AppHandle) -> Result<YouTubeStatus, YouTubeError> {
    load_status(&app)
}

#[tauri::command]
#[specta::specta]
pub async fn youtube_set_credentials(
    app: AppHandle,
    client_id: String,
    client_secret: String,
) -> Result<YouTubeStatus, YouTubeError> {
    let store = YouTubeStore::update(&app, |s| {
        s.client_id = Some(client_id.trim().to_string()).filter(|v| !v.is_empty());
        s.client_secret = Some(client_secret.trim().to_string()).filter(|v| !v.is_empty());
    })
    .map_err(YouTubeError::Store)?;
    Ok(YouTubeStatus::from(&store))
}

#[tauri::command]
#[specta::specta]
pub async fn youtube_connect(app: AppHandle) -> Result<YouTubeStatus, YouTubeError> {
    let store = oauth::connect(&app).await?;
    Ok(YouTubeStatus::from(&store))
}

#[tauri::command]
#[specta::specta]
pub async fn youtube_disconnect(app: AppHandle) -> Result<YouTubeStatus, YouTubeError> {
    oauth::disconnect(&app).await?;
    load_status(&app)
}

#[tauri::command]
#[specta::specta]
pub async fn youtube_list_channels(app: AppHandle) -> Result<Vec<YouTubeChannel>, YouTubeError> {
    api::list_channels(&app).await
}

#[tauri::command]
#[specta::specta]
pub async fn youtube_set_channel(
    app: AppHandle,
    channel_id: String,
    channel_title: String,
) -> Result<YouTubeStatus, YouTubeError> {
    let store = YouTubeStore::update(&app, |s| {
        s.channel_id = Some(channel_id);
        s.channel_title = Some(channel_title);
    })
    .map_err(YouTubeError::Store)?;
    Ok(YouTubeStatus::from(&store))
}

#[tauri::command]
#[specta::specta]
pub async fn youtube_set_preferences(
    app: AppHandle,
    auto_upload: bool,
    default_privacy: YouTubePrivacy,
) -> Result<YouTubeStatus, YouTubeError> {
    let store = YouTubeStore::update(&app, |s| {
        s.auto_upload = auto_upload;
        s.default_privacy = default_privacy;
    })
    .map_err(YouTubeError::Store)?;
    Ok(YouTubeStatus::from(&store))
}

/// Uploads the finished mp4 for a recording project to YouTube. The caller is expected to have
/// rendered the video first (studio recordings only have an output file after export); this mirrors
/// the cap.so `upload_exported_video` contract.
#[tauri::command]
#[specta::specta]
pub async fn youtube_upload_recording(
    app: AppHandle,
    project_path: PathBuf,
    progress: Channel<UploadProgress>,
) -> Result<YouTubeSharingMeta, YouTubeError> {
    let store = YouTubeStore::get(&app)
        .map_err(YouTubeError::Store)?
        .ok_or(YouTubeError::NotConnected)?;
    if !store.is_connected() {
        return Err(YouTubeError::NotConnected);
    }

    let mut meta = RecordingMeta::load_for_project(&project_path)
        .map_err(|e| YouTubeError::Store(e.to_string()))?;

    if let Some(existing) = meta.youtube.clone() {
        return Ok(existing);
    }

    let file_path = meta.output_path();
    if !file_path.exists() {
        return Err(YouTubeError::FileNotFound);
    }

    let privacy = store.default_privacy.clone();
    let result = api::upload_video(
        &app,
        &file_path,
        api::UploadRequest {
            title: meta.pretty_name.clone(),
            description: String::new(),
            privacy: privacy.clone(),
            channel_id: store.channel_id.clone(),
        },
        &progress,
    )
    .await;

    match result {
        Ok(video_id) => {
            let sharing = YouTubeSharingMeta {
                url: format!("https://youtu.be/{video_id}"),
                video_id,
                privacy: privacy.as_api_value().to_string(),
                uploaded_at: now_secs(),
            };
            meta.youtube = Some(sharing.clone());
            meta.save_for_project()
                .map_err(|e| error!("Failed to save recording meta: {e}"))
                .ok();

            let _ = app
                .state::<ArcLock<ClipboardContext>>()
                .write()
                .await
                .set_text(sharing.url.clone());

            NotificationType::YouTubeUploadComplete.send(&app);
            Ok(sharing)
        }
        Err(e) => {
            error!("YouTube upload failed: {e}");
            NotificationType::YouTubeUploadFailed.send(&app);
            Err(e)
        }
    }
}
