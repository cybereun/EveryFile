# EveryFile Phase 1 Core Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a clean-room, installable Windows EveryFile application that indexes user-selected folders, parses supported documents through Kordoc, provides immediate filename and full-text search with detailed filters, shows a right-side document preview, and stores all local data securely.

**Architecture:** A Tauri 2 shell hosts a React 19 user interface. Rust owns filesystem access, Windows integration, encrypted SQLite/FTS5 storage, indexing jobs, and all privileged commands; a bundled Node sidecar provides a newline-delimited JSON parsing protocol around the pinned Kordoc source. The renderer receives only typed DTOs through narrow Tauri commands and never receives unrestricted filesystem or shell access.

**Tech Stack:** Tauri 2.11.4, React 19.2.8, TypeScript 7.0.2, Vite 8.1.5, Vitest 4.1.10, Rust 1.94.1, SQLite/FTS5 with SQLCipher, Windows DPAPI/Credential Manager APIs, Kordoc 2.9.0 at commit `31ec46a0a55cfa92d37b4a5ad34f4a5de9db4133`, Node.js 24 LTS-compatible parser host, Testing Library, WebdriverIO 9.30.0, and Tauri WebDriver smoke tests.

## Global Constraints

- Work only in `L:\codex-L\EveryFile`; keep `L:\codex-L\Everyfile-copy` frozen.
- Do not copy source code, Git history, license text, or implementation details from the frozen comparison application.
- Reuse only the user-approved icon assets from `L:\codex-L\Everyfile-copy\src-tauri\icons`.
- Application name is `EveryFile`; Windows identifier is `com.cybereun.everyfile`.
- Application copyright is `© 2026 Lebi_Cybereun`.
- Developer is `Lebi_Cybereun`; contact email is `cybereunny@gmail.com`.
- Default UI language is Korean with English translation support.
- The default palette is warm ivory, espresso brown, and terracotta `#B95336`; green is not a primary accent.
- Index only folders explicitly selected by the user.
- Do not forcibly download cloud-only files.
- Core filename and document-body search must work without internet access.
- No telemetry or automatic crash-report upload.
- No console, PowerShell, or helper terminal window may appear in packaged normal use.
- Original documents are read-only in Phase 1.
- Kordoc and all third-party notices remain intact in `THIRD_PARTY_NOTICES.md`.
- Use test-driven development, focused files, and a passing test suite before each commit.
- Use `npm` with the committed `package-lock.json`; use stable, exact dependency versions.
- Phase 1 targets Windows x64 and produces an NSIS installer plus a portable ZIP.

## Scope Boundaries

This plan delivers Phase 1 as an independently testable product. The following approved
features receive their own implementation plans after this foundation passes its
release gate:

1. Phase 2: local embeddings, Smart search, similar documents, version grouping,
   advanced statistics, and smart folders.
2. Phase 3A: PaddleOCR, math OCR, OCR queues, and OCR diagnostics.
3. Phase 3B: Ollama, Gemini, OpenAI, grounded answers, summaries, and credentials.
4. Phase 3C: side-by-side comparison, duplicate analysis, deadline detection, and
   extended export.
5. Phase 4: parity audit, performance hardening, accessibility, and release `v1.0.0`.

## File Structure

```text
EveryFile/
├─ .github/workflows/windows.yml           # Windows CI build and test
├─ docs/plans/                              # Approved product design
├─ docs/superpowers/plans/                  # Executable implementation plans
├─ scripts/
│  ├─ build-parser-sidecar.mjs              # Build and target-name parser binary
│  ├─ build-portable.ps1                    # Create portable ZIP
│  └─ verify-no-console.ps1                 # Inspect packaged executable behavior
├─ sidecar/parser-host/
│  ├─ package.json                          # Parser-host-only dependencies and scripts
│  ├─ src/main.ts                           # NDJSON process loop
│  ├─ src/protocol.ts                       # Runtime-validated request/response types
│  ├─ src/kordoc-adapter.ts                 # Kordoc result normalization
│  └─ test/                                 # Parser protocol and fixture tests
├─ vendor/kordoc/                           # Pinned Git submodule
├─ src/
│  ├─ app/App.tsx                           # Window composition and routing state
│  ├─ app/App.test.tsx                      # App shell behavior
│  ├─ components/                           # Reusable controls
│  ├─ features/folders/                     # Folder sidebar and registration
│  ├─ features/search/                      # Search box, filters, results
│  ├─ features/preview/                     # Text/original preview and toolbar
│  ├─ features/library/                     # Bookmarks and tags
│  ├─ features/settings/                    # Phase 1 settings
│  ├─ lib/ipc.ts                            # Only frontend Tauri invoke wrapper
│  ├─ lib/types.ts                          # Renderer DTOs matching Rust serde types
│  ├─ styles/tokens.css                     # Color, typography, spacing tokens
│  └─ styles/app.css                        # Three-pane layout
├─ src-tauri/
│  ├─ capabilities/default.json             # Minimal renderer capabilities
│  ├─ icons/                                # Approved icon set
│  ├─ migrations/0001_initial.sql            # Encrypted application schema
│  ├─ src/application/commands.rs            # Tauri command boundary
│  ├─ src/domain/models.rs                   # Stable domain and DTO types
│  ├─ src/infrastructure/database.rs         # SQLCipher connection and migration
│  ├─ src/infrastructure/secure_key.rs       # DPAPI key protection
│  ├─ src/folders/discovery.rs               # Safe folder enumeration
│  ├─ src/folders/repository.rs              # Registered-folder persistence
│  ├─ src/indexing/coordinator.rs             # Job state machine
│  ├─ src/indexing/watcher.rs                 # Real-time change events
│  ├─ src/parsing/client.rs                  # Rust-to-sidecar NDJSON client
│  ├─ src/search/query.rs                    # User query parser
│  ├─ src/search/repository.rs               # FTS5 and filename queries
│  ├─ src/library/repository.rs              # Bookmarks and tags
│  ├─ src/state.rs                           # Managed application services
│  ├─ src/lib.rs                             # Tauri setup and command registration
│  └─ tests/                                 # Rust integration tests
├─ tests/e2e/                                # Packaged UI smoke tests
├─ THIRD_PARTY_NOTICES.md                    # Preserved dependency notices
├─ package.json                              # Root scripts and frontend dependencies
└─ README.md                                 # Product, privacy, build, and install guide
```

---

### Task 1: Scaffold the Branded Tauri Application

