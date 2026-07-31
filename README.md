# EveryFile

EveryFile is a private-first Windows desktop app for finding anything in the
folders you choose. It searches filenames and document text, previews original
layouts, and keeps bookmarks, tags, statistics, and search history on your PC.

Copyright © 2026 Lebi_Cybereun

Developer: Lebi_Cybereun · Email: cybereunny@gmail.com

## Start using the app

1. Select the pink folder button in the top-right toolbar, or select
   **폴더 추가** in the left panel.
2. Choose a folder. EveryFile indexes only folders that you explicitly add.
3. Wait for the bottom status bar to show that indexing is complete, then search
   by keyword or filename.

The left folder panel and right preview panel can be opened or closed
independently from the header. Their widths and visibility are remembered.

## Search and preview

- Keyword and filename search, exact/all/any term matching, extension, date,
  folder, exclusion, result-within-result, sorting, paging, and saved presets
- Text and original-layout preview with in-document find
- Open file/location, copy text/path, Markdown export, bookmarks, notes, and tags
- Document statistics and normal/private search history controls
- HWP/HWPX and other supported office formats parsed through the bundled Kordoc
  sidecar

## OCR

OCR is optional under **설정 → 검색**. When enabled, EveryFile detects images
and scanned PDFs that do not already contain useful text. JPG, PNG, WebP, BMP,
TIFF, and scanned PDF processing runs locally with PaddleOCR. Normal text PDFs
use their existing text instead of OCR.

Math OCR is a separate option because its models require more CPU, memory, and
processing time. Turning OCR off prevents image text from being indexed, while
filename and path search continue to work.

OCR does not upload documents or images.

## AI and privacy

AI is disabled by default. Enabling it reveals a provider selector:

- **Ollama** sends the selected request only to the user-configured local Ollama
  endpoint.
- **Gemini** and **OpenAI** send the bounded document context needed for the
  selected summary or question only after an explicit remote-transfer consent.

AI requests are cancellable. Provider credentials are protected with Windows
DPAPI; the local SQLCipher database is encrypted. Core folder indexing, search,
OCR, preview, bookmarks, tags, statistics, and history do not require an AI
provider and do not upload documents. EveryFile also avoids automatically
hydrating cloud-only placeholder files.

## Install or run portably

- Run `EveryFile-Setup-v1.0.0.exe` for a normal per-user installation.
- Extract `EveryFile-Portable-v1.0.0.zip` to a writable folder and run
  `EveryFile.exe` without installation.

Both distributions use the Windows GUI subsystem and do not open a terminal
window. These community builds are not code-signed, so Windows SmartScreen may
show a warning. Verify `SHA256SUMS.txt` before running.

## Development and release

Prerequisites: Node.js, Rust, Python, and a complete Perl runtime such as
Strawberry Perl (required by the vendored SQLCipher/OpenSSL build).

```powershell
git submodule update --init --recursive
npm ci
npm test -- --run
npm run build
powershell -ExecutionPolicy Bypass -File scripts\release-windows.ps1
```

The release gate builds and tests the frontend, Rust core, parser and local OCR
sidecars, installer, portable ZIP, privacy checks, icon/no-console checks, and a
clean-profile launch acceptance. Artifacts are written to `artifacts\release`.

## Licenses

EveryFile is distributed under the license in `LICENSE`. Third-party components
retain their own licenses and notices; see `THIRD_PARTY_NOTICES.md` and
`vendor\kordoc\LICENSE`.
