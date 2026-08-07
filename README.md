# EveryFile

[![Windows CI](https://github.com/cybereun/EveryFile/actions/workflows/windows.yml/badge.svg)](https://github.com/cybereun/EveryFile/actions/workflows/windows.yml)
[![Latest release](https://img.shields.io/github/v/release/cybereun/EveryFile?display_name=tag&sort=semver)](https://github.com/cybereun/EveryFile/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2B-0078D4?logo=windows&logoColor=white)](https://github.com/cybereun/EveryFile/releases)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-2021-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-7.x-3178C6?logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![License](https://img.shields.io/badge/license-proprietary-6B5840)](LICENSE)

## 한국어

EveryFile은 사용자가 직접 선택한 폴더 안의 파일을 빠르게 찾는 Windows용
로컬 우선 문서 검색 앱입니다. 파일명과 문서 내용을 검색하고, 문서 텍스트와
원본 레이아웃을 미리 보며, 북마크·태그·통계·검색 기록을 PC 안에 보관합니다.

저작권 © 2026 Lebi_Cybereun<br />
개발자: Lebi_Cybereun · 이메일: [cybereunny@gmail.com](mailto:cybereunny@gmail.com)

### 주요 특징

- **로컬 우선 검색**: 등록한 폴더만 색인하며 파일명, 경로, 문서 본문을 검색합니다.
- **상세 검색 조건**: 키워드/파일명 모드, 전체·하나 이상·정확히 일치·제외·관련도순,
  확장자, 날짜, 폴더, 파일명 포함 여부, 결과 내 검색, 정렬, 페이지 이동을 제공합니다.
- **미리보기**: 텍스트, 표, 문서 내 찾기, 이미지 미리보기, PDF 원본 레이아웃을 지원합니다.
  HWP/HWPX/PDF의 원본 레이아웃은 가능한 경우 페이지 형태로 표시합니다.
- **문서 처리**: Kordoc 파서를 이용해 HWP/HWPX, DOC/DOCX, XLS/XLSX, PPT/PPTX,
  ODT/ODS, RTF, PDF, TXT 및 주요 이미지 형식을 처리합니다.
- **OCR**: 설정에서 켤 수 있는 로컬 PaddleOCR입니다. 텍스트가 없는 스캔 PDF와
  JPG·PNG·WebP·BMP·TIFF 이미지의 글자를 색인하며, 일반 텍스트 PDF는 기존 텍스트를 사용합니다.
  수식 OCR은 별도 옵션이며 CPU·메모리·처리 시간이 더 필요합니다.
- **AI 선택 기능**: 기본값은 꺼져 있습니다. Ollama(로컬), Gemini, OpenAI 중 사용자가
  선택한 제공자만 사용하며, 원격 AI 요청 전 문서 전송 동의를 확인합니다.
- **개인 작업 공간**: 북마크, 메모, 태그, 스마트 폴더, 최근 검색, 통계, CSV/Excel/Markdown
  내보내기, 일반/비공개 검색 기록을 제공합니다.
- **안정적인 UI**: 왼쪽 폴더 패널과 오른쪽 미리보기 패널을 독립적으로 열고 닫을 수 있으며,
  너비와 표시 상태를 기억합니다. 색인 작업은 하단 상태바에서 진행률을 표시합니다.
- **자동 업데이트**: 설정 → 진단에서 자동 확인을 켜면 앱 시작 시와 6시간마다 GitHub
  Releases를 확인합니다. 새 버전이 있으면 서명된 설치 안내 창을 표시합니다.

### 개인정보 및 보안

- 색인, 검색, 미리보기, OCR, 통계, 북마크, 태그, 검색 기록은 로컬에서 처리됩니다.
- 선택하지 않은 폴더는 읽지 않으며, 문서·이미지·검색어를 자동으로 외부에 전송하지 않습니다.
- AI는 기본 비활성화입니다. Gemini/OpenAI는 사용자가 기능을 켜고 제공자를 선택한 뒤
  명시적으로 동의한 요청에서만 제한된 문서 문맥을 전송합니다.
- AI 자격 증명은 Windows DPAPI로 보호하고, 로컬 데이터베이스는 SQLCipher로 암호화합니다.
- 클라우드 전용 placeholder 파일을 자동으로 내려받아 색인하지 않습니다.

### 설치 및 실행

1. [최신 릴리즈](https://github.com/cybereun/EveryFile/releases)에서 설치 파일 또는 포터블 ZIP을 받습니다.
2. 일반 설치는 `EveryFile-Setup-v1.0.1.exe`를 실행합니다.
3. 무설치 사용은 `EveryFile-Portable-v1.0.1.zip`을 쓰기 가능한 폴더에 압축 해제한 뒤
   `EveryFile.exe`를 실행합니다.
4. 앱에서 오른쪽 위 폴더 추가 버튼 또는 왼쪽 패널의 **폴더 추가**를 눌러 색인할 폴더를 선택합니다.
5. 하단 상태바가 색인 완료를 표시하면 키워드 또는 파일명으로 검색합니다.

두 배포본 모두 Windows GUI 모드로 실행되므로 별도 터미널 창을 열지 않습니다.
설치 파일은 Authenticode 코드 서명이 없을 수 있어 SmartScreen 경고가 표시될 수 있습니다.
릴리즈의 `SHA256SUMS.txt`로 파일을 확인하세요.

### 개발 환경 및 필요한 도구

- Windows 10/11 x64
- Git 및 Git submodule 지원
- Node.js 24.x 및 npm
- Rust stable(2021 edition), Cargo, MSVC 빌드 도구
- Python 3.x(로컬 OCR sidecar 빌드용)
- Strawberry Perl(내장 SQLCipher/OpenSSL 빌드용)
- Windows WebView2 Runtime(설치 프로그램이 없는 경우 부트스트랩 설치)

```powershell
git clone https://github.com/cybereun/EveryFile.git
cd EveryFile
git submodule update --init --recursive
npm ci
npm test -- --run
npm run build
```

Windows 릴리즈 빌드에는 서명 키와 sidecar 빌드가 필요합니다.
개인 서명 키는 저장소에 커밋하지 말고 `TAURI_SIGNING_PRIVATE_KEY` 및
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 환경 변수 또는 GitHub Actions Secret으로만 사용하세요.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release-windows.ps1
npm run generate:update-manifest
```

릴리즈 게이트는 프론트엔드·Rust 테스트, Kordoc parser, 로컬 OCR sidecar, 설치 파일,
포터블 ZIP, 개인정보 검사, 콘솔 창 검사, 깨끗한 프로필 실행 검사를 수행합니다.
결과물은 `artifacts\release`와 `artifacts\portable\EveryFile`에 생성됩니다.

### 라이선스

EveryFile의 원본 코드, UI, 아이콘, 브랜딩, 문서는 [EveryFile Proprietary License](LICENSE)의
적용을 받습니다. Kordoc, RHWP, PaddleOCR/PaddlePaddle 등 제3자 구성 요소는 각각의 고유
라이선스를 유지하며, 자세한 내용은 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)를 확인하세요.

## English

EveryFile is a local-first Windows desktop application for finding files in
folders that you explicitly choose. It searches filenames, paths, and document
text, previews text/tables and original layouts, and keeps bookmarks, tags,
statistics, and search history on the local PC.

Copyright © 2026 Lebi_Cybereun<br />
Developer: Lebi_Cybereun · Email: [cybereunny@gmail.com](mailto:cybereunny@gmail.com)

### Highlights

- **Local-first indexing**: only explicitly registered folders are indexed; filename, path, and content search stay local.
- **Detailed search**: keyword or filename mode, all/any/exact/exclude matching, relevance sorting, extensions, dates, folders, filename inclusion, result filtering, paging, and saved presets.
- **Preview workspace**: text, tables, in-document find, image preview, and original-layout PDF/HWP/HWPX preview when available.
- **Document parsing**: bundled Kordoc processing for HWP/HWPX, DOC/DOCX, XLS/XLSX, PPT/PPTX, ODT/ODS, RTF, PDF, TXT, and common image formats.
- **Private OCR**: optional local PaddleOCR for text-free scanned PDFs and JPG/PNG/WebP/BMP/TIFF images. Text PDFs reuse their existing text; math OCR is optional and more resource-intensive.
- **Optional AI**: disabled by default. Choose Ollama (local), Gemini, or OpenAI; remote requests require explicit transfer consent and are cancellable.
- **Workspace tools**: bookmarks, notes, tags, smart folders, recent searches, document statistics, CSV/Excel/Markdown export, and normal/private history.
- **Responsive interface**: independently collapsible/resizable folder and preview panels, remembered layout preferences, and a quiet bottom indexing status bar.
- **Signed updates**: startup and six-hour checks can be enabled under Settings → Diagnostics. A signed release-note dialog appears only when a newer GitHub Release is available.

### Privacy and security

Indexing, search, preview, OCR, statistics, bookmarks, tags, and search history
run locally. AI is opt-in; Gemini/OpenAI receive only the bounded context needed
for an explicitly approved request. Credentials are protected with Windows DPAPI,
and the local database uses SQLCipher. EveryFile does not automatically hydrate
cloud-only placeholder files.

### Install and run

Download the [latest release](https://github.com/cybereun/EveryFile/releases).
Run `EveryFile-Setup-v1.0.1.exe` for a per-user installation, or extract
`EveryFile-Portable-v1.0.1.zip` and launch `EveryFile.exe` without installation.
Both distributions use the Windows GUI subsystem and do not open a terminal.
The installer may be unsigned by Authenticode; verify `SHA256SUMS.txt` before running.

### Development requirements

Windows 10/11 x64, Git with submodules, Node.js 24.x/npm, Rust stable with the
MSVC toolchain, Python 3.x for the OCR sidecar, Strawberry Perl for the bundled
SQLCipher/OpenSSL build, and WebView2 Runtime are required for a full build.

```powershell
git clone https://github.com/cybereun/EveryFile.git
cd EveryFile
git submodule update --init --recursive
npm ci
npm test -- --run
npm run build
```

Run `scripts\release-windows.ps1` for the complete Windows release gate, then
`npm run generate:update-manifest` to create the signed Tauri `latest.json`
manifest. Keep the private signing key out of Git and use GitHub Actions Secrets
for published releases.

### License

EveryFile source code, UI, artwork, branding, and documentation are covered by
the [EveryFile Proprietary License](LICENSE). Third-party components retain
their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

### Design acknowledgement / 디자인 참고

Some of EveryFile's UI/UX direction was inspired by the layout and interaction
patterns of [Docufinder (Anything)](https://github.com/chrisryugj/Docufinder).
EveryFile is maintained as a separate implementation and its current
distribution does not bundle Docufinder source code, icons, images, branding,
or documentation. This acknowledgement does not grant rights to Docufinder's
works; please consult the [Docufinder license](https://github.com/chrisryugj/Docufinder/blob/main/LICENSE)
for any use of that project.

EveryFile의 일부 UI/UX 방향은
[Docufinder (Anything)](https://github.com/chrisryugj/Docufinder)의 화면 구성과
상호작용 패턴에서 영감을 받아 별도로 구현했습니다. 현재 EveryFile 배포본에는
Docufinder의 소스 코드, 아이콘, 이미지, 브랜드 또는 문서를 포함하지 않습니다.
이는 Docufinder 저작물에 대한 사용 권한을 부여하는 문구가 아니며, 해당 프로젝트를
사용할 경우 [Docufinder 라이선스](https://github.com/chrisryugj/Docufinder/blob/main/LICENSE)를
확인해야 합니다.
