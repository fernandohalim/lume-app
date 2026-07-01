# lūme — release checklist (v1.0)

The app is done and shipping as **v1.0** (auth, live playback + full transport,
album-art glow, queue + search, synced lyrics, system tray + mini-pill). What's
left is finishing the macOS build and publishing one combined GitHub release.

## Windows — DONE ✅
- [x] Installer built: NSIS `lūme_1.0.0_x64-setup.exe`. Skipped MSI (`--bundles nsis`)
      because WiX dislikes the non-ASCII product name; NSIS handles it fine.
- [x] Installer **auto-creates `lume.env`** on install via `src-tauri/nsis/hooks.nsh`
      (`installerHooks` in tauri.conf.json) — writes a starter config only if none exists,
      deletes it on uninstall.
- [x] Portable zip: `lume.exe` + ready `lume.env` + `SETUP.txt`.
- [x] Staged in `dist/` (gitignored): `lume_1.0.0_x64-setup.exe`,
      `lume-1.0.0-windows-portable.zip`, `RELEASE_NOTES_v1.0.0.md`.
- [ ] Test-install sanity check: run the installer, confirm a `lume.env` lands next to
      `lume.exe` (Start-menu entry → Open file location) and the crescent icon renders.

## macOS — TODO (do this on the Mac)
- [ ] Build: `bash build.sh` → `.app` + `.dmg` in `src-tauri/target/release/bundle/`.
      Uses WKWebView (no WebView2), native Keychain; `macOSPrivateApi` already on.
      No proxy needed at home. Consider `--target universal-apple-darwin` for Intel + ARM.
- [ ] Give the `.dmg` the same treatment as Windows: it'll be named with the macron
      (`lūme_1.0.0_….dmg`) — copy to an ASCII asset name for the release.
- [ ] Decide the macOS config-drop story: the NSIS hook is Windows-only, so document (in
      the release notes) that Mac users create `lume.env` next to the app, OR add an
      equivalent step for the `.app` (e.g. ship a `lume.env` alongside the `.dmg`).
- [ ] Retina sanity check: 380×132 window + 216×52 mini-pill; tray/dock behavior
      (hide/show, close → tray, ⌘Q quits, minimize, minify → corner pill).
- [ ] Gatekeeper: unsigned → warns on first open (right-click → Open, or
      `xattr -dr com.apple.quarantine lūme.app`). Proper fix = Apple Developer ID +
      notarization (paid; out of scope for ≤5 personal users).

## Publish the release (after macOS is built)
1. Commit the v1.0 source changes (NOT `dist/` — it's gitignored): rename to `lūme`,
   `mainBinaryName`, version bumps, app icon (`assets/icon.svg` + `src-tauri/icons/`),
   `src-tauri/nsis/hooks.nsh`, README, LICENSE. Then `git push`.
2. Stage the macOS artifacts into `dist/` next to the Windows ones.
3. Draft release, then flip to published when happy:
   ```bash
   gh release create v1.0.0 --draft --title "🌙 lūme v1.0.0" \
     --notes-file dist/RELEASE_NOTES_v1.0.0.md \
     dist/lume_1.0.0_x64-setup.exe \
     dist/lume-1.0.0-windows-portable.zip \
     dist/lume-1.0.0-macos-<arch>.dmg
   ```

## Alternative (no Mac needed later)
- [ ] A `.github/workflows/release.yml` on GitHub Actions macOS runners (free for this
      public repo) can build the universal `.dmg` in the cloud and attach it to the
      release on tag push — replaces the manual Mac build. Not set up yet.

## Notes / decisions locked in
- Config model: **example-only** — everyone brings their own Spotify Client ID (no ID
  baked into any artifact). Windows installer auto-creates the starter `lume.env`.
- App is **unsigned** on both platforms (personal use, ≤5 allowlisted Spotify users).
- Phase 6 (hotkeys / autostart / in-app settings) was **dropped** — the app is enough for v1.0.
