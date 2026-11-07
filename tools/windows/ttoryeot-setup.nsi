; 또렷 Windows 설치 파일. tools/release-local.sh가 makensis로 만든다.
; makensis -INPUTCHARSET UTF8 -DVERSION=0.3.0 -DVERSION_NUM=0.3.0.0 -DSTAGE=<묶음 폴더> -DOUTFILE=<setup.exe> ttoryeot-setup.nsi
; - 관리자 권한 없이 사용자별로 %LOCALAPPDATA%\Programs\ttoryeot에 설치한다.
; - 사용자 Path에 설치 폴더 하나만 더하고, 제거할 때 그 항목만 뺀다. Path는 NSIS 문자열 길이 제한
;   (1024자) 때문에 NSIS 안에서 읽고 쓰지 않고 path.ps1이 레지스트리 값을 그대로 다룬다.
; - NSIS에 들어 있는 플러그인(nsExec, System)만 쓴다. 서명하지 않는다.

!macro RequireDefine NAME
  !ifndef ${NAME}
    !error "${NAME}을 -D로 넘기세요(VERSION, VERSION_NUM, STAGE, OUTFILE)"
  !endif
!macroend
!insertmacro RequireDefine VERSION
!insertmacro RequireDefine VERSION_NUM
!insertmacro RequireDefine STAGE
!insertmacro RequireDefine OUTFILE

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user
ManifestDPIAware true

!define APP_ID "ttoryeot"
!define APP_NAME "또렷 (ttoryeot)"
!define PUBLISHER "207studio"
!define REPO_URL "https://github.com/207studio-oss/ttoryeot"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"
!define POWERSHELL "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"

Name "${APP_NAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\${APP_ID}"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
ShowInstDetails show
ShowUninstDetails show

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!include "FileFunc.nsh"
!include "WinMessages.nsh"

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_NOAUTOCLOSE
!define MUI_UNFINISHPAGE_NOAUTOCLOSE
!define MUI_FINISHPAGE_TEXT "$(FinishText)"
!define MUI_FINISHPAGE_SHOWREADME "$WINDIR\notepad.exe"
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(ShowReadme)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION ShowReadme
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${STAGE}/LICENSE-AGPL-3.0"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

; 첫 언어가 기본이다. Windows 표시 언어가 영어면 영어 화면이 나온다.
!insertmacro MUI_LANGUAGE "Korean"
!insertmacro MUI_LANGUAGE "English"

LangString FinishText ${LANG_KOREAN} "또렷 ${VERSION}을 설치했습니다.$\r$\n$\r$\n창이 있는 앱이 아니라 명령줄 도구입니다. 새 명령 프롬프트나 PowerShell을 열고 'ttoryeot --help'를 입력하세요. MCP 클라이언트에는 'ttoryeot-mcp'를 등록합니다."
LangString FinishText ${LANG_ENGLISH} "Ttoryeot ${VERSION} is installed.$\r$\n$\r$\nIt is a command-line tool, not a windowed app. Open a new Command Prompt or PowerShell and run 'ttoryeot --help'. Register 'ttoryeot-mcp' in your MCP client."
LangString ShowReadme ${LANG_KOREAN} "README 열기"
LangString ShowReadme ${LANG_ENGLISH} "Open README"
LangString NeedX64 ${LANG_KOREAN} "또렷은 64비트(x64) Windows에서만 동작합니다."
LangString NeedX64 ${LANG_ENGLISH} "Ttoryeot runs on 64-bit (x64) Windows only."
LangString PathAdded ${LANG_KOREAN} "사용자 Path에 설치 폴더를 더했습니다."
LangString PathAdded ${LANG_ENGLISH} "Added the install folder to the user Path."
LangString PathPresent ${LANG_KOREAN} "설치 폴더가 이미 사용자 Path에 있습니다."
LangString PathPresent ${LANG_ENGLISH} "The install folder is already in the user Path."
LangString PathFailed ${LANG_KOREAN} "사용자 Path에 설치 폴더를 더하지 못했습니다. 명령을 쓰려면 다음 폴더를 직접 Path에 넣으세요:$\r$\n$INSTDIR"
LangString PathFailed ${LANG_ENGLISH} "Could not add the install folder to the user Path. Add this folder to Path yourself to use the commands:$\r$\n$INSTDIR"
LangString UnPathFailed ${LANG_KOREAN} "사용자 Path에서 설치 폴더를 빼지 못했습니다. 다음 항목을 직접 지우세요:$\r$\n$INSTDIR"
LangString UnPathFailed ${LANG_ENGLISH} "Could not remove the install folder from the user Path. Remove this entry yourself:$\r$\n$INSTDIR"
LangString UninstallLink ${LANG_KOREAN} "또렷 제거"
LangString UninstallLink ${LANG_ENGLISH} "Uninstall Ttoryeot"