**Files:**
- Create: `package.json`
- Create: `package-lock.json`
- Create: `vite.config.ts`
- Create: `vitest.config.ts`
- Create: `tsconfig.json`
- Create: `src/app/App.tsx`
- Create: `src/app/App.test.tsx`
- Create: `src/main.tsx`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/icons/*`
- Create: `README.md`
- Create: `.gitignore`

**Interfaces:**
- Produces: Tauri application `com.cybereun.everyfile`, root `App` component, `npm test`, `npm run build`, and `npm run tauri build`.
- Consumes: Approved design document and approved icon files only.

- [ ] **Step 1: Add the failing application identity test**

```tsx
// src/app/App.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("renders the EveryFile product identity", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "EveryFile" })).toBeVisible();
    expect(screen.getByText("파일을 찾는 가장 빠른 방법")).toBeVisible();
  });
});
```

- [ ] **Step 2: Scaffold Tauri and install exact test dependencies**

Run:

```powershell
npm create tauri-app@latest . -- --manager npm --template react-ts --identifier com.cybereun.everyfile --tauri-version 2 --force --yes
npm install --save-exact react@19.2.8 react-dom@19.2.8
npm install --save-dev --save-exact typescript@7.0.2 vite@8.1.5 vitest@4.1.10 @vitejs/plugin-react@6.0.4 @testing-library/react@16.3.2 @testing-library/jest-dom@7.0.0 jsdom@30.0.1
```

Expected: Tauri scaffold exists; `npm test -- --run` fails because `App` does not yet expose the tested copy.

- [ ] **Step 3: Implement the minimal branded shell**

```tsx
// src/app/App.tsx
export function App() {
  return (
    <main>
      <h1>EveryFile</h1>
      <p>파일을 찾는 가장 빠른 방법</p>
    </main>
  );
}
```

Set `productName`, `identifier`, window title, and executable name to `EveryFile` and
`com.cybereun.everyfile` in `src-tauri/tauri.conf.json`. Set the package version to
`0.1.0`.

- [ ] **Step 4: Copy and generate only the approved icon assets**

Run:

```powershell
Copy-Item -LiteralPath 'L:\codex-L\Everyfile-copy\src-tauri\icons\icon.png' -Destination 'src-tauri\icons\icon.png'
Copy-Item -LiteralPath 'L:\codex-L\Everyfile-copy\src-tauri\icons\icon.ico' -Destination 'src-tauri\icons\icon.ico'
npm run tauri icon 'src-tauri\icons\icon.png'
```

Expected: Tauri generates the Windows icon sizes without reading or copying reference
application source files.

- [ ] **Step 5: Run the scaffold verification**

Run:

```powershell
npm test -- --run
npm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

Expected: all commands pass.

- [ ] **Step 6: Create the private GitHub repository and push the clean-room root**

Run:

```powershell
gh repo create cybereun/EveryFile --private --source . --remote origin
git push -u origin main
```

Expected: remote URL is `https://github.com/cybereun/EveryFile`; existing
`cybereun/Everyfile-copy` remains untouched.

- [ ] **Step 7: Commit**

```powershell
git add package.json package-lock.json vite.config.ts vitest.config.ts tsconfig.json src src-tauri README.md .gitignore
git commit -m "feat: scaffold branded EveryFile desktop app"
git push
```

---

### Task 2: Define Domain Contracts and Settings

**Files:**
- Create: `src-tauri/src/domain/mod.rs`
- Create: `src-tauri/src/domain/models.rs`
- Create: `src-tauri/src/settings.rs`
- Create: `src-tauri/src/application/mod.rs`
- Create: `src-tauri/src/application/commands.rs`
- Create: `src-tauri/src/state.rs`
- Create: `src-tauri/tests/domain_contracts.rs`
- Create: `src/lib/types.ts`
- Create: `src/lib/ipc.ts`
- Create: `src/lib/ipc.test.ts`

**Interfaces:**
- Produces:
  - Rust `FolderRecord`, `DocumentRecord`, `SearchRequest`, `SearchResponse`,
    `SearchHit`, `PreviewDocument`, `IndexStatus`, and `AppSettings`.
  - TypeScript equivalents with camelCase JSON names.
  - Tauri wrapper functions `getSettings()`, `saveSettings()`, `listFolders()`,
    `searchDocuments()`, and `getPreview()`.
- Consumes: Tauri `invoke` only through `src/lib/ipc.ts`.

- [ ] **Step 1: Write Rust serialization contract tests**

```rust
// src-tauri/tests/domain_contracts.rs
use everyfile_lib::domain::models::{SearchMode, SearchRequest};

#[test]
fn search_request_serializes_with_camel_case_keys() {
    let request = SearchRequest {
        query: "중간고사".into(),
        mode: SearchMode::Keyword,
        folder_ids: vec!["folder-1".into()],
        extensions: vec!["hwp".into(), "pdf".into()],
        modified_after: None,
        modified_before: None,
        include_filename: true,
        limit: 100,
        offset: 0,
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["mode"], "keyword");
    assert_eq!(value["folderIds"][0], "folder-1");
    assert_eq!(value["includeFilename"], true);
}
```

- [ ] **Step 2: Run the contract test and confirm failure**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test domain_contracts
```

Expected: compilation fails because the domain types do not exist.

- [ ] **Step 3: Implement stable Rust DTOs**

```rust
// src-tauri/src/domain/models.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    Keyword,
    Filename,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub mode: SearchMode,
    pub folder_ids: Vec<String>,
    pub extensions: Vec<String>,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub include_filename: bool,
    pub private_search: bool,
    pub sort: String,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub document_id: String,
    pub file_name: String,
    pub path: String,
    pub extension: String,
    pub size_bytes: u64,
    pub modified_at: String,
    pub snippet: Option<String>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRecord {
    pub id: String,
    pub canonical_path: String,
    pub display_name: String,
    pub document_count: u64,
    pub index_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub total: u64,
    pub elapsed_ms: u64,
    pub applied_filters: Vec<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBlock {
    pub kind: String,
    pub text: String,
    pub level: Option<u8>,
    pub page_number: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDocument {
    pub document_id: String,
    pub file_name: String,
    pub path: String,
    pub extension: String,
    pub markdown: String,
    pub blocks: Vec<PreviewBlock>,
    pub warnings: Vec<String>,
    pub bookmarked: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub job_id: String,
    pub state: String,
    pub total_files: u64,
    pub completed_files: u64,
    pub current_file_name: Option<String>,
    pub error_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub language: String,
    pub theme: String,
    pub history_retention_days: u32,
    pub minimize_to_tray: bool,
    pub start_with_windows: bool,
    pub start_hidden: bool,
    pub max_file_size_bytes: u64,
    pub result_page_size: u32,
}
```

Create `DocumentRecord` with the persisted fields from the `documents` migration and
derive the same camel-case serialization. Implement `Default` for `AppSettings` with
`ko`, `light`, 90 days, all three startup/tray flags false, 200 MB maximum file size,
and 100 results per page. No untyped JSON maps cross the Tauri command boundary.

Create `AppState` with `settings: RwLock<AppSettings>` and a temporary
`database_ready: AtomicBool`. Register `get_settings` and `save_settings` in
`application/commands.rs`; the database-backed services replace the temporary
readiness flag in Task 3.

- [ ] **Step 4: Add matching TypeScript DTOs and typed invoke wrappers**

```ts
// src/lib/ipc.ts
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, PreviewDocument, SearchRequest, SearchResponse } from "./types";

export const getSettings = () => invoke<AppSettings>("get_settings");
export const saveSettings = (settings: AppSettings) =>
  invoke<AppSettings>("save_settings", { settings });
export const searchDocuments = (request: SearchRequest) =>
  invoke<SearchResponse>("search_documents", { request });
export const getPreview = (documentId: string) =>
  invoke<PreviewDocument>("get_preview", { documentId });
```

- [ ] **Step 5: Test default settings**

Use this exact default-settings test:

```rust
assert_eq!(AppSettings::default().language, "ko");
assert_eq!(AppSettings::default().theme, "light");
assert_eq!(AppSettings::default().history_retention_days, 90);
assert!(!AppSettings::default().minimize_to_tray);
assert!(!AppSettings::default().start_with_windows);
```

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml
npm test -- --run
```

Expected: all contract and wrapper tests pass.

- [ ] **Step 6: Commit**

```powershell
git add src/lib src-tauri/src/domain src-tauri/src/settings.rs src-tauri/tests
git commit -m "feat: define EveryFile domain contracts"
git push
```

---

### Task 3: Create the DPAPI-Protected SQLCipher Store

**Files:**
- Create: `src-tauri/migrations/0001_initial.sql`
- Create: `src-tauri/src/infrastructure/mod.rs`
- Create: `src-tauri/src/infrastructure/secure_key.rs`
- Create: `src-tauri/src/infrastructure/database.rs`
- Create: `src-tauri/tests/encrypted_database.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Produces:
  - `SecureKeyStore::load_or_create(app_data_dir: &Path) -> Result<SecretKey>`
  - `Database::open(path: &Path, key: &SecretKey) -> Result<Database>`
  - `Database::migrate() -> Result<()>`
  - `Database::connection() -> MutexGuard<Connection>`
- Consumes: Windows DPAPI on production Windows builds and an injected fixed key in tests.

- [ ] **Step 1: Write the encrypted reopen test**

```rust
// src-tauri/tests/encrypted_database.rs
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use tempfile::tempdir;

#[test]
fn database_reopens_with_the_same_key_and_rejects_a_different_key() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("everyfile.db");
    let key = SecretKey::from_bytes([7_u8; 32]);
    let wrong = SecretKey::from_bytes([9_u8; 32]);

    Database::open(&path, &key).unwrap().migrate().unwrap();
    assert!(Database::open(&path, &key).is_ok());
    assert!(Database::open(&path, &wrong).is_err());
}
```

- [ ] **Step 2: Run the test and verify failure**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test encrypted_database
```

Expected: compilation fails because secure storage and database modules do not exist.

- [ ] **Step 3: Add database dependencies**

Add exact compatible releases selected by `cargo add` and commit the resolved
`Cargo.lock`:

```powershell
cargo add --manifest-path src-tauri\Cargo.toml rusqlite --features bundled-sqlcipher-vendored-openssl
cargo add --manifest-path src-tauri\Cargo.toml rand zeroize thiserror parking_lot
cargo add --manifest-path src-tauri\Cargo.toml windows --features Win32_Security_Cryptography,Win32_System_Memory
cargo add --manifest-path src-tauri\Cargo.toml --dev tempfile
```

- [ ] **Step 4: Implement DPAPI key wrapping**

```rust
// src-tauri/src/infrastructure/secure_key.rs
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; 32]);

impl SecretKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self { Self(bytes) }
    pub fn as_bytes(&self) -> &[u8; 32] { &self.0 }
}
```

`SecureKeyStore::load_or_create` must generate 32 random bytes, protect them with
`CryptProtectData` using the description `EveryFile index key`, write only the
protected blob to `key.dat`, and unprotect that blob with `CryptUnprotectData` on
subsequent launches. Zeroize unprotected buffers after opening the database.

- [ ] **Step 5: Create the initial schema**

```sql
-- src-tauri/migrations/0001_initial.sql
CREATE TABLE IF NOT EXISTS folders (
  id TEXT PRIMARY KEY,
  canonical_path TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY,
  folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
  canonical_path TEXT NOT NULL UNIQUE,
  file_name TEXT NOT NULL,
  extension TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  modified_at TEXT NOT NULL,
  content_hash TEXT,
  parser_kind TEXT,
  parse_state TEXT NOT NULL,
  parse_error_code TEXT,
  indexed_at TEXT
);

CREATE TABLE IF NOT EXISTS document_content (
  document_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
  title TEXT,
  body TEXT NOT NULL,
  markdown TEXT NOT NULL,
  blocks_json TEXT NOT NULL,
  warnings_json TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS document_fts USING fts5(
  document_id UNINDEXED,
  file_name,
  title,
  body,
  tokenize='unicode61'
);

CREATE TABLE IF NOT EXISTS bookmarks (
  document_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
  note TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  color TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS document_tags (
  document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (document_id, tag_id)
);

CREATE TABLE IF NOT EXISTS search_history (
  id TEXT PRIMARY KEY,
  query TEXT NOT NULL,
  mode TEXT NOT NULL,
  filters_json TEXT NOT NULL,
  result_count INTEGER NOT NULL,
  elapsed_ms INTEGER NOT NULL,
  searched_at TEXT NOT NULL,
  private INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS index_jobs (
  id TEXT PRIMARY KEY,
  folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
  state TEXT NOT NULL,
  completed_files INTEGER NOT NULL DEFAULT 0,
  total_files INTEGER NOT NULL DEFAULT 0,
  last_path TEXT,
  updated_at TEXT NOT NULL
);
```

- [ ] **Step 6: Open SQLCipher before applying migrations**

`Database::open` must hex-encode the 32-byte key, execute `PRAGMA key =
"x'<hex>'";`, immediately verify the key with a query against `sqlite_master`, enable
foreign keys, use WAL mode, and apply migrations in one transaction.

- [ ] **Step 7: Verify encrypted storage**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test encrypted_database
cargo test --manifest-path src-tauri\Cargo.toml
```

Expected: the same key reopens the database, a different key fails, and plaintext test
content is not visible in the database byte stream.

- [ ] **Step 8: Commit**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/migrations src-tauri/src/infrastructure src-tauri/tests/encrypted_database.rs
git commit -m "feat: add encrypted local search store"
git push
```

---

### Task 4: Register Folders and Discover Files Safely

**Files:**
- Create: `src-tauri/src/folders/mod.rs`
- Create: `src-tauri/src/folders/repository.rs`
- Create: `src-tauri/src/folders/discovery.rs`
- Create: `src-tauri/tests/folder_discovery.rs`
- Modify: `src-tauri/src/application/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/features/folders/FolderSidebar.tsx`
- Create: `src/features/folders/FolderSidebar.test.tsx`

**Interfaces:**
- Produces:
  - `register_folder(path: PathBuf) -> Result<FolderRecord>`
  - `remove_folder(folder_id: String) -> Result<()>`
  - `list_folders() -> Result<Vec<FolderRecord>>`
  - `discover(folder: &FolderRecord, options: DiscoveryOptions) -> Stream<FileCandidate>`
- Consumes: encrypted `Database` and Tauri dialog-selected path.

- [ ] **Step 1: Write discovery boundary tests**

```rust
#[test]
fn discovery_stays_inside_the_registered_root_and_skips_symlink_cycles() {
    let fixture = FolderFixture::with_file("docs/a.txt", "alpha")
        .with_external_symlink("docs/escape", "../outside");
    let files = discover_all(fixture.root(), DiscoveryOptions::default()).unwrap();
    assert_eq!(files.iter().map(|f| f.relative_path.as_str()).collect::<Vec<_>>(), ["docs/a.txt"]);
}
```

Use table-driven assertions for the remaining boundaries:

```rust
#[test]
fn duplicate_roots_and_excluded_directories_are_handled() {
    let fixture = FolderFixture::new()
        .with_file(".git/config", "ignored")
        .with_file("node_modules/pkg/index.js", "ignored")
        .with_file("visible/report.txt", "indexed");
    let files = discover_all(fixture.root(), DiscoveryOptions::default()).unwrap();
    assert_eq!(relative_paths(&files), ["visible/report.txt"]);
    let repository = test_folder_repository();
    repository.register(fixture.root()).unwrap();
    assert_eq!(
        repository.register(fixture.root()).unwrap_err().code(),
        "FOLDER_ALREADY_REGISTERED"
    );
}

#[test]
fn cloud_placeholder_is_metadata_only() {
    let candidate = windows_placeholder_fixture("cloud/report.pdf");
    assert!(candidate.metadata_only);
    assert_eq!(candidate.relative_path, "cloud/report.pdf");
}
```

- [ ] **Step 2: Confirm tests fail**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery
```

Expected: compilation fails because folder discovery does not exist.

- [ ] **Step 3: Implement safe enumeration**

`discovery.rs` must:

- canonicalize the selected root once;
- compare every candidate's canonical path to that root;
- never follow directory symlinks or junctions;
- skip configured names such as `.git`, `node_modules`, and `$RECYCLE.BIN`;
- yield metadata before reading file bodies;
- mark online-only files as `metadata_only`;
- continue after permission errors and emit a structured warning.

Use `ignore::WalkBuilder` with hidden files allowed, link following disabled, and a
custom filter for exclusions.

- [ ] **Step 4: Implement folder persistence and Tauri commands**

Commands accept only paths returned from the native folder picker. Persist canonical
paths and reject a second registration of the same canonical path. Removing a folder
deletes its index rows but never touches the filesystem.

- [ ] **Step 5: Implement the sidebar**

```tsx
export function FolderSidebar({ folders, onAdd }: Props) {
  return (
    <aside aria-label="등록 폴더">
      <div className="sidebar-heading">
        <h2>등록 폴더</h2>
        <button type="button" onClick={onAdd} aria-label="폴더 추가">+</button>
      </div>
      {folders.map((folder) => (
        <button className="folder-row" key={folder.id} type="button">
          <span>{folder.displayName}</span>
          <span>{folder.documentCount.toLocaleString()}</span>
        </button>
      ))}
    </aside>
  );
}
```

The empty state contains one `폴더 선택` action and states that only selected folders
are indexed.

- [ ] **Step 6: Run backend and UI tests**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test folder_discovery
npm test -- --run src/features/folders/FolderSidebar.test.tsx
```

Expected: all folder boundary and UI tests pass.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/folders src-tauri/src/application src-tauri/src/lib.rs src-tauri/tests/folder_discovery.rs src/features/folders
git commit -m "feat: add safe folder registration and discovery"
git push
```

---

### Task 5: Integrate the Pinned Kordoc Parser Sidecar

**Files:**
- Create: `.gitmodules`
- Create: `vendor/kordoc/` as Git submodule
- Create: `sidecar/parser-host/package.json`
- Create: `sidecar/parser-host/tsconfig.json`
- Create: `sidecar/parser-host/src/protocol.ts`
- Create: `sidecar/parser-host/src/kordoc-adapter.ts`
- Create: `sidecar/parser-host/src/main.ts`
- Create: `sidecar/parser-host/test/protocol.test.ts`
- Create: `scripts/build-parser-sidecar.mjs`
- Create: `src-tauri/src/parsing/mod.rs`
- Create: `src-tauri/src/parsing/client.rs`
- Create: `src-tauri/tests/parser_sidecar.rs`
- Create: `THIRD_PARTY_NOTICES.md`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes:
  - NDJSON `ParseRequest { id, operation: "parse", path, options }`
  - Paths already validated by Rust as belonging to a registered root
- Produces:
  - NDJSON `ParseResponse`
  - Rust `ParsedDocument { title, markdown, plain_text, blocks, metadata, warnings }`
  - Error codes `UNSUPPORTED`, `ENCRYPTED`, `DAMAGED`, `TIMEOUT`, `TOO_LARGE`,
    `IMAGE_BASED_PDF`, and `INTERNAL`

- [ ] **Step 1: Add and pin the Kordoc submodule**

Run:

```powershell
git submodule add https://github.com/cybereun/kordoc--.git vendor/kordoc
git -C vendor/kordoc checkout 31ec46a0a55cfa92d37b4a5ad34f4a5de9db4133
git add .gitmodules vendor/kordoc
```

Expected: `git submodule status` begins with
`31ec46a0a55cfa92d37b4a5ad34f4a5de9db4133`.

- [ ] **Step 2: Write parser protocol tests**

```ts
it("normalizes a successful Kordoc result", async () => {
  const response = await handleRequest({
    id: "req-1",
    operation: "parse",
    path: fixturePath("simple.hwpx"),
    options: { maxBytes: 20_000_000 },
  });
  expect(response).toMatchObject({
    id: "req-1",
    ok: true,
    document: { parserKind: "kordoc", warnings: [] },
  });
  expect(response.document?.plainText).toContain("테스트 문서");
});
```

Include these explicit protocol assertions:

```ts
expect(await handleLine("{bad json")).toMatchObject({
  ok: false,
  error: { code: "INVALID_REQUEST" },
});
expect(await handleLine(JSON.stringify({ id: "x", operation: "erase" }))).toMatchObject({
  ok: false,
  error: { code: "INVALID_REQUEST" },
});
expect((await parseFixture("unsupported.bin")).error?.code).toBe("UNSUPPORTED");
await expect(parseFixture("slow.pdf", { timeoutMs: 10 })).resolves.toMatchObject({
  ok: false,
  error: { code: "TIMEOUT" },
});
```

- [ ] **Step 3: Define the runtime-validated protocol**

```ts
// sidecar/parser-host/src/protocol.ts
import { z } from "zod";

export const ParseRequestSchema = z.object({
  id: z.string().min(1),
  operation: z.literal("parse"),
  path: z.string().min(1),
  options: z.object({
    maxBytes: z.number().int().positive().max(500_000_000),
  }),
});

export type ParseRequest = z.infer<typeof ParseRequestSchema>;
```

The response schema must contain exactly one of `document` or `error`. Do not include
stack traces or environment variables in responses.

- [ ] **Step 4: Normalize Kordoc output**

```ts
// sidecar/parser-host/src/kordoc-adapter.ts
import { parse } from "../../../vendor/kordoc/dist/index.js";

export async function parseWithKordoc(path: string) {
  const result = await parse(path);
  if (!result.success) {
    return { ok: false as const, error: mapKordocError(result.error) };
  }
  return {
    ok: true as const,
    document: {
      parserKind: "kordoc",
      title: result.metadata?.title ?? null,
      markdown: result.markdown,
      plainText: result.blocks.map(blockToText).filter(Boolean).join("\n\n"),
      blocks: result.blocks,
      metadata: result.metadata ?? {},
      warnings: result.warnings ?? [],
    },
  };
}
```

`main.ts` reads one line at a time from stdin, validates it, processes at most three
requests concurrently, writes one JSON response per line, and writes diagnostic detail
only to stderr.

- [ ] **Step 5: Build a self-contained Windows sidecar**

`sidecar/parser-host/package.json` uses `zod@4.4.3`, `tsx@4.23.1`,
`tsup@8.5.1`, and `@yao-pkg/pkg@6.21.0` as exact versions.
`scripts/build-parser-sidecar.mjs` performs:

1. `npm ci` and `npm run build` in `vendor/kordoc`;
2. `npm ci` and `npm run bundle` in `sidecar/parser-host`;
3. `pkg dist/main.cjs --target node22-win-x64 --output everyfile-parser.exe`;
4. `rustc --print host-tuple`;
5. copy to `src-tauri/binaries/everyfile-parser-<target>.exe`.

Configure:

```json
{
  "bundle": {
    "externalBin": ["binaries/everyfile-parser"]
  }
}
```

Only Rust receives `shell:allow-spawn`; the webview receives no generic shell
permission.

- [ ] **Step 6: Implement the Rust NDJSON client**

`ParserClient` starts one persistent sidecar with hidden-window creation flags, writes
requests to stdin, matches responses by request ID, enforces a per-request timeout,
restarts once after an unexpected exit, and converts protocol errors to typed Rust
errors.

- [ ] **Step 7: Preserve license notices**

Create `THIRD_PARTY_NOTICES.md` containing the unmodified Kordoc MIT copyright and
license text from `vendor/kordoc/LICENSE`, followed by the notices from
`vendor/kordoc/NOTICE`. Label EveryFile's own copyright separately.

- [ ] **Step 8: Verify parser behavior and hidden execution**

Run:

```powershell
npm test --prefix sidecar/parser-host -- --run
node scripts/build-parser-sidecar.mjs
cargo test --manifest-path src-tauri\Cargo.toml --test parser_sidecar
```

Expected: HWPX, HWP, PDF, XLSX, and DOCX fixtures produce normalized text; malformed
input returns a typed error; no sidecar console window is visible.

- [ ] **Step 9: Commit**

```powershell
git add .gitmodules vendor/kordoc sidecar scripts/build-parser-sidecar.mjs src-tauri/src/parsing src-tauri/tests/parser_sidecar.rs src-tauri/tauri.conf.json src-tauri/capabilities/default.json THIRD_PARTY_NOTICES.md
git commit -m "feat: integrate pinned Kordoc parser sidecar"
git push
```

---

### Task 6: Build the Resumable Indexing Coordinator and File Watcher

**Files:**
- Create: `src-tauri/src/indexing/mod.rs`
- Create: `src-tauri/src/indexing/coordinator.rs`
- Create: `src-tauri/src/indexing/watcher.rs`
- Create: `src-tauri/tests/indexing_flow.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/application/commands.rs`
- Create: `src/features/folders/IndexStatus.tsx`
- Create: `src/features/folders/IndexStatus.test.tsx`

**Interfaces:**
- Consumes: `FileCandidate`, `ParserClient`, and `Database`.
- Produces:
  - `IndexCoordinator::start(folder_id) -> JobId`
  - `pause(job_id)`, `resume(job_id)`, `cancel(job_id)`
  - `IndexStatus { state, total_files, completed_files, current_path, errors }`
  - Tauri event `index-status://changed`

- [ ] **Step 1: Write the resumability integration test**

```rust
#[tokio::test]
async fn interrupted_job_resumes_without_reparsing_completed_files() {
    let harness = IndexHarness::new().with_files(["a.txt", "b.txt", "c.txt"]);
    let job = harness.start().await;
    harness.wait_until_completed_files(job, 1).await;
    harness.simulate_process_restart().await;
    harness.resume(job).await;
    let status = harness.wait_until_finished(job).await;
    assert_eq!(status.completed_files, 3);
    assert_eq!(harness.parse_count("a.txt"), 1);
}
```

Add the following state assertions to `indexing_flow.rs`:

```rust
#[tokio::test]
async fn cancellation_and_file_failures_are_isolated() {
    let harness = IndexHarness::new()
        .with_valid_file("a.txt")
        .with_damaged_file("broken.pdf")
        .with_valid_file("c.txt");
    let job = harness.start().await;
    let status = harness.wait_until_finished(job).await;
    assert_eq!(status.completed_files, 3);
    assert_eq!(status.error_count, 1);
    assert_eq!(harness.search("a").await.len(), 1);
    assert_eq!(harness.search("c").await.len(), 1);

    let second = harness.start_with_many_files(100).await;
    harness.cancel(second).await;
    assert_eq!(harness.status(second).await.state, JobState::Cancelled);
}

#[tokio::test]
async fn watcher_coalesces_writes_and_reconciles_rename_and_delete() {
    let harness = IndexHarness::new().with_valid_file("old.txt");
    harness.finish_initial_index().await;
    harness.emit_repeated_write_events("old.txt", 5).await;
    assert_eq!(harness.parse_count("old.txt"), 2);
    harness.rename("old.txt", "new.txt").await;
    assert!(harness.document("new.txt").await.is_some());
    harness.delete("new.txt").await;
    assert!(harness.document("new.txt").await.is_none());
}
```

- [ ] **Step 2: Confirm failure**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test indexing_flow
```

Expected: compilation fails because the coordinator does not exist.

- [ ] **Step 3: Implement the persisted job state machine**

Use explicit states:

```rust
pub enum JobState {
    Queued,
    Discovering,
    Parsing,
    Paused,
    Completed,
    Cancelled,
    Failed,
}
```

Persist progress after each completed file. Use a bounded Tokio channel so parsing
cannot consume unbounded memory. Prioritize frontend search commands over background
work by yielding between files and honoring a shared activity limiter.

- [ ] **Step 4: Upsert documents transactionally**

For each file:

1. Upsert metadata with `parse_state = 'parsing'`.
2. Parse outside the database transaction.
3. In one transaction, replace `document_content`, replace the FTS row, update
   `parse_state`, and increment job progress.
4. On failure, record a typed error and continue.

- [ ] **Step 5: Add real-time watcher reconciliation**

Use `notify` with a 500 ms debounce. Coalesce repeated writes by canonical path. Rename
updates the document path when identity can be matched; otherwise perform delete plus
create. A scheduled reconciliation compares stored size and modified time to current
metadata without reading every file body.

- [ ] **Step 6: Render index progress**

`IndexStatus.tsx` shows total, completed, current filename, pause/resume, cancel, and
error count. It never displays a full private path unless the user expands details.

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test indexing_flow
npm test -- --run src/features/folders/IndexStatus.test.tsx
```

Expected: all indexing state, restart, and UI tests pass.

- [ ] **Step 8: Commit**

```powershell
git add src-tauri/src/indexing src-tauri/src/state.rs src-tauri/src/application/commands.rs src-tauri/tests/indexing_flow.rs src/features/folders/IndexStatus*
git commit -m "feat: add resumable document indexing"
git push
```

---

### Task 7: Implement Safe Full-Text and Filename Search

**Files:**
- Create: `src-tauri/src/search/mod.rs`
- Create: `src-tauri/src/search/query.rs`
- Create: `src-tauri/src/search/repository.rs`
- Create: `src-tauri/tests/search_queries.rs`
- Modify: `src-tauri/src/application/commands.rs`
- Modify: `src-tauri/src/domain/models.rs`

**Interfaces:**
- Consumes: `SearchRequest` and encrypted database.
- Produces:
  - `ParsedQuery::parse(input: &str) -> Result<ParsedQuery>`
  - `SearchRepository::search(request: &SearchRequest) -> Result<SearchResponse>`
  - operators `"phrase"`, `-term`, `ext:`, `path:`, `after:`, `before:`, and `~N`

- [ ] **Step 1: Write query parser tests**

```rust
#[test]
fn parses_combined_search_operators_without_sql_fragments() {
    let parsed = ParsedQuery::parse(
        "\"중간 고사\" -정답 ext:hwp,pdf path:교육 after:2026-01-01"
    ).unwrap();
    assert_eq!(parsed.phrases, ["중간 고사"]);
    assert_eq!(parsed.excluded_terms, ["정답"]);
    assert_eq!(parsed.extensions, ["hwp", "pdf"]);
    assert_eq!(parsed.path_terms, ["교육"]);
    assert_eq!(parsed.after.as_deref(), Some("2026-01-01"));
}
```

Add empty query, escaped quotes, invalid date, invalid extension, near search, Korean
tokens, and an input containing SQL metacharacters.

- [ ] **Step 2: Confirm failure**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test search_queries
```

Expected: compilation fails because the search module does not exist.

- [ ] **Step 3: Implement a tokenizer and typed query model**

Never concatenate raw user text into SQL. Parse operators into typed fields. Build the
FTS `MATCH` expression from quoted and escaped tokens and bind every non-FTS filter as
a SQL parameter.

- [ ] **Step 4: Implement filename and FTS repositories**

Filename mode queries normalized `file_name` with escaped `LIKE` patterns. Keyword mode
queries `document_fts`, joins metadata, obtains highlighted snippets, and supports
folder, extension, date, filename-inclusion, sorting, limit, and offset filters.

Return:

```rust
SearchResponse {
    hits,
    total,
    elapsed_ms,
    applied_filters,
    has_more,
}
```

Cap a single page at 200 rows and the default at 100.

- [ ] **Step 5: Record non-private history**

After a successful query, insert one `search_history` row only when
`request.private_search == false`. Do not record empty focus events or cancelled
requests.

- [ ] **Step 6: Run search correctness and injection tests**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test search_queries
cargo test --manifest-path src-tauri\Cargo.toml
```

Expected: exact phrase, exclusion, filters, paging, Korean text, and hostile inputs all
pass without SQL errors.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/search src-tauri/src/application/commands.rs src-tauri/src/domain/models.rs src-tauri/tests/search_queries.rs
git commit -m "feat: add safe filename and full-text search"
git push
```

---

### Task 8: Build the Warm-Ivory Three-Pane Application Shell

**Files:**
- Create: `src/styles/tokens.css`
- Create: `src/styles/app.css`
- Create: `src/components/IconButton.tsx`
- Create: `src/components/ResizablePane.tsx`
- Create: `src/app/Header.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/app/App.test.tsx`
- Create: `src/app/App.accessibility.test.tsx`

**Interfaces:**
- Produces: Header, collapsible left sidebar, central workspace, resizable right preview,
  and bottom status bar.
- Consumes: folder state and selected document ID through React props/state only.

- [ ] **Step 1: Write layout and keyboard tests**

```tsx
it("toggles the sidebar with Ctrl+B and focuses search with slash", async () => {
  render(<App />);
  await userEvent.keyboard("{Control>}b{/Control}");
  expect(screen.getByRole("complementary", { name: "등록 폴더" })).not.toBeVisible();
  await userEvent.keyboard("/");
  expect(screen.getByRole("searchbox")).toHaveFocus();
});
```

- [ ] **Step 2: Define the approved design tokens**

```css
/* src/styles/tokens.css */
:root {
  --color-canvas: #f7f0e4;
  --color-surface: #fffaf1;
  --color-surface-muted: #eee1ca;
  --color-text: #46372d;
  --color-text-muted: #806f60;
  --color-border: #d9c4a4;
  --color-accent: #b95336;
  --color-accent-hover: #9f432c;
  --color-highlight: #f4d29a;
  --radius-sm: 8px;
  --radius-md: 12px;
  --shadow-popover: 0 12px 28px rgb(70 55 45 / 16%);
}
```

Use system Korean fonts first and preserve readable contrast at 100%, 125%, 150%, and
200% UI scales.

- [ ] **Step 3: Implement the three-pane shell**

The left pane defaults to 260 px, the right pane to 38% of usable width, and the center
has a 520 px minimum. Persist pane widths in local settings. When the window is too
narrow, collapse the right preview before hiding search controls.

- [ ] **Step 4: Add header and status bar**

Header controls: Home, Statistics, Add Folder, Settings. Status bar: indexed document
count, folder count, queue state, and app version. All icon-only buttons have Korean
and English accessible names.

- [ ] **Step 5: Run UI and accessibility tests**

Run:

```powershell
npm test -- --run src/app
npm run build
```

Expected: layout, shortcuts, focus behavior, and axe-compatible accessible names pass.

- [ ] **Step 6: Commit**

```powershell
git add src/app src/components src/styles
git commit -m "feat: add EveryFile three-pane shell"
git push
```

---

### Task 9: Implement the Detailed Search Toolbar and Result List

**Files:**
- Create: `src/features/search/SearchWorkspace.tsx`
- Create: `src/features/search/SearchInput.tsx`
- Create: `src/features/search/SearchFilters.tsx`
- Create: `src/features/search/SearchResults.tsx`
- Create: `src/features/search/searchStore.ts`
- Create: `src/features/search/useImmediateSearch.ts`
- Create: `src/features/search/SearchWorkspace.test.tsx`
- Create: `src/features/search/useImmediateSearch.test.ts`
- Modify: `src/app/App.tsx`

**Interfaces:**
- Consumes: `searchDocuments(request)` and emits selected document ID.
- Produces: immediate cancellable search, synchronized chips and query syntax, detailed
  filter state, paging, sorting, and information-dense result selection.

- [ ] **Step 1: Write immediate-search race tests**

```ts
it("keeps only the newest response", async () => {
  const api = deferredSearchApi();
  const { result } = renderHook(() => useImmediateSearch(api.search));
  act(() => result.current.setQuery("중간"));
  act(() => result.current.setQuery("중간고사"));
  api.resolve("중간고사", responseWith("latest.hwp"));
  api.resolve("중간", responseWith("stale.hwp"));
  await waitFor(() => expect(result.current.hits[0].fileName).toBe("latest.hwp"));
});
```

- [ ] **Step 2: Implement debounced cancellation**

Use a 120 ms input debounce. Assign a monotonically increasing request sequence and
discard responses not matching the latest sequence. Cancel the previous Rust search
task when a new request begins.

- [ ] **Step 3: Implement the full visible filter row**

Include:

- Keyword / Filename
- Options: all terms, any term, exact phrase, exclusion, near search
- Sort: relevance, confidence, newest, oldest, name, size
- Multi-select extension popover
- Date presets and custom range
- Folder scope
- Include filename toggle
- Search-within-results input
- Preset button

Filter controls use visible labels rather than color alone. Active filters appear as
removable chips.

- [ ] **Step 4: Synchronize query syntax and filters**

Typing `ext:hwp,pdf` checks HWP and PDF in the extension popover. Selecting a date
range writes the equivalent structured request without rewriting the visible user's
plain-language query. Removing a chip removes the corresponding parsed operator.

- [ ] **Step 5: Implement information-dense results**

Each row shows filename, breadcrumb path, extension badge, modified time, size, and
highlighted excerpt. Separate filename matches and content matches. Arrow keys change
selection; Enter opens the selected source file through a scoped Rust command.

- [ ] **Step 6: Test filter combinations and responsive overflow**

Run:

```powershell
npm test -- --run src/features/search
npm run build
```

Expected: search races, filter mapping, keyboard selection, paging, and narrow-window
overflow tests pass.

- [ ] **Step 7: Commit**

```powershell
git add src/features/search src/app/App.tsx
git commit -m "feat: add detailed immediate search workspace"
git push
```

---

### Task 10: Add Text Preview, PDF Layout Preview, Bookmarks, and Tags

**Files:**
- Create: `src-tauri/src/library/mod.rs`
- Create: `src-tauri/src/library/repository.rs`
- Create: `src-tauri/tests/library_actions.rs`
- Modify: `src-tauri/src/application/commands.rs`
- Create: `src/features/preview/PreviewPanel.tsx`
- Create: `src/features/preview/DocumentTextView.tsx`
- Create: `src/features/preview/PdfLayoutView.tsx`
- Create: `src/features/preview/PreviewToolbar.tsx`
- Create: `src/features/preview/PreviewPanel.test.tsx`
- Create: `src/features/library/BookmarkButton.tsx`
- Create: `src/features/library/TagEditor.tsx`
- Create: `src/features/library/LibraryActions.test.tsx`

**Interfaces:**
- Consumes: `getPreview(documentId)`, parsed blocks, and scoped open/copy/export
  commands.
- Produces:
  - `set_bookmark(document_id, note)`
  - `remove_bookmark(document_id)`
  - `create_tag(name, color)`
  - `set_document_tags(document_id, tag_ids)`
  - Text and Original Layout tabs

- [ ] **Step 1: Write repository and preview tests**

```rust
#[test]
fn bookmark_updates_note_without_duplicating_the_document() {
    let db = test_database();
    seed_document(&db, "doc-1");
    set_bookmark(&db, "doc-1", "첫 메모").unwrap();
    set_bookmark(&db, "doc-1", "수정 메모").unwrap();
    assert_eq!(list_bookmarks(&db).unwrap()[0].note, "수정 메모");
}
```

```tsx
it("finds and moves between matches in document text", async () => {
  render(<DocumentTextView blocks={paragraphs("중간고사 준비 중간고사")} />);
  await userEvent.keyboard("{Control>}f{/Control}");
  await userEvent.type(screen.getByRole("searchbox", { name: "문서 내 찾기" }), "중간고사");
  expect(screen.getByText("1 / 2")).toBeVisible();
});
```

- [ ] **Step 2: Implement library repositories**

Use upsert semantics for bookmarks, enforce case-insensitive unique tag names, validate
tag colors against the approved palette, and cascade records when an indexed document
is removed.

- [ ] **Step 3: Render normalized blocks without raw HTML**

`DocumentTextView` maps structured blocks to React elements. It escapes all source
text, sanitizes link protocols to `http`, `https`, and `mailto`, preserves table
structure, and highlights search terms with `<mark>`.

- [ ] **Step 4: Implement PDF original layout**

Use `pdfjs-dist` in a worker with local assets only. Render one visible page plus a
small prefetch window, with previous/next, zoom, fit width, and full-screen controls.
For non-PDF formats in Phase 1, show the explicit message `이 형식은 문서 텍스트로
미리볼 수 있습니다` and provide `파일 열기`; do not upload for conversion.

- [ ] **Step 5: Implement the approved toolbar**

Show Open File, Find, Bookmark, and More. More contains Open Location, Copy Text, Save
Markdown, Copy Path, and Add Tag. AI buttons remain absent because AI is disabled in
Phase 1.

- [ ] **Step 6: Run preview and library tests**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test library_actions
npm test -- --run src/features/preview src/features/library
```

