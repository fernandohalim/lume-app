# lūme — release checklist

Everything through the app itself is done and shipping as **v1.0** (auth, live
playback + full transport, album-art glow, queue + search, synced lyrics, system
tray + mini-pill). What remains is packaging and cross-platform builds.

## Windows
- [ ] Installer: `.\build.cmd` → NSIS `.exe` + MSI in `src-tauri\target\release\bundle\`.
      First run downloads NSIS/WiX (may need the proxy); fall back to `.\build.cmd --no-bundle`
      for a standalone `lume.exe` if it stalls.
- [ ] Sanity-check the installed app's Start-menu name / icon (product name is `lūme`,
      binary stays `lume.exe` via `mainBinaryName`).

## macOS
- [ ] Build: `bash build.sh` → `.app` + `.dmg`. Uses WKWebView (no WebView2), native
      Keychain, `macOSPrivateApi` already on for transparency/always-on-top.
- [ ] Retina sanity check on the 380×132 window and the mini-pill (216×52).
- [ ] Gatekeeper: unsigned apps warn on first open (right-click → Open, or
      `xattr -dr com.apple.quarantine lume.app`). Proper fix = Apple Developer ID +
      notarization (paid; out of scope for ≤5 personal users).

## Both
- [ ] Verify per-OS bundle icons render (new crescent icon) and tray/dock behavior:
      tray left-click hides/shows, close (×) → tray, tray Quit / ⌘Q exits, minimize →
      taskbar/dock, minify → corner pill.
- [ ] Confirm config travels in `lume.env` next to the app (Client ID + optional proxy);
      the same build runs anywhere. Spotify dev mode caps at 5 allowlisted users.
