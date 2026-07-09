# Self-Contained YouTube Upload for Cap — Implementation Plan

## 1. Goal & hard constraints

Add "connect a YouTube channel → upload finished recordings as *unlisted* → notify with a
copy-link" to the Cap desktop app, such that:

- **No dependency on cap.so** — no web backend, no `apiClient.desktop.*`, no `authStore`, no
  plan gating. The feature works for a user who has never signed into Cap.
- **Not tied to the distributor** — the shipped binary contains **no OAuth client secret and no
  API key**, and consumes **no quota the distributor owns**. Each end-user authenticates against
  *their own* Google Cloud project (bring-your-own-key, BYOK).
- **UI-native** — lives in the existing Integrations settings area and the existing
  post-recording surfaces; reuses the automation engine, notification system, and upload-progress
  plumbing already in the repo.

## 2. Credential model (BYOK by default)

YouTube's Data API quota is charged to the Google Cloud project that owns the OAuth client, not to
the uploader. A single embedded client ID would force every user to share one project's
~10,000 units/day (≈6 uploads/day *total*) and tie the app to whoever owns that project.

**Primary model — Bring-Your-Own OAuth client.** The user creates a Google Cloud project once and
pastes their **Client ID** and **Client secret** (Desktop-app type) into Cap's YouTube settings.

- The distributed binary ships zero secrets. Anyone can fork/redistribute; nothing points back to
  the original author.
- Each user has their own quota. No shared bottleneck, no billing relationship.
- Google's "Desktop app" client type treats the secret as non-confidential and allows loopback
  (`http://127.0.0.1:{port}`) redirects on any port with no pre-registration — exactly what
  `tauri-plugin-oauth` provides.

**Optional convenience for a distributor.** An *optional* build-time default is read via
`option_env!("CAP_YOUTUBE_CLIENT_ID")` / `option_env!("CAP_YOUTUBE_CLIENT_SECRET")`. Left unset
(the default), the binary has no credentials and falls back to the user-supplied fields.

### Google Cloud setup the wizard walks the user through (one-time)

1. Create a project at console.cloud.google.com.
2. Enable **YouTube Data API v3**.
3. Configure the **OAuth consent screen** (External).
4. Create an **OAuth client ID → Application type: Desktop app**.
5. Copy the Client ID + secret into Cap.

### Caveats surfaced in the wizard

- `youtube.upload` is a **sensitive** scope. While the consent screen is in **Testing** status,
  only added test users can authorize *and refresh tokens expire after 7 days*. To avoid weekly
  re-auth, the user should **Publish** the consent screen to **Production** (sensitive — not
  restricted — scopes can go to production without formal Google verification; users see a one-time
  "Google hasn't verified this app" screen they accept).

## 3. Architecture & data flow

```
Settings ▸ Integrations ▸ YouTube
  ├─ (wizard) paste Client ID + secret ──► youtubeStore (tauri-plugin-store key "youtube")
  ├─ Connect ─► youtube_connect()  [Rust]
  │     PKCE verifier/challenge → tauri-plugin-oauth loopback port
  │     → open accounts.google.com/o/oauth2/v2/auth?...&redirect_uri=http://127.0.0.1:{port}
  │     → capture ?code=  → POST oauth2.googleapis.com/token
  │     → store {access_token, refresh_token, expires_at}
  ├─ Channel picker ─► youtube_list_channels()  → GET youtube/v3/channels?mine=true
  └─ Toggle "Auto-upload finished recordings as unlisted" + default privacy

Recording finishes ─► automation engine ─► Action::UploadToYouTube   (Phase 2)
  or manual "Upload to YouTube" button                                (Phase 1)
        │
        ├─ ensure .mp4 exists (studio: headless render; instant: already present)
        ├─ ensure_access_token()  (refresh if expired)
        ├─ resumable upload  ► POST upload/youtube/v3/videos?uploadType=resumable
        │                     ► PUT bytes in chunks (Content-Range) → progress Channel
        ├─ store youtu.be/<id> on RecordingMeta.youtube
        └─ clipboard.write(url) + native notification "Uploaded — link copied"
```

