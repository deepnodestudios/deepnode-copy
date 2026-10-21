; Moves a shell extension DLL that Explorer may hold loaded out of the way so the new copy can be
; written. Each run uses a fresh temp name: a fixed "deepnode_shell.old.dll" stays locked while an
; Explorer instance still uses it, and then the rename (and the whole DLL update) silently failed.
; The moved file is deleted now if possible, otherwise on the next reboot.
!macro DN_RETIRE_SHELL_DLL DIR
  Push $0
  Delete /REBOOTOK "${DIR}\deepnode_shell.old.dll"
  IfFileExists "${DIR}\deepnode_shell.dll" 0 +5
    GetTempFileName $0 "${DIR}"
    Delete "$0"
    Rename "${DIR}\deepnode_shell.dll" "$0"
    Delete /REBOOTOK "$0"
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; Give the running app time to flush history/Sentry and exit cleanly after launching us.
  Sleep 2000
  ; Terminate any remaining DeepNode Copy instances
  DetailPrint "Durduruluyor: Calisan DeepNode Copy surecleri..."
  nsExec::Exec 'taskkill /F /IM deepnode-gui.exe /IM DeepNodeCopy.exe /IM "DeepNode Copy.exe"'

  ; Move loaded shell extension DLLs aside so file copy is not blocked by Windows Explorer.
  !insertmacro DN_RETIRE_SHELL_DLL "$INSTDIR"
  !insertmacro DN_RETIRE_SHELL_DLL "$INSTDIR\bin"

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

  ; The sparse package manifest must resolve its COM DLL, executable and assets from one
  ; external location, so mirror the resource bundle into the install root.
  IfFileExists "$INSTDIR\_up_\bin\AppxManifest.xml" 0 +5
    CopyFiles /SILENT "$INSTDIR\_up_\bin\AppxManifest.xml" "$INSTDIR\AppxManifest.xml"
    CopyFiles /SILENT "$INSTDIR\_up_\bin\DeepNodePublic.cer" "$INSTDIR\DeepNodePublic.cer"
    CreateDirectory "$INSTDIR\Assets"
    CopyFiles /SILENT "$INSTDIR\_up_\bin\Assets\*.*" "$INSTDIR\Assets"

  ; Register the COM shell extension DLL cleanly
  DetailPrint "Windows Gezgini kabuk uzantisi kaydediliyor..."
  IfFileExists "$INSTDIR\deepnode_shell.dll" 0 +2
    ExecWait 'regsvr32.exe /s "$INSTDIR\deepnode_shell.dll"'

  ; Older versions started with Windows; the shell extension now launches the app on demand.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DeepNode Copy"

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

  ; Remove the per-user static verbs and Explorer handler entries written by the app.
  DeleteRegKey HKCU "Software\Classes\*\shell\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\*\shell\DeepNodeMove"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\DeepNodeMove"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\DeepNodeMove"

  DeleteRegKey HKCU "Software\Classes\*\shellex\DragDropHandlers\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Directory\shellex\DragDropHandlers\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Folder\shellex\DragDropHandlers\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Drive\shellex\DragDropHandlers\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\lnkfile\shellex\DragDropHandlers\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Directory\shellex\CopyHookHandlers\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Folder\shellex\CopyHookHandlers\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Directory\shellex\DropHandler\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Folder\shellex\DropHandler\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shellex\DropHandler\DeepNodeCopy"
  DeleteRegKey HKCU "Software\Classes\Drive\shellex\DropHandler\DeepNodeCopy"

  ; Remove the Windows 11 sparse context-menu package for the current user.
  nsExec::Exec 'powershell -NoProfile -Command "Get-AppxPackage -Name DeepNodeCopy | Remove-AppxPackage -ErrorAction SilentlyContinue"'

  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DeepNode Copy"

  ; Delete old renamed DLLs on reboot if still locked
  Delete /REBOOTOK "$INSTDIR\deepnode_shell.old.dll"
  Delete /REBOOTOK "$INSTDIR\bin\deepnode_shell.old.dll"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend
