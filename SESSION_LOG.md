# Session Log

Development history for the CapTube YouTube-upload feature. See `SESSION_LOG_TEMPLATE.md` for the entry format, and `CLAUDE.md` for current status.

**Current Phase:** Phase 3 pending · live dogfood in progress (blocked on a blank settings page)
**Sessions completed:** 3

---

*Add new entries above this line*

---

## Session 3 — 2026-07-10 (live dogfood, in progress)

### What We Accomplished
- Got the desktop app building and running locally on matt's Mac (Apple M1 Pro, macOS).
- Documented the full macOS build path and the gotchas hit along the way.

### Build gotchas resolved (macOS)
- **Rust PATH**: after `rustup`, must `. "$HOME/.cargo/env"` (or new shell) before `cargo`/`pnpm dev:desktop`.
- **Repo not cloned**: `git clone https://github.com/Common-People-Code/CapTube.git` then `git checkout claude/cap-youtube-upload-vyv3pr`.
- **`env-setup`**: desktop only; accept default `VITE_SERVER_URL=https://cap.so` (the YouTube feature ignores it); decline Docker/S3/MySQL.
- **`pnpm dev:desktop`** auto-runs `cap-setup` (downloads FFmpeg/native-deps) + `build:sidecar` (builds cap-muxer/cap-exporter/cap-cli) — no manual sidecar build needed.
- **Full Xcode required**: build failed compiling `cidre` with `xcodebuild requires Xcode` until full Xcode was installed and selected (`sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer && sudo xcodebuild -license accept`). Command Line Tools alone is not enough.
- **Screen Recording permission in dev** attaches to the **terminal app** (Terminal/iTerm), not "Cap", because the recorder runs as a child of the dev process. Grant it there and fully quit/relaunch the terminal.

### Open issue (BLOCKING) — YouTube settings page renders blank/gray
- Symptom: Settings → Integrations → YouTube → Configure shows a gray page. `location.href` = `http://localhost:3002/settings/integrations/youtube-config` (correct route/window). `document.querySelector('#app').innerText` = `""`. **No console errors.** Backend is fine: `window.__TAURI_INTERNALS__.invoke('youtube_get_status')` returns `{connected:false,hasCredentials:false,...}`.
- Tried: removed the page-level `<Suspense>` gate (whose fallback was a text-less spinner) so the form renders immediately — committed (91151b4) and confirmed HMR-loaded via Vite `page reload …/youtube-config.tsx` — but **still blank**.
- Next hypotheses to check: (a) a silent ErrorBoundary swallowing a render error; (b) the shared `Section`/`SettingsPageContent` from `../Setting`; (c) SSR pass rendering empty (Vite logs an `(ssr)` reload). Plan: push a version with `console.log("[yt-config] mount")` + an unconditional visible banner to determine whether the component mounts at all.

### Next Session Should
- [ ] Resolve the blank YouTube settings page (instrument mount + first visible element).
- [ ] Then run the real end-to-end: paste Google client id/secret → connect → pick channel → record → upload.
- [ ] Revisit the channelId-in-insert nuance (YouTube ignores `snippet.channelId`; channel routing is set at OAuth consent) and Phase 3 (native notification buttons are mobile-only in tauri-plugin-notification v2).

### Notes
- PR #1 has Phases 1 & 2 + docs. Branch: `claude/cap-youtube-upload-vyv3pr`.

---

## Session 2 — 2026-07-09

### What We Accomplished
- Opened **PR #1** for Phase 1.
- Added session-tracking docs (`CLAUDE.md`, `SESSION_LOG.md`, `SESSION_LOG_TEMPLATE.md`).
- Built **Phase 2 — auto-upload on completion** (compile/typecheck/lint verified, 21 automation tests pass):
  - `Action::UploadToYouTube { privacy, copyLink }` + `Capability::UploadToYouTube` in `crates/automation` (types.rs, lib.rs: `required_capability`, dispatch, `AutomationHost::upload_to_youtube`).
  - Implemented on `DesktopAutomationHost` — renders studio recordings first (default 1080p/web profile), then calls the shared `youtube::upload_project`. MockHost + CLI host updated (CLI reports it desktop-only).
  - Refactored `youtube_upload_recording` into a shared `pub async fn upload_project(app, path, privacy_override, copy_link, progress)` used by both the manual command and the automation host.
  - Frontend: `uploadToYouTube` in all `automations.ts`/`automations.tsx` action maps; auto-upload toggle in `youtube-config.tsx` that manages a pair of automation rules via `setYouTubeAutoUpload`.
  - Regenerated `tauri.ts` (restored macOS-only types after the Linux regen, as in Session 1).

### Technical Decisions Made

**Two managed rules, not one**
- What: The auto-upload toggle writes two rules (`youtube-auto-upload-studioRecordingFinished` and `-instantRecordingFinished`).
- Why: An `AutomationRule` has a single `trigger`; both finished-recording triggers must be covered.
- Alternatives considered: a multi-trigger rule (not supported by the schema).

**Shared `upload_project` instead of duplicating upload logic**
- What: Extracted the manual command's body into a reusable function taking a privacy override + copy-link flag.
- Why: The automation host and the manual command need identical upload+persist+notify behavior; one code path avoids drift.

**Studio render lives in the host, not the youtube module**
- What: `DesktopAutomationHost::upload_to_youtube` renders via the existing `export` path when `output_path()` is missing.
- Why: Keeps the `youtube` module free of the `cap_export` dependency; reuses the host's proven render path.

### Files Created / Modified
- `crates/automation/src/{types,lib,tests}.rs` — action/capability/trait/dispatch + MockHost.
- `apps/desktop/src-tauri/src/automation.rs` — host impl + `default_youtube_export_profile`.
- `apps/desktop/src-tauri/src/youtube/{mod,store}.rs` — `upload_project`, `YouTubePrivacy::from_api_value`.
- `apps/cli/src/automation.rs` — CLI host stub (desktop-only error).
- `apps/desktop/src/utils/automations.ts` — labels/context + `get/setYouTubeAutoUpload`.
- `apps/desktop/src/routes/(window-chrome)/settings/automations.tsx` — action label maps.
- `apps/desktop/src/routes/(window-chrome)/settings/integrations/youtube-config.tsx` — auto-upload toggle.
- `apps/desktop/src/utils/tauri.ts` — regenerated (`Action` now has `uploadToYouTube`).

### Blockers / Issues
- Outstanding: still no live end-to-end run (no GUI in the build env). Phases 1 & 2 are compile/typecheck/lint-verified only.

### Next Session Should
- [ ] Phase 3: notification "Copy link" action button (thread the URL through `NotificationType`, register a notification-action listener).
- [ ] Dogfood live with a real Google project; confirm studio auto-render + upload path end-to-end.

### Notes
- The raw "Upload to YouTube" automation action shows in the Automations tab with default privacy/copyLink; the primary UX is the settings toggle.
- Auto-upload rules are keyed by a stable id prefix (`youtube-auto-upload-…`) so the toggle can find and remove them idempotently.

---

## Session 1 — 2026-07-09

### What We Accomplished
- Reviewed the Cap codebase and designed a **self-contained, cap.so-independent** YouTube upload feature; wrote the phased plan to `analysis/plans/youtube-upload.md`.
- Built **Phase 1** end-to-end (compile/typecheck/lint verified) and opened **PR #1**:
  - Rust `youtube/` module — BYOK Google OAuth (PKCE + loopback via `tauri-plugin-oauth`), token refresh/revoke, channel listing, and chunked **resumable** upload to the YouTube Data API with `privacyStatus: unlisted`.
  - `RecordingMeta.youtube` persistence; clipboard copy + native notification on success.
  - Integrations settings card + `youtube-config.tsx` (Google Cloud setup wizard, credential entry, connect/disconnect, channel picker, default privacy).
  - Manual "Upload to YouTube" button in the editor header.
  - Regenerated `tauri.ts` bindings via a new opt-in `make_specta_builder` test.
