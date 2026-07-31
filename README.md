# EveryFile

EveryFile is a Windows desktop application for quickly finding documents in
folders you choose. It provides filename and document-text search, detailed
filters, previews, bookmarks, tags, statistics, and local search history.

Copyright © 2026 Lebi_Cybereun.

## Privacy

Core folder indexing and search run on this PC. Documents are not uploaded
during these operations. The database is encrypted and its key is protected by
Windows DPAPI. EveryFile does not automatically download cloud-placeholder
files.

AI features are a separate, opt-in future capability. If enabled, their privacy
depends on the provider selected by the user.

## Install and run

Run `EveryFile-Setup-v1.0.0.exe` and launch EveryFile from the Start menu. The
portable ZIP can be extracted anywhere and started with `EveryFile.exe`.
Neither build opens a terminal window.

Windows may show a SmartScreen warning because local builds are not
code-signed. Verify the accompanying SHA-256 checksum before running.

## Development

```powershell
git submodule update --init --recursive
npm ci
npm test -- --run
npm run build
node scripts/build-parser-sidecar.mjs
npm run tauri build
```

Create the release artifacts:

```powershell
npm run release:windows
```

The installer, portable ZIP, and checksums are written to `artifacts\release`.

## Third-party software

See `THIRD_PARTY_NOTICES.md` and `vendor\kordoc\LICENSE`.
