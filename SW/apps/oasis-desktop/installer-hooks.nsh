; Extra steps for the OASIS installer (Tauri NSIS hooks). It installs for the current user, so the
; Windows startup entry lives under HKCU, the same one the app's "Start with Windows" switch writes
; (value ProjectOASIS, the program in quotes and --tray, so it starts quietly in the tray).
;
; The C# OASIS uses the same value name. An entry that is not this copy's is never replaced without
; asking, and a silent install leaves it alone.

!macro NSIS_HOOK_POSTINSTALL
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ProjectOASIS"
  StrCmp $0 '"$INSTDIR\${MAINBINARYNAME}.exe" --tray' oasis_skip_autostart
  StrCmp $0 "" oasis_new_autostart oasis_other_autostart
  oasis_new_autostart:
    ; a silent install (/S) answers Yes: OASIS is for a pad that should always be ready
    MessageBox MB_YESNO|MB_ICONQUESTION "Start OASIS automatically when you sign in to Windows?$\r$\n$\r$\nIt starts quietly in the tray, so your pad buttons and voice are ready as soon as you sit down. You can change this later in OASIS, under Settings." /SD IDYES IDNO oasis_skip_autostart
    Goto oasis_set_autostart
  oasis_other_autostart:
    MessageBox MB_YESNO|MB_ICONQUESTION "Windows is already set to start another copy of OASIS when you sign in.$\r$\n$\r$\nStart this one instead?" /SD IDNO IDNO oasis_skip_autostart
  oasis_set_autostart:
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ProjectOASIS" '"$INSTDIR\${MAINBINARYNAME}.exe" --tray'
  oasis_skip_autostart:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ProjectOASIS"
  StrCmp $0 '"$INSTDIR\${MAINBINARYNAME}.exe" --tray' 0 oasis_keep_autostart
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ProjectOASIS"
  oasis_keep_autostart:
!macroend
