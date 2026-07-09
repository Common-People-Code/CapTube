# Session Log

Development history for the CapTube YouTube-upload feature. See `SESSION_LOG_TEMPLATE.md` for the entry format, and `CLAUDE.md` for current status.

**Current Phase:** Phase 2 — Auto-upload on completion
**Sessions completed:** 1

---

*Add new entries above this line*

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
