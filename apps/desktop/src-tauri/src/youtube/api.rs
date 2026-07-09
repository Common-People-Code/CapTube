use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, ipc::Channel};
use tokio::io::AsyncReadExt;

use super::{YouTubeError, oauth, store::YouTubePrivacy};
use crate::UploadProgress;

const CHANNELS_ENDPOINT: &str = "https://www.googleapis.com/youtube/v3/channels";
const UPLOAD_ENDPOINT: &str =
    "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status";
const CHUNK_SIZE: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeChannel {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

#[derive(Deserialize)]
struct ChannelsResponse {
    #[serde(default)]
    items: Vec<ChannelItem>,
}

#[derive(Deserialize)]
struct ChannelItem {
    id: String,
    snippet: ChannelSnippet,
}

#[derive(Deserialize)]
struct ChannelSnippet {
    title: String,
    #[serde(default)]
    thumbnails: Option<Thumbnails>,
}

#[derive(Deserialize)]
struct Thumbnails {
    #[serde(default)]
    default: Option<Thumbnail>,
}

#[derive(Deserialize)]
struct Thumbnail {
    url: String,
}

fn short_client() -> Result<reqwest::Client, YouTubeError> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| YouTubeError::Http(e.to_string()))
}

/// A client without a global request timeout — a resumable chunk PUT of a large video would
/// otherwise trip the default 30s cap.
fn upload_client() -> Result<reqwest::Client, YouTubeError> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| YouTubeError::Http(e.to_string()))
}

pub async fn list_channels(app: &AppHandle) -> Result<Vec<YouTubeChannel>, YouTubeError> {
    let token = oauth::ensure_access_token(app).await?;

    let response = short_client()?
        .get(CHANNELS_ENDPOINT)
        .query(&[("part", "snippet"), ("mine", "true")])
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| YouTubeError::Http(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(map_api_error(status.as_u16(), body));
    }

    let parsed: ChannelsResponse = response
        .json()
        .await
        .map_err(|e| YouTubeError::Http(e.to_string()))?;

    Ok(parsed
        .items
        .into_iter()
        .map(|item| YouTubeChannel {
            id: item.id,
            title: item.snippet.title,
            thumbnail_url: item
                .snippet
                .thumbnails
                .and_then(|t| t.default)
                .map(|t| t.url),
        })
        .collect())
}

pub struct UploadRequest {
    pub title: String,
    pub description: String,
    pub privacy: YouTubePrivacy,
    pub channel_id: Option<String>,
}

/// Resumable upload of a finished mp4 to YouTube. Returns the new video id.
pub async fn upload_video(
    app: &AppHandle,
    file_path: &Path,
    request: UploadRequest,
    progress: &Channel<UploadProgress>,
) -> Result<String, YouTubeError> {
    if !file_path.exists() {
        return Err(YouTubeError::FileNotFound);
    }

    let token = oauth::ensure_access_token(app).await?;

    let mut file = tokio::fs::File::open(file_path)
        .await
        .map_err(|e| YouTubeError::Http(e.to_string()))?;
    let total = file
        .metadata()
        .await
        .map_err(|e| YouTubeError::Http(e.to_string()))?
        .len();

    let mut snippet = serde_json::json!({
        "title": request.title,
        "description": request.description,
    });
    if let Some(channel_id) = request.channel_id.as_ref() {
        snippet["channelId"] = serde_json::Value::String(channel_id.clone());
    }
    let metadata = serde_json::json!({
        "snippet": snippet,
        "status": {
            "privacyStatus": request.privacy.as_api_value(),
            "selfDeclaredMadeForKids": false,
        },
    });

    let init = short_client()?
        .post(UPLOAD_ENDPOINT)
        .bearer_auth(&token)
        .header("X-Upload-Content-Length", total.to_string())
        .header("X-Upload-Content-Type", "video/*")
        .json(&metadata)
        .send()
        .await
        .map_err(|e| YouTubeError::Http(e.to_string()))?;

    let status = init.status();
    if !status.is_success() {
        let body = init.text().await.unwrap_or_default();
        return Err(map_api_error(status.as_u16(), body));
    }

    let session_url = init
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| YouTubeError::Api {
            code: 0,
            message: "Missing resumable upload session URL".to_string(),
        })?;

    progress.send(UploadProgress { progress: 0.0 }).ok();

    let client = upload_client()?;
    let mut offset: u64 = 0;
    let mut buffer = vec![0u8; CHUNK_SIZE];

    loop {
        let mut filled = 0usize;
        while filled < buffer.len() {
            let read = file
                .read(&mut buffer[filled..])
                .await
                .map_err(|e| YouTubeError::Http(e.to_string()))?;
            if read == 0 {
                break;
            }
            filled += read;
        }

        if filled == 0 {
            break;
        }

        let chunk_start = offset;
        let chunk_end = offset + filled as u64 - 1;
        let content_range = format!("bytes {chunk_start}-{chunk_end}/{total}");

        let response = client
            .put(&session_url)
            .header(reqwest::header::CONTENT_LENGTH, filled.to_string())
            .header(reqwest::header::CONTENT_RANGE, content_range)
            .body(buffer[..filled].to_vec())
            .send()
            .await
            .map_err(|e| YouTubeError::Http(e.to_string()))?;

        offset += filled as u64;
        let pct = if total > 0 {
            (offset as f64 / total as f64).min(1.0)
        } else {
            0.0
        };
        progress.send(UploadProgress { progress: pct }).ok();

        let status = response.status();
        if status.as_u16() == 308 {
            continue;
        }
        if status.is_success() {
            let body: VideoResource = response
                .json()
                .await
                .map_err(|e| YouTubeError::Http(e.to_string()))?;
            return Ok(body.id);
        }

        let body = response.text().await.unwrap_or_default();
        return Err(map_api_error(status.as_u16(), body));
    }

    Err(YouTubeError::Api {
        code: 0,
        message: "Upload ended before YouTube confirmed the video".to_string(),
    })
}

#[derive(Deserialize)]
struct VideoResource {
    id: String,
}

fn map_api_error(code: u16, body: String) -> YouTubeError {
    if code == 401 {
        return YouTubeError::NeedsReconnect;
    }
    if code == 403 && body.contains("quotaExceeded") {
        return YouTubeError::QuotaExceeded;
    }
    YouTubeError::Api {
        code,
        message: body,
    }
}
