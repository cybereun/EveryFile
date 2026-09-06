; Tauri's passive installer runs after the application exits.
; The /R flag relaunches only from the template's .onInstSuccess.
!macro NSIS_HOOK_PREINSTALL
  SetDetailsView show
  !insertmacro MUI_HEADER_TEXT "EveryFile ${VERSION} 업데이트" "기존 아이콘과 검색 기능 유지 · UI 개선 · 완료 후 자동 재실행"
  DetailPrint "EveryFile ${VERSION} 설치를 시작합니다."
  DetailPrint "검색 결과, 미리보기와 필터를 더 읽기 쉽게 정리했습니다."
  DetailPrint "기존 앱/설치 아이콘과 더보기 메뉴 5개를 그대로 유지합니다."
  DetailPrint "설정, 색인, 북마크 등 사용자 데이터는 유지됩니다."
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro MUI_HEADER_TEXT "EveryFile ${VERSION} 설치 완료" "업데이트가 완료되었습니다."
  DetailPrint "EveryFile ${VERSION} 파일 설치가 완료되었습니다."
  ${If} $PassiveMode = 1
    DetailPrint "잠시 후 앱이 자동으로 다시 시작됩니다."
    Sleep 1500
  ${EndIf}
!macroend
