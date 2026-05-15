!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Installing WTranscriber runtime dependencies"
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\scripts\install-windows-runtime.ps1" -InstallDir "$INSTDIR"'
  Pop $0
  StrCmp $0 "0" runtime_ok
    Abort "WTranscriber runtime dependency installation failed. Check the installer log and your network connection."
  runtime_ok:
!macroend
