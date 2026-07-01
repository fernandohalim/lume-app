; lūme NSIS installer hooks.
; Auto-create a starter lume.env next to the app on install so the user only
; has to paste their Client ID — but never overwrite an existing one, so
; reinstalls / upgrades keep their configuration.

!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$INSTDIR\lume.env" lume_env_exists
    Push $0
    FileOpen $0 "$INSTDIR\lume.env" w
    FileWrite $0 "# lume configuration - set your Spotify Client ID below, then restart lume.$\r$\n"
    FileWrite $0 "# Create an app at https://developer.spotify.com/dashboard (select Web API),$\r$\n"
    FileWrite $0 "# set the redirect URI to http://127.0.0.1:8888/callback, and add your$\r$\n"
    FileWrite $0 "# account under User Management. You need Spotify Premium.$\r$\n"
    FileWrite $0 "LUME_SPOTIFY_CLIENT_ID=your_client_id_here$\r$\n"
    FileWrite $0 "$\r$\n"
    FileWrite $0 "# Optional proxy (corporate networks only). Leave blank on a normal connection.$\r$\n"
    FileWrite $0 "LUME_HTTP_PROXY=$\r$\n"
    FileClose $0
    Pop $0
  lume_env_exists:
!macroend

; Clean up the generated config on uninstall (the installer didn't track it).
!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\lume.env"
!macroend