Everything is inside `apps/desktop/src-tauri` + the desktop SolidStart frontend + the
`crates/automation` crate. No other app/package is touched.

## 4. Components

### A. Local persistence — `youtubeStore`

New `tauri-plugin-store` key `"youtube"` (same store file, same pattern as `auth.rs`/`AuthStore`).

Rust — `apps/desktop/src-tauri/src/youtube/store.rs`:

```rust
#[derive(Serialize, Deserialize, Type, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeStore {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    pub access_token_expires_at: Option<i64>,
    pub channel_id: Option<String>,
    pub channel_title: Option<String>,
    #[serde(default)]
    pub auto_upload: bool,
    #[serde(default)]
    pub default_privacy: YouTubePrivacy,
}

#[derive(Serialize, Deserialize, Type, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum YouTubePrivacy { #[default] Unlisted, Private, Public }
```

- `get` / `update` / `save` mirroring `general_settings.rs`.
- `effective_client_id()/secret()` = stored value, else `option_env!` default, else `None`.
- Security: plaintext parity with existing `auth.rs`; keychain hardening is a later pass.

Frontend — `apps/desktop/src/store.ts`: `export const youtubeStore = declareStore<YouTubeStore>("youtube");`

### B. OAuth — `apps/desktop/src-tauri/src/youtube/oauth.rs`

Reuses the loopback mechanism `apps/desktop/src/utils/auth.ts` uses; token exchange runs in Rust
with the pooled `HttpClient` from `http_client.rs`.

- `youtube_connect(app) -> Result<YouTubeConnection, YouTubeError>`: PKCE (S256), start loopback,
  open auth URL with `access_type=offline&prompt=consent`, capture `code`, validate `state`,
  exchange at `oauth2.googleapis.com/token`, persist tokens, fetch channels.
- `ensure_access_token(app) -> Result<String, YouTubeError>`: refresh when <60s remaining; handle
  `invalid_grant` → `NeedsReconnect`.
- `youtube_disconnect(app)`: best-effort token revoke, clear fields.

`YouTubeError` (`thiserror`, `Serialize + Type`): `MissingCredentials`, `OAuthCancelled`,
`TokenExchange`, `NeedsReconnect`, `Http`, `QuotaExceeded`, `Api{code,message}`, `FileNotFound`,
`NotConnected`.

### C. YouTube API client — `apps/desktop/src-tauri/src/youtube/api.rs`

Rust `reqwest` is not gated by the Tauri `http:` capability, so no capability change is needed.

- `list_channels(app) -> Vec<YouTubeChannel>`: `GET youtube/v3/channels?part=snippet&mine=true`.
  Scope kept minimal (`youtube.upload` + `youtube.readonly`); no `userinfo.email`, show channel
  title only.
- `upload_video(app, file, meta, on_progress) -> Result<String, YouTubeError>`: resumable upload —
  init `POST .../videos?uploadType=resumable&part=snippet,status` with
  `{status:{privacyStatus:"unlisted", selfDeclaredMadeForKids:false}, snippet:{...}}`, read
  `Location`, PUT ~8 MB chunks with `Content-Range`, handle `308`, parse `id`, build
  `https://youtu.be/{id}`. Progress via existing `tauri::ipc::Channel<UploadProgress>`.

### D. RecordingMeta — persist the URL

`crates/project/src/meta.rs`: add `youtube: Option<YouTubeSharingMeta>` to `RecordingMeta` with
`struct YouTubeSharingMeta { video_id, url, privacy, uploaded_at }` (mirrors `SharingMeta`). Add to
specta `.typ::<…>()`.

### E. Automation action (Phase 2)

