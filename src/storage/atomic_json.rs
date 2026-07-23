use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::paths::create_private_directory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoragePermissions {
    Shared,
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicJsonStore {
    path: PathBuf,
    permissions: StoragePermissions,
}

impl AtomicJsonStore {
    pub fn new(path: impl Into<PathBuf>, permissions: StoragePermissions) -> Self {
        Self {
            path: path.into(),
            permissions,
        }
    }

    pub fn shared(path: impl Into<PathBuf>) -> Self {
        Self::new(path, StoragePermissions::Shared)
    }

    pub fn private(path: impl Into<PathBuf>) -> Self {
        Self::new(path, StoragePermissions::Private)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn read<T: DeserializeOwned>(&self) -> Result<T, JsonStoreError> {
        let content = fs::read(&self.path)
            .map_err(|source| JsonStoreError::io("read JSON file", &self.path, source))?;
        serde_json::from_slice(&content).map_err(|source| JsonStoreError::Deserialize {
            path: self.path.clone(),
            source,
        })
    }

    pub fn write<T: Serialize>(&self, value: &T) -> Result<(), JsonStoreError> {
        self.write_with_before_commit(value, |_| Ok(()))
    }

    fn write_with_before_commit<T, F>(
        &self,
        value: &T,
        before_commit: F,
    ) -> Result<(), JsonStoreError>
    where
        T: Serialize,
        F: FnOnce(&Path) -> io::Result<()>,
    {
        let mut content =
            serde_json::to_vec_pretty(value).map_err(|source| JsonStoreError::Serialize {
                path: self.path.clone(),
                source,
            })?;
        content.push(b'\n');

        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        self.create_parent(parent)?;

        let (mut temporary_file, mut temporary_guard) =
            create_temporary_file(parent, &self.path, self.permissions)?;
        temporary_file.write_all(&content).map_err(|source| {
            JsonStoreError::io("write temporary JSON file", &self.path, source)
        })?;
        temporary_file.flush().map_err(|source| {
            JsonStoreError::io("flush temporary JSON file", &self.path, source)
        })?;
        temporary_file
            .sync_all()
            .map_err(|source| JsonStoreError::io("sync temporary JSON file", &self.path, source))?;
        drop(temporary_file);

        before_commit(temporary_guard.path()).map_err(|source| {
            JsonStoreError::io("prepare atomic JSON commit", &self.path, source)
        })?;

        fs::rename(temporary_guard.path(), &self.path)
            .map_err(|source| JsonStoreError::io("replace JSON file", &self.path, source))?;
        temporary_guard.commit();
        sync_parent_directory(parent, &self.path)?;
        Ok(())
    }

    fn create_parent(&self, parent: &Path) -> Result<(), JsonStoreError> {
        let result = match self.permissions {
            StoragePermissions::Shared => fs::create_dir_all(parent),
            StoragePermissions::Private => create_private_directory(parent),
        };
        result.map_err(|source| JsonStoreError::io("create JSON directory", parent, source))
    }
}

#[derive(Debug)]
pub enum JsonStoreError {
    Serialize {
        path: PathBuf,
        source: serde_json::Error,
    },
    Deserialize {
        path: PathBuf,
        source: serde_json::Error,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl JsonStoreError {
    fn io(operation: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.to_owned(),
            source,
        }
    }
}

impl fmt::Display for JsonStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialize { path, source } => {
                write!(
                    formatter,
                    "failed to serialize JSON for '{}': {source}",
                    path.display()
                )
            }
            Self::Deserialize { path, source } => {
                write!(
                    formatter,
                    "failed to deserialize JSON from '{}': {source}",
                    path.display()
                )
            }
            Self::Io {
                operation,
                path,
                source,
            } => {
                write!(
                    formatter,
                    "failed to {operation} '{}': {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for JsonStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialize { source, .. } | Self::Deserialize { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
        }
    }
}

struct TemporaryFileGuard {
    path: PathBuf,
    committed: bool,
}

impl TemporaryFileGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for TemporaryFileGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn create_temporary_file(
    parent: &Path,
    target: &Path,
    permissions: StoragePermissions,
) -> Result<(File, TemporaryFileGuard), JsonStoreError> {
    let target_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("data.json");

    for _ in 0..16 {
        let temporary_path = parent.join(format!(
            ".{target_name}.{}.tmp",
            uuid::Uuid::new_v4().simple()
        ));
        match open_temporary_file(&temporary_path, permissions) {
            Ok(file) => return Ok((file, TemporaryFileGuard::new(temporary_path))),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(JsonStoreError::io(
                    "create temporary JSON file",
                    target,
                    source,
                ));
            }
        }
    }

    Err(JsonStoreError::io(
        "create temporary JSON file",
        target,
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique temporary file",
        ),
    ))
}

