# Session Log

Development history for the CapTube YouTube-upload feature. See `SESSION_LOG_TEMPLATE.md` for the entry format, and `CLAUDE.md` for current status.

**Current Phase:** Phase 3 — Notification action button
**Sessions completed:** 3

---

*Add new entries above this line*

---

## Session 3 — 2026-07-15

### What We Accomplished
- Diagnosed a build failure the user hit on macOS with ~12 GB free disk. Established it was **build scratch (`target/`), not recording and not the distributable** — the two had been conflated. The shipped DMG is a few hundred MB; the 20–40 GB is compiler intermediates.
- Found the real waste: `[profile.release]` carried `debug = true`, baking full debuginfo into every shipped binary (main app + `cap-muxer`/`cap-exporter`/`cap-cli` sidecars). Tauri doesn't strip by default, so it bloated both `target/` and the DMG.
- **Shrunk release builds and the distributable**: set `debug = false` + `strip = true` on `[profile.release]`; kept `lto`/`opt-level = "s"`/`codegen-units = 1` (those keep the binary small — only the debuginfo was dead weight). `cargo verify-project` passes.
- Documented the build footprint and low-disk build path (relocating `target/` via `CARGO_TARGET_DIR`, e.g. to an external HDD) in `README.md` + `CLAUDE.md`.
- Opened **PR #2** on branch `claude/project-status-disk-space-6neui5` (branched fresh off `main` after PR #1 merged; commits `95ee344` docs + `499a05f` profile+docs) — now **merged** into `main` at `0e800ff`.
- Follow-up on the same branch (rebased onto merged `main`): added the full **external-drive build walkthrough** to `README.md` (format check, `pnpm cap-setup`, `CARGO_TARGET_DIR` build, keep-awake) plus the native-deps-stay-in-repo nuance. Pushed as a **new PR** since PR #2 is closed/merged.

### Technical Decisions Made

**Strip + drop debuginfo, but leave LTO/panic alone**
- What: `debug = false`, `strip = true`; did *not* touch `lto`, `opt-level`, or `panic = "unwind"`.
- Why: Debuginfo was the wasteful part inflating both build scratch and the DMG. LTO + `opt-level = "s"` make the binary *smaller*, so they stay. `panic = "abort"` would shave more but risks `catch_unwind`-based error handling in the recording paths.
- Trade-off accepted: release crash backtraces lose symbol/line detail — fine for a self-distributed fork. Follow-up if richer crash reports are ever needed: `split-debuginfo` with separately-archived symbols.

**Docs-only for `CARGO_TARGET_DIR`, not a committed `.cargo/config.toml`**
- What: Documented the env var rather than committing a `[build] target-dir` override.
- Why: A committed target-dir would force everyone onto one machine-specific path. The env var is per-user/per-session.

### Files Created / Modified
- `Cargo.toml` — `[profile.release]`: `debug = true` → `debug = false`, added `strip = true`.
- `README.md` — reframed the disk requirement as build scratch (not the distributable); rewrote the **Reducing build disk usage** section; added the **Building against an external drive (macOS)** subsection.
- `CLAUDE.md` — build-footprint note under the checks block.
- `SESSION_LOG.md` — this entry.

### Blockers / Issues
- Outstanding: no full `pnpm tauri:build` run in this environment (no GUI, sidecar binaries gitignored), so the exact size reduction from the profile change is **unmeasured** — confirm on a real Mac.
- Outstanding (carried from before): the YouTube feature still has no live end-to-end dogfood run, and Phase 3 (notification "Copy link" action button) is not started.

### Next Session Should
- [ ] On a Mac, run `pnpm cap-setup` then `CARGO_TARGET_DIR=/Volumes/<HDD>/cap-target pnpm tauri:build`; confirm the DMG builds and note the size delta.
- [x] Fold the external-HDD recipe + the native-deps nuance into `README.md` — done (new PR).
- [ ] Resume Phase 3: notification "Copy link" action button.
- [ ] Dogfood the YouTube flow live against a real Google project.

### Notes
- **`CARGO_TARGET_DIR` moves only the compiler scratch.** Native deps (ffmpeg, onnxruntime, macOS `Spacedrive.framework`) are hardcoded to the *in-repo* `target/` via `scripts/setup.js` (`const targetDir = path.join(__root, "target")`, line 17) and `tauri.conf.json` (line 81). They're small and stay on the internal disk — which is why redirecting scratch to an external drive works without breaking the bundle.
- **External build disk must be APFS or HFS+.** exFAT/FAT32/NTFS lack symlinks, Unix permissions, and case sensitivity the Rust build needs — it will fail with confusing linker/permission errors.
- macOS: `caffeinate -s` in a spare terminal keeps a spinning HDD from sleeping mid-build.

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