- `crates/automation/src/types.rs`: `Action::UploadToYouTube { privacy, copy_link, title_template }`.
- `crates/automation/src/lib.rs`: `Capability::UploadToYouTube`, `required_capability` mapping,
  dispatch arm, trait method `upload_to_youtube`.
- `apps/desktop/src-tauri/src/automation.rs`: `DesktopAutomationHost::upload_to_youtube` next to
  `upload()` — ensures render for studio, uploads, stores meta, clipboard + notify.
- `apps/desktop/src/utils/automations.ts`: label + context maps.
- Settings toggle writes a single managed automation rule (both recording-finished triggers +
  `UploadToYouTube{unlisted}`) via `automation::set_automations`.

### F. Manual upload command + button (Phase 1)

- `youtube_upload_recording(app, project_path, privacy, channel) -> Result<String, YouTubeError>`.
- UI: "Upload to YouTube" button in `routes/editor/ShareButton.tsx` and
  `routes/recordings-overlay.tsx`, reusing the progress dialog; when `RecordingMeta.youtube` exists,
  switch to "Copy YouTube link".

### G. Settings UI (Phase 1)

- Card in `settings/integrations/index.tsx`.
- `settings/integrations/youtube-config.tsx`: credentials wizard + inputs, Connect/Disconnect,
  channel `<Select>`, auto-upload toggle, default-privacy select. No Pro/org logic.

### H. Notification + copy link (Phase 1; action button Phase 3)

- `notifications.rs`: `YouTubeUploadComplete` / `YouTubeUploadFailed`. Copy URL to clipboard + fire
  native notification (gated by `enable_notifications`).
- Phase 3: real "Copy link" action button (extend `send_notification` + register a
  notification-action listener).

### I. Command registration & bindings

- New `youtube` module in `lib.rs`; register commands in `collect_commands!`; add `.typ::<…>()`.
- Bindings regenerate into `apps/desktop/src/utils/tauri.ts` on the next debug desktop run
  (generated file — not hand-edited).

### J. Capabilities

Already satisfied in `capabilities/default.json`: `oauth:allow-start`, `notification:default`,
`clipboard-manager:allow-write-text`, `http:default` (`https://*`). No changes required.

## 5. Studio vs instant

| | mp4 at finish? | Auto-upload |
|---|---|---|
| Instant | Yes (`content/output.mp4`) | Upload immediately on `instantRecordingFinished` |
| Studio | No (only after export → `output/result.mp4`) | Headless render then upload on `studioRecordingFinished` |

## 6. Error handling & edge cases

Missing credentials → wizard; revoked/expired refresh token → `NeedsReconnect`; `quotaExceeded`
→ notify, keep local files; offline/transient → resume from committed offset; duplicate →
copy existing link; large files → streamed chunk PUT.

## 7. Testing

Rust unit tests (PKCE derivation, auth-URL, token parsing, Content-Range math, action serde,
capability mapping, mocked HTTP); automation `MockHost` extension; manual E2E with a real Google
project; `/verify` before commit. Gates: `cargo fmt --all`, `cargo check -p cap-desktop`,
`cargo check -p cap-automation`, `biome check --write` on touched TS.

## 8. Phased delivery

**Phase 1 — MVP**: youtube Rust module (store/oauth/api), `RecordingMeta.youtube`, notifications,
`youtubeStore`, integrations card, `youtube-config.tsx` wizard + channel picker, manual upload
button.

**Phase 2 — Auto-upload**: automation `Action::UploadToYouTube` + `Capability` + host impl + TS
labels; settings toggle → managed rule.

**Phase 3 — Notification action button**: action-carrying notification variants + listener.

## 9. Confirmed decisions

1. BYOK by default (no baked credentials in shipped build; `option_env!` path present but unset).
2. Scopes: `youtube.upload` + `youtube.readonly`; no `userinfo.email` (show channel title).
3. Auto-upload enablement via a single managed automation rule + raw action in Automations tab.
4. Plaintext `tauri-plugin-store` for MVP; keychain hardening later.
