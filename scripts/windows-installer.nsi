Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef VERSION_RESOURCE
  !error "VERSION_RESOURCE is required"
!endif
!ifndef PACKAGE_DIR
  !error "PACKAGE_DIR is required"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE is required"
!endif
!ifndef ICON_FILE
  !error "ICON_FILE is required"
!endif

!ifndef INSTALL_KEY
  !define INSTALL_KEY "Software\Orca\Install"
!endif
!ifndef UNINSTALL_KEY
  !define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orca"
!endif
!ifndef SHORTCUT_NAME
  !define SHORTCUT_NAME "Orca"
!endif
!ifndef INSTALL_DIRECTORY
  !define INSTALL_DIRECTORY "$LOCALAPPDATA\Programs\Orca"
!endif
Name "Orca"
VIProductVersion "${VERSION_RESOURCE}"
VIAddVersionKey "ProductName" "Orca"
VIAddVersionKey "FileDescription" "Orca Music Player Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Orca contributors"
OutFile "${OUTPUT_FILE}"
InstallDir "${INSTALL_DIRECTORY}"
InstallDirRegKey HKCU "${INSTALL_KEY}" "Directory"
RequestExecutionLevel user
SetCompressor /SOLID lzma
ManifestDPIAware true
ShowInstDetails show
ShowUninstDetails show
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${PACKAGE_DIR}\LICENSE"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Orca requires Windows x64." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  SetShellVarContext current
  SetRegView 64
  ReadRegStr $0 HKCU "${INSTALL_KEY}" "Directory"
  ${If} $0 != ""
    StrCpy $INSTDIR $0
  ${EndIf}
FunctionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
FunctionEnd

; Refuse upgrades/removal while the executable is locked. Never kill playback.
!macro CheckExecutable PREFIX
Function ${PREFIX}CheckExecutable
  IfFileExists "$INSTDIR\Orca.exe" 0 done
  System::Call 'kernel32::CreateFileW(w "$INSTDIR\Orca.exe", i 0x40000000, i 0, p 0, i 3, i 0, p 0) p .r0'
  ${If} $0 == -1
    MessageBox MB_OK|MB_ICONSTOP "Close Orca, including its tray/background mode, before continuing. Check that the installation folder is writable." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  System::Call 'kernel32::CloseHandle(p r0)'
done:
FunctionEnd
!macroend
!insertmacro CheckExecutable ""
!insertmacro CheckExecutable "un."

Section "Orca"
  Call CheckExecutable
  SetOutPath "$INSTDIR"
  SetOverwrite on
  ClearErrors
  File "${PACKAGE_DIR}\Orca.exe"
  File "${PACKAGE_DIR}\vcruntime140.dll"
  File "${PACKAGE_DIR}\LICENSE"
  File "${PACKAGE_DIR}\README.txt"
  File "${PACKAGE_DIR}\BUILD-INFO.json"
  File "${PACKAGE_DIR}\SHA256SUMS.txt"
  IfErrors failed
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  IfErrors failed
  CreateShortCut "$SMPROGRAMS\${SHORTCUT_NAME}.lnk" "$INSTDIR\Orca.exe" "" "$INSTDIR\Orca.exe" 0
  WriteRegStr HKCU "${INSTALL_KEY}" "Directory" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Orca"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Orca contributors"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\Orca.exe,0"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  IfErrors failed
  Goto done
failed:
  MessageBox MB_OK|MB_ICONSTOP "Orca could not be installed completely. Check folder permissions and free space, then run setup again." /SD IDOK
  SetErrorLevel 1
  Abort
done:
SectionEnd

Section "Uninstall"
  Call un.CheckExecutable
  ClearErrors
  Delete "$INSTDIR\Orca.exe"
  IfErrors failed
  Delete "$INSTDIR\vcruntime140.dll"
  IfErrors failed
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\BUILD-INFO.json"
  Delete "$INSTDIR\SHA256SUMS.txt"
  Delete "$INSTDIR\Uninstall.exe"
  IfErrors failed
  ReadRegStr $0 HKCU "${INSTALL_KEY}" "Directory"
  ${If} $0 == $INSTDIR
    Delete "$SMPROGRAMS\${SHORTCUT_NAME}.lnk"
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
    DeleteRegKey HKCU "${INSTALL_KEY}"
  ${EndIf}
  ; Only remove an empty app directory. Music, profiles and extra files survive.
  RMDir "$INSTDIR"
  Goto done
failed:
  MessageBox MB_OK|MB_ICONSTOP "Orca could not be removed. Close it and retry." /SD IDOK
  SetErrorLevel 1
  Abort
done:
SectionEnd
