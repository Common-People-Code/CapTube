use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tracing::warn;

use super::{YouTubeError, store::YouTubeStore};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
const BASE_SCOPES: &str = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";
// Adds delete capability (videos.delete). Only requested when the user opts into deleting old
// versions, so upload-only users never grant "manage/delete your videos".
const DELETE_SCOPE: &str = "https://www.googleapis.com/auth/youtube.force-ssl";

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn random_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

fn http() -> Result<reqwest::Client, YouTubeError> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| YouTubeError::Http(e.to_string()))
}

/// Runs the full authorization-code + PKCE loopback flow against the user's own Google OAuth
/// client. The Google "Desktop app" client type allows a `http://127.0.0.1:{port}` redirect on any
/// port with no pre-registration, which is exactly what `tauri-plugin-oauth`'s loopback server
/// provides.
pub async fn connect(app: &AppHandle) -> Result<YouTubeStore, YouTubeError> {
    let store = YouTubeStore::get(app)
        .map_err(YouTubeError::Store)?
        .unwrap_or_default();
    let client_id = store
        .effective_client_id()
        .ok_or(YouTubeError::MissingCredentials)?;
    let client_secret = store
        .effective_client_secret()
        .ok_or(YouTubeError::MissingCredentials)?;

    let want_delete = store.delete_old_on_reupload;
    let scope = if want_delete {
        format!("{BASE_SCOPES} {DELETE_SCOPE}")
    } else {
        BASE_SCOPES.to_string()
    };

    let verifier = random_token(32);
    let challenge = code_challenge(&verifier);
    let state = random_token(16);

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let mut tx = Some(tx);
    let config = tauri_plugin_oauth::OauthConfig {
        ports: None,
        response: Some(
            "<html><body style=\"font-family:system-ui;text-align:center;padding-top:4rem\">\
             <h2>Connected to YouTube</h2><p>You can close this tab and return to Cap.</p>\
             </body></html>"
                .into(),
        ),
    };

    let port = tauri_plugin_oauth::start_with_config(config, move |url| {
        if let Some(tx) = tx.take() {
            let _ = tx.send(url);
        }
    })
    .map_err(|e| YouTubeError::Oauth(e.to_string()))?;

    let redirect_uri = format!("http://127.0.0.1:{port}");

    let auth_url = {
        let mut url = Url::parse(AUTH_ENDPOINT).map_err(|e| YouTubeError::Oauth(e.to_string()))?;
        url.query_pairs_mut()
            .append_pair("client_id", &client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &scope)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("state", &state);
        url.to_string()
    };

    if let Err(e) = crate::open_external_link(app.clone(), auth_url) {
        let _ = tauri_plugin_oauth::cancel(port);
        return Err(YouTubeError::Oauth(format!("Failed to open browser: {e}")));
    }

    let callback = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
        .await
        .map_err(|_| YouTubeError::OAuthCancelled)?
        .map_err(|_| YouTubeError::OAuthCancelled)?;

    let callback_url =
        Url::parse(&callback).map_err(|e| YouTubeError::Oauth(format!("Bad callback: {e}")))?;
    let mut code = None;
    let mut returned_state = None;
    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => returned_state = Some(value.into_owned()),
            "error" => return Err(YouTubeError::Oauth(value.into_owned())),
            _ => {}
        }
    }

    if returned_state.as_deref() != Some(state.as_str()) {
        return Err(YouTubeError::Oauth("State mismatch".to_string()));
    }
    let code = code.ok_or_else(|| YouTubeError::Oauth("No authorization code".to_string()))?;

    let response = http()?
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|e| YouTubeError::Http(e.to_string()))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(YouTubeError::TokenExchange(body));
    }

    let tokens: TokenResponse = response
        .json()
        .await
        .map_err(|e| YouTubeError::TokenExchange(e.to_string()))?;

    let refresh_token = tokens.refresh_token.ok_or_else(|| {
        YouTubeError::TokenExchange(
            "Google did not return a refresh token. Revoke Cap's access in your Google account and reconnect.".to_string(),
        )
    })?;
    let expires_at = now_secs() + tokens.expires_in.unwrap_or(3600);

    let updated = YouTubeStore::update(app, |s| {
        s.refresh_token = Some(refresh_token);
        s.access_token = Some(tokens.access_token);
        s.access_token_expires_at = Some(expires_at);
        s.delete_scope_granted = want_delete;
    })
    .map_err(YouTubeError::Store)?;

    Ok(updated)
}

/// Returns a valid access token, refreshing via the stored refresh token when the cached one is
/// within 60 seconds of expiry.
pub async fn ensure_access_token(app: &AppHandle) -> Result<String, YouTubeError> {
    let store = YouTubeStore::get(app)
        .map_err(YouTubeError::Store)?
        .ok_or(YouTubeError::NotConnected)?;

    if let (Some(token), Some(expires_at)) = (&store.access_token, store.access_token_expires_at) {
        if expires_at - 60 > now_secs() {
            return Ok(token.clone());
        }
    }

    let refresh_token = store
        .refresh_token
        .clone()
        .ok_or(YouTubeError::NotConnected)?;
    let client_id = store
        .effective_client_id()
        .ok_or(YouTubeError::MissingCredentials)?;
    let client_secret = store
        .effective_client_secret()
        .ok_or(YouTubeError::MissingCredentials)?;

    let response = http()?
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("refresh_token", refresh_token.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| YouTubeError::Http(e.to_string()))?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        if body.contains("invalid_grant") {
            return Err(YouTubeError::NeedsReconnect);
        }
        return Err(YouTubeError::TokenExchange(body));
    }

    let tokens: TokenResponse = response
        .json()
        .await
        .map_err(|e| YouTubeError::TokenExchange(e.to_string()))?;
    let expires_at = now_secs() + tokens.expires_in.unwrap_or(3600);
    let access_token = tokens.access_token.clone();

    YouTubeStore::update(app, |s| {
        s.access_token = Some(tokens.access_token);
        s.access_token_expires_at = Some(expires_at);
        if let Some(new_refresh) = tokens.refresh_token {
            s.refresh_token = Some(new_refresh);
        }
    })
    .map_err(YouTubeError::Store)?;

    Ok(access_token)
}

pub async fn disconnect(app: &AppHandle) -> Result<(), YouTubeError> {
    let store = YouTubeStore::get(app)
        .map_err(YouTubeError::Store)?
        .unwrap_or_default();

    if let Some(refresh_token) = store.refresh_token.as_ref() {
        if let Ok(client) = http() {
            if let Err(e) = client
                .post(REVOKE_ENDPOINT)
                .form(&[("token", refresh_token.as_str())])
                .send()
                .await
            {
                warn!("Failed to revoke YouTube token: {e}");
            }
        }
    }

    YouTubeStore::update(app, |s| {
        s.refresh_token = None;
        s.access_token = None;
        s.access_token_expires_at = None;
        s.channel_id = None;
        s.channel_title = None;
        s.auto_upload = false;
    })
    .map_err(YouTubeError::Store)?;

    Ok(())
}