fn open_temporary_file(path: &Path, permissions: StoragePermissions) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        let mode = match permissions {
            StoragePermissions::Shared => 0o666,
            StoragePermissions::Private => 0o600,
        };
        options.mode(mode);
    }

    options.open(path)
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path, target: &Path) -> Result<(), JsonStoreError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| JsonStoreError::io("sync JSON directory", target, source))
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path, _target: &Path) -> Result<(), JsonStoreError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;

    use serde::Serializer;
    use serde::ser::Error as _;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Document {
        revision: u32,
        value: String,
    }

    struct SerializationFailure;

    impl Serialize for SerializationFailure {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(S::Error::custom("injected serialization failure"))
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = env::temp_dir().join(format!("kodeck-json-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn document(revision: u32) -> Document {
        Document {
            revision,
            value: format!("revision-{revision}"),
        }
    }

    #[test]
    fn writes_and_reads_pretty_json() {
        let directory = TestDirectory::new();
        let store = AtomicJsonStore::shared(directory.0.join("nested/document.json"));

        store.write(&document(1)).unwrap();

        assert_eq!(store.read::<Document>().unwrap(), document(1));
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.contains('\n'));
        assert!(raw.ends_with('\n'));
    }

    #[test]
    fn atomically_replaces_an_existing_document() {
        let directory = TestDirectory::new();
        let store = AtomicJsonStore::shared(directory.0.join("document.json"));
        store.write(&document(1)).unwrap();

        store.write(&document(2)).unwrap();

        assert_eq!(store.read::<Document>().unwrap(), document(2));
    }

    #[test]
    fn interrupted_commit_preserves_the_last_valid_document() {
        let directory = TestDirectory::new();
        let store = AtomicJsonStore::shared(directory.0.join("document.json"));
        store.write(&document(1)).unwrap();

        let error = store
            .write_with_before_commit(&document(2), |_| {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "injected interruption",
                ))
            })
            .unwrap_err();

        assert!(error.to_string().contains("injected interruption"));
        assert_eq!(store.read::<Document>().unwrap(), document(1));
        let files: Vec<_> = fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files, vec![std::ffi::OsString::from("document.json")]);
    }

    #[test]
    fn serialization_failure_does_not_touch_the_existing_document() {
        let directory = TestDirectory::new();
        let store = AtomicJsonStore::shared(directory.0.join("document.json"));
        store.write(&document(1)).unwrap();
        let original = fs::read(store.path()).unwrap();

        let error = store.write(&SerializationFailure).unwrap_err();

        assert!(matches!(error, JsonStoreError::Serialize { .. }));
        assert_eq!(fs::read(store.path()).unwrap(), original);
    }

    #[test]
    fn reports_malformed_json_without_modifying_it() {
        let directory = TestDirectory::new();
        let path = directory.0.join("document.json");
        fs::write(&path, b"{invalid").unwrap();
        let store = AtomicJsonStore::shared(&path);

        let error = store.read::<Document>().unwrap_err();

        assert!(matches!(error, JsonStoreError::Deserialize { .. }));
        assert_eq!(fs::read(&path).unwrap(), b"{invalid");
    }

    #[cfg(unix)]
    #[test]
    fn private_store_uses_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new();
        let private_directory = directory.0.join("private/workspace");
        let store = AtomicJsonStore::private(private_directory.join("state.json"));
        store.write(&document(1)).unwrap();

        let directory_mode = fs::metadata(private_directory)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let file_mode = fs::metadata(store.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(directory_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }
}