Expected: text rendering, safe links, find navigation, PDF controls, bookmarks, and
tags pass.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/library src-tauri/src/application/commands.rs src-tauri/tests/library_actions.rs src/features/preview src/features/library
git commit -m "feat: add secure preview and library actions"
git push
```

---

### Task 11: Add Settings, Local History, Statistics, Diagnostics, and Export

**Files:**
- Create: `src/features/settings/SettingsDialog.tsx`
- Create: `src/features/settings/GeneralSettings.tsx`
- Create: `src/features/settings/SearchSettings.tsx`
- Create: `src/features/settings/SystemSettings.tsx`
- Create: `src/features/settings/DiagnosticsSettings.tsx`
- Create: `src/features/statistics/StatisticsDialog.tsx`
- Create: `src/features/statistics/SearchHistoryTab.tsx`
- Create: `src/features/settings/SettingsDialog.test.tsx`
- Create: `src-tauri/src/statistics.rs`
- Create: `src-tauri/src/export.rs`
- Create: `src-tauri/src/diagnostics.rs`
- Create: `src-tauri/tests/statistics_and_export.rs`
- Modify: `src-tauri/src/application/commands.rs`

**Interfaces:**
- Produces:
  - `get_statistics() -> DocumentStatistics`
  - `list_search_history(limit, offset)`
  - `delete_search_history(id)` and `clear_search_history()`
  - `export_results(request, format, destination)`
  - `list_parse_errors()` and `retry_parse(document_id)`
- Consumes: encrypted database and native save dialog paths.

- [ ] **Step 1: Write statistics and private-history tests**

```rust
#[test]
fn private_searches_do_not_change_history_statistics() {
    let harness = SearchHarness::with_documents(3);
    harness.search("기록됨", false).unwrap();
    harness.search("비공개", true).unwrap();
    let stats = harness.statistics().unwrap();
    assert_eq!(stats.total_searches, 1);
    assert_eq!(stats.unique_search_terms, 1);
}
```

Add these concrete export and retention checks:

```rust
#[test]
fn retention_and_exports_preserve_types_and_quoting() {
    let harness = StatisticsHarness::seeded();
    harness.insert_history("old", days_ago(91));
    harness.insert_history("kept", days_ago(89));
    harness.run_retention(90).unwrap();
    assert_eq!(harness.history_terms(), ["kept"]);
    assert_eq!(harness.statistics().by_extension["pdf"], 2);
    assert_eq!(harness.statistics().by_folder["Documents"], 3);

    let csv = harness.export_csv([hit("a,b.pdf", 42)]).unwrap();
    assert!(csv.contains("\"a,b.pdf\""));
    let workbook = harness.export_xlsx([hit("report.pdf", 42)]).unwrap();
    assert_eq!(read_xlsx_number(&workbook, "sizeBytes"), 42.0);
}

#[test]
fn cancelling_native_save_dialog_creates_no_file() {
    let harness = StatisticsHarness::seeded();
    assert_eq!(harness.export_with_cancelled_dialog().unwrap(), ExportOutcome::Cancelled);
    assert!(harness.export_directory_is_empty());
}
```

- [ ] **Step 2: Implement aggregate queries**

Statistics include total documents, indexed documents, total bytes, type distribution,
folder distribution, recently modified documents, largest documents, parse states,
total searches, unique terms, frequent searches, and recent searches. Never write or
read external analytics endpoints.

- [ ] **Step 3: Implement 90-day history retention and Private Search**

Run retention cleanup at startup and once per day. Choices are 30, 90, 365, and
unlimited. Private searches never create a history row.

- [ ] **Step 4: Implement safe exports**

CSV and XLSX exports contain search-result metadata and excerpts. Markdown export
contains the selected parsed document. Validate the destination returned by the native
save dialog and write to a temporary sibling file before atomic rename.

- [ ] **Step 5: Implement settings and statistics dialogs**

Settings tabs in Phase 1 are General, Search, System, and Diagnostics. Include language,
result density, click behavior, dates, included/excluded folders, max file size,
version grouping as a disabled row with an explanatory Phase 2 label, startup,
tray, indexing intensity, log folder, and data reset.

Statistics uses terracotta and brown chart colors, keyboard-readable data tables, and
clickable segments that submit a matching search filter.

- [ ] **Step 6: Implement local-only diagnostics**

Store structured logs under the app data directory, redact registered root prefixes,
retain seven days, and never transmit automatically. The reset command closes the
database, removes only the resolved EveryFile app-data directory, verifies that the
target is inside `%LOCALAPPDATA%\com.cybereun.everyfile`, and restarts with an empty
index.

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --test statistics_and_export
npm test -- --run src/features/settings src/features/statistics
```

