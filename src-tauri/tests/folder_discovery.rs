use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use everyfile_lib::application::commands::register_selected_folder;
use everyfile_lib::folders::discovery::{discover, discover_all, DiscoveryOptions};
use everyfile_lib::folders::repository::FolderRepository;
use everyfile_lib::infrastructure::database::Database;
use everyfile_lib::infrastructure::secure_key::SecretKey;
use tempfile::TempDir;
use zeroize::Zeroizing;

struct FolderFixture {
    temp: TempDir,
    root: PathBuf,
}

impl FolderFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        Self { temp, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn with_file(self, relative_path: &str, content: &str) -> Self {
        let path = self.root.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
        self
    }

    #[cfg(windows)]
    fn with_external_directory_link(self, relative_path: &str) -> Self {
        use std::os::windows::fs::symlink_dir;

        let outside = self.temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("escaped.txt"), "outside").unwrap();

        let link = self.root.join(relative_path);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink_dir(&outside, link).unwrap();
        self
    }

    #[cfg(unix)]
    fn with_external_directory_link(self, relative_path: &str) -> Self {
        use std::os::unix::fs::symlink;

        let outside = self.temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("escaped.txt"), "outside").unwrap();

        let link = self.root.join(relative_path);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&outside, link).unwrap();
        self
    }
}

fn relative_paths(root: &Path, options: DiscoveryOptions) -> Vec<String> {
    discover_all(root, options)
        .unwrap()
        .files
        .into_iter()
        .map(|candidate| candidate.relative_path)
        .collect()
}

fn test_folder_repository() -> (TempDir, Arc<Database>, FolderRepository) {
    let temp = tempfile::tempdir().unwrap();
    let key = SecretKey::from_bytes(Zeroizing::new([17_u8; 32]));
    let database = Arc::new(Database::open(&temp.path().join("folders.db"), &key).unwrap());
    database.migrate().unwrap();
    let repository = FolderRepository::new(Arc::clone(&database));
    (temp, database, repository)
}

#[test]
fn discovery_stays_inside_the_registered_root_and_skips_directory_links() {
    let fixture = FolderFixture::new()
        .with_file("docs/a.txt", "alpha")
        .with_external_directory_link("docs/escape");

    assert_eq!(
        relative_paths(fixture.root(), DiscoveryOptions::default()),
        ["docs/a.txt"]
    );
}

#[test]
fn default_exclusions_skip_only_configured_directory_names() {
    let fixture = FolderFixture::new()
        .with_file(".git/config", "ignored")
        .with_file("node_modules/pkg/index.js", "ignored")
        .with_file("$RECYCLE.BIN/deleted.txt", "ignored")
        .with_file("visible/.hidden-report.txt", "included")
        .with_file("visible/report.txt", "included");

    assert_eq!(
        relative_paths(fixture.root(), DiscoveryOptions::default()),
        ["visible/.hidden-report.txt", "visible/report.txt"]
    );
}

#[test]
fn custom_exclusions_are_applied_by_directory_name() {
    let fixture = FolderFixture::new()
        .with_file("build/output.txt", "ignored")
        .with_file("src/build-notes.txt", "included");
    let options = DiscoveryOptions::default().with_excluded_directory("build");

    assert_eq!(
        relative_paths(fixture.root(), options),
        ["src/build-notes.txt"]
    );
}

#[test]
fn discovery_does_not_apply_repository_ignore_files() {
    let fixture = FolderFixture::new()
        .with_file(".ignore", "hidden-by-rule.txt")
        .with_file("hidden-by-rule.txt", "included")
        .with_file("visible.txt", "included");

    assert_eq!(
        relative_paths(fixture.root(), DiscoveryOptions::default()),
        [".ignore", "hidden-by-rule.txt", "visible.txt"]
    );
}

#[test]
fn duplicate_canonical_roots_are_rejected_with_a_stable_error_code() {
    let fixture = FolderFixture::new();
    let (_database_dir, _database, repository) = test_folder_repository();

    let first = repository.register(fixture.root()).unwrap();
    let error = repository.register(fixture.root()).unwrap_err();

    assert_eq!(error.code(), "FOLDER_ALREADY_REGISTERED");
    assert_eq!(
        first.canonical_path,
        fixture.root().canonicalize().unwrap().to_string_lossy()
    );
}

#[test]
fn cancelled_folder_selection_is_a_successful_no_op() {
    let (_database_dir, _database, repository) = test_folder_repository();

    let result = register_selected_folder(None, &repository).unwrap();

    assert!(result.is_none());
    assert!(repository.list().unwrap().is_empty());
}

#[test]
fn registered_folder_discovery_stream_yields_file_metadata() {
    let fixture = FolderFixture::new().with_file("docs/report.txt", "alpha");
    let (_database_dir, _database, repository) = test_folder_repository();
    let registered = repository.register(fixture.root()).unwrap();

    let stream = discover(&registered, DiscoveryOptions::default()).unwrap();
    assert!(stream.warnings().is_empty());
    let files = stream.collect::<Vec<_>>();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "docs/report.txt");
    assert_eq!(files[0].size_bytes, 5);
    assert!(!files[0].metadata_only);
}

#[test]
fn list_folders_reports_document_counts_and_remove_deletes_only_index_rows() {
    let fixture = FolderFixture::new().with_file("keep-me.txt", "source");
    let (_database_dir, database, repository) = test_folder_repository();
    let registered = repository.register(fixture.root()).unwrap();

    {
        let connection = database.connection();
        connection
            .execute(
                "INSERT INTO documents (
                   id, folder_id, canonical_path, file_name, extension, size_bytes,
                   modified_at, parse_state
                 ) VALUES (
                   'doc-1', ?1, ?2, 'keep-me.txt', 'txt', 6,
                   '2026-07-29T00:00:00Z', 'pending'
                 )",
                rusqlite::params![
                    registered.id,
                    fixture.root().join("keep-me.txt").to_string_lossy()
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO document_fts (document_id, file_name, title, body)
                 VALUES ('doc-1', 'keep-me.txt', NULL, 'source')",
                [],
            )
            .unwrap();
    }

    let listed = repository.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].document_count, 1);

    repository.remove(&registered.id).unwrap();

    assert!(fixture.root().join("keep-me.txt").is_file());
    assert!(repository.list().unwrap().is_empty());
    let connection = database.connection();
    let document_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
        .unwrap();
    let fts_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM document_fts", [], |row| row.get(0))
        .unwrap();
    assert_eq!((document_count, fts_count), (0, 0));
}

#[cfg(windows)]
#[test]
fn offline_or_recall_attributes_are_metadata_only() {
    use everyfile_lib::folders::discovery::is_metadata_only_file_attributes;

    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

    for attributes in [
        FILE_ATTRIBUTE_OFFLINE,
        FILE_ATTRIBUTE_RECALL_ON_OPEN,
        FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
    ] {
        assert!(is_metadata_only_file_attributes(attributes));
    }
    assert!(!is_metadata_only_file_attributes(0));
}

#[test]
fn renderer_capability_does_not_grant_generic_path_operations() {
    let capability: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
    let permissions = capability["permissions"].as_array().unwrap();
    let forbidden_prefixes = ["opener:", "fs:", "shell:", "dialog:"];

    for permission in permissions {
        let permission = permission.as_str().unwrap();
        assert!(
            forbidden_prefixes
                .iter()
                .all(|prefix| !permission.starts_with(prefix)),
            "renderer capability permits a generic path operation: {permission}"
        );
    }
}
