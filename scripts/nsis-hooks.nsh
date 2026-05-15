!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Installing WTranscriber runtime dependencies. This can take several minutes."
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\scripts\install-windows-runtime.ps1" -InstallDir "$INSTDIR"'
  Pop $0
  StrCmp $0 "0" runtime_ok
    Abort "WTranscriber runtime dependency installation failed. Check the installer details or $TEMP\wtranscriber-runtime-install.log, then check your network connection."
  runtime_ok:
  DetailPrint "WTranscriber runtime dependencies installed"
!macroend