Expected: statistics, retention, private search, exports, redaction, and settings tests
pass.

- [ ] **Step 8: Commit**

```powershell
git add src/features/settings src/features/statistics src-tauri/src/statistics.rs src-tauri/src/export.rs src-tauri/src/diagnostics.rs src-tauri/src/application/commands.rs src-tauri/tests/statistics_and_export.rs
git commit -m "feat: add local statistics settings and exports"
git push
```

---

### Task 12: Package, Verify, and Release the Phase 1 Windows Build

**Files:**
- Create: `tests/e2e/phase1.spec.ts`
- Create: `wdio.conf.ts`
- Create: `tests/e2e/support.ts`
- Create: `scripts/build-portable.ps1`
- Create: `scripts/verify-no-console.ps1`
- Create: `.github/workflows/windows.yml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `package.json`
- Modify: `README.md`

**Interfaces:**
- Produces:
  - `EveryFile-Setup-v0.1.0.exe`
  - `EveryFile-Portable-v0.1.0.zip`
  - Windows CI evidence for tests and builds
- Consumes: passing frontend, Rust, sidecar, integration, and privacy tests.

- [ ] **Step 1: Install and configure Tauri WebDriver smoke testing**

Run:

```powershell
npm install --save-dev --save-exact webdriverio@9.30.0 @wdio/cli@9.30.0 @wdio/local-runner@9.30.0 @wdio/mocha-framework@9.30.0
cargo install tauri-driver --locked
```

Create an `e2e` Cargo feature that registers the command
`e2e_register_fixture_folder(path)` only in E2E builds. The command canonicalizes the
path, requires it to be inside `tests\fixtures`, and then calls the same production
folder-registration service. `tests/e2e/support.ts` calls that command through the
Tauri IPC bridge and is not included in normal production builds.

- [ ] **Step 2: Write the packaged-app smoke tests**

```ts
describe("EveryFile Phase 1 packaged app", () => {
  it("indexes a fixture folder and searches parsed content", async () => {
    await registerFixtureLibrary("tests/fixtures/library");
    await browser.$("aria/색인 완료").waitForDisplayed();
    const search = await browser.$("aria/모든 파일에서 검색");
    await search.setValue("중간고사");
    await expect(browser.$("aria/guide.hwpx")).toBeDisplayed();
    await browser.$("aria/guide.hwpx").click();
    await expect(browser.$("aria/문서 미리보기")).toHaveText(
      expect.stringContaining("중간고사"),
    );
  });

  it("private search leaves no history", async () => {
    await browser.$("aria/비공개 검색").click();
    await browser.$("aria/모든 파일에서 검색").setValue("민감한 검색");
    await browser.$("aria/통계").click();
    await browser.$("aria/검색 히스토리").click();
    await expect(browser.$("text=민감한 검색")).not.toExist();
  });
});
```

Add a Rust integration test that installs a local proxy sentinel, performs indexing and
search, and asserts the sentinel received zero HTTP connections. Add a restart test
that records a bookmark, restarts the Tauri process through WebdriverIO service hooks,
reopens the same document, and asserts the toolbar action is `북마크 제거`.

- [ ] **Step 3: Configure the Windows bundle**

Set NSIS as the primary bundle, product name `EveryFile`, version `0.1.0`, publisher
`Lebi_Cybereun`, icon paths, and x64 target. Disable developer tools in release builds.
Keep the normal close action as full exit.

- [ ] **Step 4: Build the portable package**

`scripts/build-portable.ps1` must:

1. resolve `src-tauri\target\release\EveryFile.exe`;
2. create a staging directory under `artifacts\portable\EveryFile`;
3. copy the executable, sidecar, required resources, `THIRD_PARTY_NOTICES.md`, and
   `README.md`;
4. create `EveryFile-Portable-v0.1.0.zip`;
5. fail if any expected resource is absent.

- [ ] **Step 5: Verify no console window**

`scripts/verify-no-console.ps1` launches the packaged executable and parser sidecar
through the app, checks that no new visible window class with a console host title
appears, confirms the main window title is `EveryFile`, and exits the app normally.

- [ ] **Step 6: Add Windows CI**

The workflow runs:

```powershell
npm ci
git submodule update --init --recursive
npm test -- --run
cargo test --manifest-path src-tauri\Cargo.toml
node scripts/build-parser-sidecar.mjs
npm run tauri build
npm run test:e2e
powershell -File scripts/build-portable.ps1
```

Upload installer, portable ZIP, test reports, and SHA-256 checksums as workflow
artifacts. Do not publish a GitHub Release from unreviewed commits.

- [ ] **Step 7: Run the complete local release gate**

Run:

```powershell
npm ci
git submodule update --init --recursive
npm test -- --run
npm run build
cargo fmt --manifest-path src-tauri\Cargo.toml -- --check
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri\Cargo.toml
node scripts/build-parser-sidecar.mjs
npm run tauri build
npm run test:e2e
powershell -ExecutionPolicy Bypass -File scripts/build-portable.ps1
powershell -ExecutionPolicy Bypass -File scripts/verify-no-console.ps1
```

Expected: every command succeeds, installer and portable ZIP exist, the app displays
the correct icon in the executable, window, taskbar, and installer, and normal launch
shows no console.

- [ ] **Step 8: Review privacy and clean-room evidence**

Verify:

- `rg -n "Everyfile-copy" .` finds only design/history references and the approved icon
  provenance note.
- No reference source files exist in the repository.
- Indexing/search smoke tests make no outbound network request.
- Kordoc and dependency notices are complete.
- Reset/uninstall tests do not modify fixture source documents.

- [ ] **Step 9: Commit the Phase 1 release candidate**

```powershell
git add tests wdio.conf.ts scripts .github src-tauri/tauri.conf.json package.json package-lock.json README.md
git commit -m "build: package EveryFile phase 1 for Windows"
git push
```

- [ ] **Step 10: Tag only after the release gate passes**

Run:

```powershell
git tag -a v0.1.0 -m "EveryFile Phase 1 core search"
git push origin v0.1.0
```

Expected: the tag references the tested release candidate commit.

## Phase 1 Completion Checklist

- [ ] New private `cybereun/EveryFile` repository contains only clean-room work.
- [ ] `cybereun/Everyfile-copy` and its local comparison folder remain unchanged.
- [ ] User-selected folders can be added, removed, indexed, paused, resumed, and
      reconciled.
- [ ] Filename search becomes available before body parsing finishes.
- [ ] Kordoc parses the approved core formats through a pinned, hidden sidecar.
- [ ] Keyword, filename, operator, filter, sorting, and paging tests pass.
- [ ] Right preview provides structured text, document find, PDF layout, open, copy,
      Markdown save, bookmark, and tags.
- [ ] Statistics and 90-day local search history work; Private Search records nothing.
- [ ] Database contents are encrypted with a DPAPI-protected key.
- [ ] Cloud-only files are not downloaded.
- [ ] No document data leaves the computer during core indexing and search.
- [ ] NSIS installer and portable ZIP launch without a visible terminal.
- [ ] Installer, executable, window, taskbar, and tray use the approved icon.
- [ ] All test, lint, build, privacy, and packaging commands pass.