- Added session-tracking docs: this file, `SESSION_LOG_TEMPLATE.md`, and a filled-in `CLAUDE.md`.

### Technical Decisions Made

**BYOK (bring-your-own Google OAuth client)**
- What: The app ships no client ID/secret; each user pastes their own from their own Google Cloud project. An optional `option_env!("CAP_YOUTUBE_CLIENT_ID/SECRET")` build-time default exists but is unset.
- Why: The YouTube Data API quota is billed to the OAuth client's project. A single embedded client would force every user to share one ~10k-units/day quota (~6 uploads/day total) and tie the app to one owner. BYOK keeps it distributable and untethered.
- Alternatives considered: (a) server-side OAuth mirroring Cap's Google Drive integration — rejected because it reintroduces a cap.so dependency; (b) a single bundled client — rejected on the shared-quota bottleneck.

**Desktop-direct upload, no web backend**
- What: The desktop obtains tokens and uploads straight to googleapis; nothing routes through cap.so.
- Why: Matches how Cap already uploads directly to S3, avoids a bandwidth relay, and satisfies the independence requirement.

**Auto-upload deferred to the automation engine (Phase 2)**
- What: The auto-upload toggle is not in the Phase 1 UI; it will create a single managed automation rule.
- Why: Studio recordings have no `.mp4` until rendered; the automation host already renders headlessly, so reusing it avoids a bespoke render path. Shipping a dead toggle in Phase 1 would mislead.

### Files Created / Modified
- `analysis/plans/youtube-upload.md` — NEW: phased implementation plan.
- `apps/desktop/src-tauri/src/youtube/{mod,store,oauth,api}.rs` — NEW: the YouTube module.
- `apps/desktop/src-tauri/src/notifications.rs` — YouTube notification variants.
- `apps/desktop/src-tauri/src/lib.rs` — command registration; extracted `make_specta_builder()` + opt-in binding-export test.
- `crates/project/src/meta.rs` — `RecordingMeta.youtube` + `YouTubeSharingMeta` (timestamp `#[specta(type = f64)]`).
- `apps/desktop/src/routes/(window-chrome)/settings/integrations/{index,youtube-config}.tsx` — card + settings page.
- `apps/desktop/src/routes/editor/{YouTubeUploadButton,Header}.tsx` — manual upload button.
- `apps/desktop/src/store.ts`, `apps/desktop/src/utils/tauri.ts` — store accessor + regenerated bindings.
- `CLAUDE.md`, `SESSION_LOG.md`, `SESSION_LOG_TEMPLATE.md` — NEW: session tracking.

### Blockers / Issues
- Resolved: fresh sandbox lacked GTK/ffmpeg/alsa system libs and the gitignored sidecar binaries — installed the libs and stubbed the sidecars (not committed) to get `cargo check` and the binding-export test running.
- Resolved: regenerating bindings on Linux flipped `SystemDiagnostics`/`MacOSVersionInfo` to their non-macOS form — hand-restored those two lines so the diff is only the YouTube additions.
- Outstanding: no GUI in the build environment, so the OAuth/upload flow is compile/typecheck/lint-verified but not run live. Needs a real dogfood pass with a Google project.

### Next Session Should
- [ ] Phase 2: add `Action::UploadToYouTube` + `Capability::UploadToYouTube` to `crates/automation`, implement `upload_to_youtube` on `DesktopAutomationHost`.
- [ ] Add the auto-upload toggle to `youtube-config.tsx` that writes a single managed automation rule.
- [ ] Regenerate bindings; run `cargo check`/`clippy` + `tsc`/`biome`.
- [ ] Dogfood Phase 1 live and report back before Phase 3.

### Notes
- Instant vs studio recordings: instant has the `.mp4` at stop; studio only after export. This shapes the auto-upload design.
- `cargo fmt --all` touches unrelated files — revert them before committing.
- Tokens are stored in plaintext `tauri-plugin-store` (parity with `auth.rs`); keychain hardening is a later pass.

---

*Framework v2.0 | February 2026*