VIProductVersion "${VERSION_NUM}"
VIFileVersion "${VERSION_NUM}"
VIAddVersionKey /LANG=${LANG_KOREAN} "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=${LANG_KOREAN} "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_KOREAN} "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_KOREAN} "FileDescription" "${APP_NAME} ${VERSION} 설치"
VIAddVersionKey /LANG=${LANG_KOREAN} "CompanyName" "${PUBLISHER}"
VIAddVersionKey /LANG=${LANG_KOREAN} "LegalCopyright" "Copyright (C) 2026 ${PUBLISHER}. AGPL-3.0-only."
VIAddVersionKey /LANG=${LANG_ENGLISH} "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "FileDescription" "${APP_NAME} ${VERSION} Setup"
VIAddVersionKey /LANG=${LANG_ENGLISH} "CompanyName" "${PUBLISHER}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "LegalCopyright" "Copyright (C) 2026 ${PUBLISHER}. AGPL-3.0-only."

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP|MB_OK "$(NeedX64)" /SD IDOK
    Abort
  ${EndIf}
FunctionEnd

Function ShowReadme
  Exec '"$WINDIR\notepad.exe" "$INSTDIR\README.md"'
FunctionEnd

; MODE = add | remove. 결과 코드를 스택에 남긴다(0 바꿈, 2 할 일 없음, 그 밖은 실패).
!macro RunPathScript MODE
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File "/oname=path.ps1" "path.ps1"
  System::Call 'Kernel32::SetEnvironmentVariable(t "TTORYEOT_PATH_DIR", t "$INSTDIR")'
  System::Call 'Kernel32::SetEnvironmentVariable(t "TTORYEOT_PATH_PS", t "$PLUGINSDIR\path.ps1")'
  ; path.ps1을 파일로 실행하면 실행 정책에 막힐 수 있어 내용을 읽어 스크립트 블록으로 돌린다.
  nsExec::ExecToLog `"${POWERSHELL}" -NoLogo -NoProfile -NonInteractive -Command "& ([scriptblock]::Create([IO.File]::ReadAllText($$env:TTORYEOT_PATH_PS))) ${MODE}"`
!macroend

Section "ttoryeot" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${STAGE}/ttoryeot.exe"
  File "${STAGE}/ttoryeot-mcp.exe"
  File "${STAGE}/README.md"
  File "${STAGE}/CHANGELOG.md"
  File "${STAGE}/LICENSING.md"
  File "${STAGE}/LICENSE-AGPL-3.0"
  File "${STAGE}/LICENSE-MIT"
  File "${STAGE}/LICENSE-APACHE"
  File "${STAGE}/THIRD_PARTY_NOTICES.md"
  File "${STAGE}/BUILD.txt"
  SetOutPath "$INSTDIR\third-party"
  File "${STAGE}/third-party/dependencies.md"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\$(UninstallLink).lnk" "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\README.lnk" "$WINDIR\notepad.exe" '"$INSTDIR\README.md"'

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_NAME} ${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "${REPO_URL}"
  WriteRegStr HKCU "${UNINST_KEY}" "HelpLink" "${REPO_URL}"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"

  !insertmacro RunPathScript add
  Pop $0
  ${If} $0 == 0
    ; 이 설치가 더한 항목만 제거할 때 뺀다.
    WriteRegDWORD HKCU "${UNINST_KEY}" "PathAdded" 1
    DetailPrint "$(PathAdded)"
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${ElseIf} $0 == 2
    DetailPrint "$(PathPresent)"
  ${Else}
    MessageBox MB_ICONEXCLAMATION|MB_OK "$(PathFailed)" /SD IDOK
  ${EndIf}
  SetOutPath "$INSTDIR"
SectionEnd

Section "Uninstall"
  ReadRegDWORD $1 HKCU "${UNINST_KEY}" "PathAdded"
  ${If} $1 == 1
    !insertmacro RunPathScript remove
    Pop $0
    ${If} $0 == 0
      SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
    ${ElseIf} $0 != 2
      MessageBox MB_ICONEXCLAMATION|MB_OK "$(UnPathFailed)" /SD IDOK
    ${EndIf}
  ${EndIf}
  SetOutPath "$TEMP"

  ; 설치한 파일만 지운다. 사용자가 고른 폴더를 통째로 지우지 않는다.
  Delete "$INSTDIR\ttoryeot.exe"
  Delete "$INSTDIR\ttoryeot-mcp.exe"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\CHANGELOG.md"
  Delete "$INSTDIR\LICENSING.md"
  Delete "$INSTDIR\LICENSE-AGPL-3.0"
  Delete "$INSTDIR\LICENSE-MIT"
  Delete "$INSTDIR\LICENSE-APACHE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
  Delete "$INSTDIR\BUILD.txt"
  Delete "$INSTDIR\third-party\dependencies.md"
  RMDir "$INSTDIR\third-party"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  ; 설치할 때의 표시 언어에 따라 이름이 달랐을 수 있어 둘 다 지운다.
  Delete "$SMPROGRAMS\${APP_NAME}\또렷 제거.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall Ttoryeot.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\README.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"

  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
