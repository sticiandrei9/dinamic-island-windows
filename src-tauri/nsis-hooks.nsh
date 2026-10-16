; Scoate hook-urile Claude Code din ~/.claude/settings.json la dezinstalare (nu și la actualizare).
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\dynamic-island.exe" --claude-unhook'
  ${EndIf}
!macroend
