# EveryFile 작업 기록

마지막 갱신: 2026-08-01 KST

## 작업 위치

- 저장소: `L:\\codex-L\\EveryFile\\.worktrees\\phase-1-core-search`
- 브랜치: `codex/phase-1-core-search`
- 비교용 동결 앱: `L:\\codex-L\\Everyfile-copy` (수정하지 않음)

## 구현 완료

- 고정 헤더/푸터와 중앙·오른쪽 독립 스크롤.
- 인덱싱을 메타데이터 우선으로 빠르게 처리하고 문서 파싱은 최대 3개 병렬 실행.
- OCR은 로컬 PaddleOCR sidecar를 사용하며 일반 텍스트 PDF는 OCR을 건너뜀.
- 색인 진행 푸터, 일시정지/취소, 성공·실패 결과 대화상자.
- 인덱싱 폴더의 호버 3점 메뉴: 즐겨찾기, 탐색기 열기, 재인덱싱, 확인 후 제거.
- 검색 필터/히스토리/통계 화면과 문서 유형·연도·폴더별 집계.
- PDF/HWP/HWPX 문서 텍스트 및 원본 레이아웃 미리보기, 검색어 노란색 강조.
- 표 셀 병합(span) 보존과 `@rhwp/core` 로컬 WASM 라이선스 고지.
- 무콘솔 GUI 실행 파일과 앱 아이콘 검증.

## 검증 및 배포 산출물

- `npm run build` 통과.
- 프론트엔드 테스트: 82 passed, 2 skipped.
- Rust 전체 테스트 및 source-open 11/11 통과.
- `cargo clippy --all-targets -- -D warnings` 통과.
- `git diff --check` 통과(개행 변환 경고만 있음).
- `scripts/verify-no-console.ps1` 통과.
- release acceptance: 2 tests passed.
- 설치 파일: `artifacts\\release\\EveryFile-Setup-v1.0.0.exe`
- 포터블: `artifacts\\release\\EveryFile-Portable-v1.0.0.zip`
- 체크섬: `artifacts\\release\\SHA256SUMS.txt`

## 남은 단계

1. 변경 사항을 커밋한다.
2. `origin`의 `codex/phase-1-core-search` 브랜치에 push한다.
3. GitHub `v1.0.0` 릴리즈에 설치 파일·포터블 ZIP·SHA256SUMS를 업로드한다.
4. 완료 후 T023을 체크하고 최종 경로/링크를 보고한다.

## 주의

- `Everyfile-copy`는 참고용으로 동결되어 있다.
- 폴더 색인과 HWP/PDF/OCR 처리는 외부 서버로 파일을 보내지 않는다. 사용자가 설정한 외부 AI provider를 선택한 경우에만 해당 AI 요청이 전송된다.
