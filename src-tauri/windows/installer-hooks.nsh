; Hooks for the NSIS installer (tauri.conf.json > bundle > windows > nsis > installerHooks).
; The installer is per-user, so these run as the user who installs and write to HKCU.

!macro NSIS_HOOK_PREINSTALL
  ; Stop the background listener of an installed version so its files can be replaced
  ; without a "close Arcade Look" prompt.
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
    Sleep 800
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Space in File Explorer, the "Quick Look" context menu item, start at login, and start
  ; the background listener now so previews work right away.
  DetailPrint "Setting up File Explorer integration..."
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --install-integration'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop the background listener and remove the start-at-login and context menu entries.
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-integration'
  Sleep 800
!macroend
