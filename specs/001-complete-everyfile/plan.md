# Implementation Plan: Complete EveryFile

**Branch**: `codex/phase-1-core-search` | **Date**: 2026-07-31 |
**Spec**: [spec.md](spec.md)

## Summary

Complete the existing clean-room Windows desktop application as vertical,
packaged user journeys. First repair the missing production folder-state
orchestration and add independent panel toggles. Then add local PaddleOCR and
optional local math OCR to indexing, explicit Ollama/Gemini/OpenAI settings and
document actions, and finally run real installer/portable acceptance, privacy,
icon, and no-console gates before replacing the release.

## Technical Context

**Language/Version**: TypeScript 7 / React 19; Rust 2021 on stable 1.94+;
Node 24 sidecars; Python 3.12-compatible OCR build tooling

**Primary Dependencies**: Tauri 2, rusqlite/SQLCipher, Kordoc parser host,
PaddleOCR local CPU runtime, a Rust async HTTP client for explicit AI calls

**Storage**: SQLCipher database, Windows DPAPI-protected database key and AI
credentials, local application/model directories

**Testing**: Vitest/Testing Library, Rust unit/integration tests, packaged
Windows WebDriver tests, PowerShell PE/window/privacy checks

**Target Platform**: Windows 10/11 x64

**Project Type**: Desktop application with two hidden GUI sidecars

**Performance Goals**: filename results during discovery; common interactive
search under one second after indexing; bounded preview/OCR/AI memory

**Constraints**: core path is offline and zero-egress; AI disabled by default;
no automatic cloud-placeholder downloads; no console windows; current machine
requires single-job low-debug Rust test builds due limited page file

**Scale/Scope**: 100,000 indexed documents per reference library, multi-GB local
libraries, one desktop user

## Constitution Check

- **Local privacy**: PASS. OCR is local; network is compiled into a narrow AI
  module and guarded by persisted enablement, selected provider, consent, and
  endpoint policy.
- **Complete journeys**: PASS by plan. Every story ends in packaged-app tests;
  unit-only completion is prohibited.
- **Explicit AI consent**: PASS. No fallback, no controls while disabled,
  provider-specific disclosure and secrets.
- **Safety/ownership**: PASS. Existing identity locks, encrypted DB, bounded
  reads, cancellation, and owned reset are retained and extended.
- **Verified releases**: PASS by gate. Tag/release mutation occurs only after
  installer and portable full-flow tests.

Post-design check: PASS. Contracts include production orchestration, OCR
isolation, AI boundary, and panel persistence. No constitutional exception is
required.

## Project Structure

```text
src/
├── app/                    # shell and production DesktopApp orchestration
├── features/
│   ├── folders/            # folders/index lifecycle
│   ├── preview/            # text/layout/actions/AI
│   ├── search/             # detailed search
│   ├── settings/           # general/search/OCR/AI/system/diagnostics
│   └── statistics/         # statistics/history
└── lib/                    # typed IPC and frontend contracts

src-tauri/
├── src/
│   ├── ai/                 # provider policy, consent, retrieval, streaming
│   ├── ocr/                # local OCR process contract/model management
│   └── ...                 # existing clean-room application/domain/services
├── binaries/               # parser and OCR GUI sidecars
├── resources/ocr/          # pinned local model assets/manifests
└── tests/                  # Rust integration/privacy tests

sidecar/
├── parser-host/            # pinned Kordoc protocol
└── ocr-host/               # pinned local PaddleOCR/math protocol and packager

tests/e2e/                  # packaged Windows journeys
scripts/                    # build, package, privacy, icon/no-console gates
specs/001-complete-everyfile/
```

**Structure Decision**: Extend the existing Tauri/React/Rust repository. Keep
native filesystem/security ownership in Rust, document parsing in the existing
Kordoc sidecar, and resource-heavy OCR in a separate killable local sidecar.
Production frontend orchestration becomes an explicit container rather than
optional test-only props.

## Delivery Phases

1. **P1 production repair**: root orchestration, folder lifecycle, visible
   errors, independent left/right panel toggles, restart persistence.
2. **P1 packaged acceptance**: a real fixture folder journey prevents another
   disconnected release.
3. **Local OCR**: settings, detection, model readiness, OCR sidecar, math option,
   index integration, zero-egress tests.
4. **Explicit AI**: encrypted provider settings, connection test, consent,
   selected-document retrieval, summary/question/cancel UI.
5. **Convergence**: close remaining toolbar/settings/statistics accessibility
   and error-state gaps.
6. **Release**: full low-memory gate, installer/portable clean-profile tests,
   checksums, tag and GitHub Release replacement.

## Complexity Tracking

| Decision | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Separate OCR sidecar | OCR native runtimes/models are large and must be cancellable without destabilizing search | Loading OCR into the desktop process increases crash and memory risk |
| Three AI adapters | User explicitly selects Ollama, Gemini, or OpenAI with no fallback | One compatibility endpoint cannot accurately enforce provider consent and response contracts |
| Packaged E2E harness | The released UI was previously disconnected despite passing unit tests | Unit/component tests cannot prove production IPC wiring |
