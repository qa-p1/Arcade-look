; Tauri NSIS hooks backed by Arcade Link's checksum-pinned shared macros.
!define ARCADE_ID "arcade.look"
!define ARCADE_NAME "${PRODUCTNAME}"
!define ARCADE_VERSION "${VERSION}"
!define ARCADE_EXE "${MAINBINARYNAME}.exe"
!define ARCADE_CLI "arcade-look"
!define ARCADE_HOMEPAGE "https://github.com/qa-p1/Arcade-catalog"
!define ARCADE_START_AT_LOGIN_DEFAULT 1
!include "${__FILEDIR__}\arcade.nsh"

!macro NSIS_HOOK_PREINSTALL
  ; Stop an installed background listener before replacing its payload.
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --quit'
    Sleep 800
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Writes the shared receipt, Start menu/optional shortcuts, CLI shim and PATH.
  !insertmacro ArcadeInstall
  ; Keep Look's Explorer integration, while the shared Startup task is the
  ; single optional start-at-login entry (it starts the same background mode).
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --install-integration --silent' $ArcadeExitCode
  ${If} $ArcadeExitCode != 0
    SetErrorLevel $ArcadeExitCode
    Abort "Arcade Look integration setup failed."
  ${EndIf}
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ArcadeLook"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Updates keep shared integration in place; normal uninstall is receipt-owned.
  ${If} $UpdateMode == 1
    StrCpy $ArcadeExitCode 0
  ${Else}
    !insertmacro ArcadeUninstall
    ${If} $ArcadeExitCode == 0
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-integration' $0
      ${If} $0 != 0
        SetErrorLevel $0
        Abort "Arcade Look integration removal failed."
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
