# EveryFile v1.0.2

## Reliable signed release assets

- Rebuilt the Windows installer and portable package from the same source
  revision so direct installs and signed automatic updates use identical code.
- Added release-time hash verification to prevent stale installer assets from
  being published under the same version.
- Displayed Windows extended-length paths such as `\\?\D:\` as `D:\` in the
  folder list and tooltips while retaining the safe internal path.

# EveryFile v1.0.1

## Signed automatic updates

- Added a signed GitHub Releases updater with an optional startup and six-hour
  check in Settings → Diagnostics.
- Added the update notes dialog with deferred installation and passive Windows
  restart behavior.
- Published the updater manifest at the EveryFile Releases `latest.json`
  endpoint; existing installations can update without losing settings or the
  local index.

# EveryFile v1.0.0

EveryFile is a Windows document search workspace by Lebi_Cybereun.

## Included

- Explicit folder registration, resilient local indexing, and detailed
  filename/content search
- Text and original-layout previews with find, open, copy, Markdown, bookmark,
  note, and tag actions
- Independently collapsible and resizable folder and preview panels
- Local PaddleOCR for scanned PDF, JPG, PNG, WebP, BMP, and TIFF, plus an
  optional resource-intensive math OCR mode
- Optional, cancellable Ollama, Gemini, and OpenAI document summary/question
  flows with remote-transfer consent
- Encrypted local storage, statistics, normal/private history, and exports
- Installer and no-install portable package with the EveryFile icon and no
  terminal window

## Privacy

Indexing, search, preview, and OCR run locally. AI is disabled by default.
Remote Gemini/OpenAI requests occur only when the user enables AI, selects the
provider, starts an operation, and accepts the transfer notice.

## Windows notice

The installer is not Authenticode-signed, so Windows SmartScreen may display a
warning. The updater package itself is signed by the Tauri updater key. Verify
the downloadable files with `SHA256SUMS.txt`.
