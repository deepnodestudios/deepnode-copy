!macro NSIS_HOOK_PREINSTALL
  ; Terminate any running DeepNode Copy instances
  DetailPrint "Durduruluyor: Calisan DeepNode Copy surecleri..."
  nsExec::Exec 'taskkill /F /IM deepnode-gui.exe /IM DeepNodeCopy.exe /IM "DeepNode Copy.exe"'
  
  ; Rename loaded shell extension DLLs so file copy is not blocked by Windows Explorer.
  ; Rename fails when the target exists, so drop the previous .old first (or on reboot if Explorer holds it).
  Delete /REBOOTOK "$INSTDIR\deepnode_shell.old.dll"
  IfFileExists "$INSTDIR\deepnode_shell.dll" 0 +2
    Rename "$INSTDIR\deepnode_shell.dll" "$INSTDIR\deepnode_shell.old.dll"

  Delete /REBOOTOK "$INSTDIR\bin\deepnode_shell.old.dll"
  IfFileExists "$INSTDIR\bin\deepnode_shell.dll" 0 +2
    Rename "$INSTDIR\bin\deepnode_shell.dll" "$INSTDIR\bin\deepnode_shell.old.dll"

  ; Clean up obsolete v0.2.6 executable so it is never invoked
  Delete "$INSTDIR\DeepNodeCopy.exe"
  Delete "$INSTDIR\DeepNode Copy.exe"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Remove any stale legacy executables
  Delete "$INSTDIR\DeepNodeCopy.exe"
  Delete "$INSTDIR\DeepNode Copy.exe"

  ; Tauri installs the "../bin/deepnode_shell.dll" resource as _up_\bin\deepnode_shell.dll;
  ; place that fresh copy in both root and bin folders.
  IfFileExists "$INSTDIR\_up_\bin\deepnode_shell.dll" 0 +3
    CopyFiles /SILENT "$INSTDIR\_up_\bin\deepnode_shell.dll" "$INSTDIR\deepnode_shell.dll"
    CopyFiles /SILENT "$INSTDIR\_up_\bin\deepnode_shell.dll" "$INSTDIR\bin\deepnode_shell.dll"

  ; Register the COM shell extension DLL cleanly
  DetailPrint "Windows Gezgini kabuk uzantisi kaydediliyor..."
  IfFileExists "$INSTDIR\deepnode_shell.dll" 0 +2
    ExecWait 'regsvr32.exe /s "$INSTDIR\deepnode_shell.dll"'

  ; Refresh Windows Explorer shell cache
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Durduruluyor: Calisan DeepNode Copy surecleri..."
  nsExec::Exec 'taskkill /F /IM deepnode-gui.exe /IM DeepNodeCopy.exe /IM "DeepNode Copy.exe"'

  ; Unregister shell extension before removal
  IfFileExists "$INSTDIR\deepnode_shell.dll" 0 +2
    ExecWait 'regsvr32.exe /u /s "$INSTDIR\deepnode_shell.dll"'

  IfFileExists "$INSTDIR\bin\deepnode_shell.dll" 0 +2
    ExecWait 'regsvr32.exe /u /s "$INSTDIR\bin\deepnode_shell.dll"'

  ; Delete old renamed DLLs on reboot if still locked
  Delete /REBOOTOK "$INSTDIR\deepnode_shell.old.dll"
  Delete /REBOOTOK "$INSTDIR\bin\deepnode_shell.old.dll"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend
