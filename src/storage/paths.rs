use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::WorkspaceId;

const APPLICATION_DIRECTORY: &str = "kodeck";
const GLOBAL_CONFIG_FILE: &str = "config.json";
const WORKSPACES_DIRECTORY: &str = "workspaces";
const SHARED_DIRECTORY: &str = ".kodeck";
const SHARED_WORKSPACE_FILE: &str = "workspace.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    config_directory: PathBuf,
    data_directory: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, AppPathsError> {
        let home = non_empty_environment_value("HOME")
            .or_else(|| non_empty_environment_value("USERPROFILE"));
        Self::resolve(
            home,
            non_empty_environment_value("XDG_CONFIG_HOME"),
            non_empty_environment_value("XDG_DATA_HOME"),
        )
    }

    pub fn resolve(
        home: Option<OsString>,
        config_home: Option<OsString>,
        data_home: Option<OsString>,
    ) -> Result<Self, AppPathsError> {
        let home = home.map(PathBuf::from);
        let config_base = resolve_base_directory("XDG_CONFIG_HOME", config_home, &home, ".config")?;
        let data_base = resolve_base_directory("XDG_DATA_HOME", data_home, &home, ".local/share")?;

        Ok(Self::new(
            config_base.join(APPLICATION_DIRECTORY),
            data_base.join(APPLICATION_DIRECTORY),
        ))
    }

    pub fn new(config_directory: impl Into<PathBuf>, data_directory: impl Into<PathBuf>) -> Self {
        Self {
            config_directory: config_directory.into(),
            data_directory: data_directory.into(),
        }
    }

    pub fn config_directory(&self) -> &Path {
        &self.config_directory
    }

    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    pub fn global_config_file(&self) -> PathBuf {
        self.config_directory.join(GLOBAL_CONFIG_FILE)
    }

    pub fn shared_workspace_file(root: impl AsRef<Path>) -> PathBuf {
        root.as_ref()
            .join(SHARED_DIRECTORY)
            .join(SHARED_WORKSPACE_FILE)
    }

    pub fn workspaces_directory(&self) -> PathBuf {
        self.data_directory.join(WORKSPACES_DIRECTORY)
    }

    pub fn workspace(&self, root: impl Into<PathBuf>, id: WorkspaceId) -> WorkspacePaths {
        WorkspacePaths::new(
            root.into(),
            self.workspaces_directory().join(id.to_string()),
        )
    }

    pub fn ensure_private_directories(&self) -> io::Result<()> {
        create_private_directory(&self.config_directory)?;
        create_private_directory(&self.data_directory)?;
        create_private_directory(&self.workspaces_directory())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePaths {
    root: PathBuf,
    private_directory: PathBuf,
}

impl WorkspacePaths {
    fn new(root: PathBuf, private_directory: PathBuf) -> Self {
        Self {
            root,
            private_directory,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn shared_directory(&self) -> PathBuf {
        self.root.join(SHARED_DIRECTORY)
    }

    pub fn shared_config_file(&self) -> PathBuf {
        AppPaths::shared_workspace_file(&self.root)
    }

    pub fn private_directory(&self) -> &Path {
        &self.private_directory
    }

    pub fn cards_file(&self) -> PathBuf {
        self.private_directory.join("cards.json")
    }

    pub fn state_file(&self) -> PathBuf {
        self.private_directory.join("state.json")
    }

    pub fn cache_file(&self) -> PathBuf {
        self.private_directory.join("cache.json")
    }

    pub fn sync_queue_file(&self) -> PathBuf {
        self.private_directory.join("sync-queue.json")
    }

    pub fn ensure_private_directory(&self) -> io::Result<()> {
        if let Some(workspaces_directory) = self.private_directory.parent() {
            create_private_directory(workspaces_directory)?;
        }
        create_private_directory(&self.private_directory)
    }
}

#[derive(Debug)]
pub enum AppPathsError {
    MissingHomeDirectory,
    RelativeBaseDirectory {
        variable: &'static str,
        path: PathBuf,
    },
}

impl fmt::Display for AppPathsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHomeDirectory => {
                formatter.write_str("HOME or USERPROFILE must be set to an absolute path")
            }
            Self::RelativeBaseDirectory { variable, path } => {
                write!(
                    formatter,
                    "{variable} must be an absolute path, got '{}'",
                    path.display()
                )
            }
        }
    }
}

impl Error for AppPathsError {}

fn resolve_base_directory(
    variable: &'static str,
    override_value: Option<OsString>,
    home: &Option<PathBuf>,
    fallback: &str,
) -> Result<PathBuf, AppPathsError> {
    if let Some(path) = override_value.map(PathBuf::from) {
        return require_absolute(variable, path);
    }

    let home = home
        .as_ref()
        .ok_or(AppPathsError::MissingHomeDirectory)
        .and_then(|path| require_absolute("HOME", path.clone()))?;
    Ok(home.join(fallback))
}

fn require_absolute(variable: &'static str, path: PathBuf) -> Result<PathBuf, AppPathsError> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(AppPathsError::RelativeBaseDirectory { variable, path })
    }
}

fn non_empty_environment_value(variable: &str) -> Option<OsString> {
    env::var_os(variable).filter(|value| !value.is_empty())
}

pub(crate) fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    set_private_directory_permissions(path)
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = env::temp_dir().join(format!("kodeck-paths-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolves_xdg_defaults_from_home() {
        let paths = AppPaths::resolve(Some(OsString::from("/home/marius")), None, None).unwrap();

        assert_eq!(
            paths.config_directory(),
            Path::new("/home/marius/.config/kodeck")
        );
        assert_eq!(
            paths.data_directory(),
            Path::new("/home/marius/.local/share/kodeck")
        );
    }

    #[test]
    fn xdg_overrides_do_not_require_home() {
        let paths = AppPaths::resolve(
            None,
            Some(OsString::from("/config")),
            Some(OsString::from("/data")),
        )
        .unwrap();

        assert_eq!(
            paths.global_config_file(),
            Path::new("/config/kodeck/config.json")
        );
        assert_eq!(
            paths.workspaces_directory(),
            Path::new("/data/kodeck/workspaces")
        );
    }

    #[test]
    fn rejects_relative_base_directories() {
        let error = AppPaths::resolve(
            Some(OsString::from("/home/marius")),
            Some(OsString::from("relative")),
            None,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AppPathsError::RelativeBaseDirectory {
                variable: "XDG_CONFIG_HOME",
                ..
            }
        ));
    }

    #[test]
    fn requires_home_when_xdg_directories_are_not_set() {
        let error = AppPaths::resolve(None, None, None).unwrap_err();

        assert!(matches!(error, AppPathsError::MissingHomeDirectory));
    }

    #[test]
    fn derives_all_workspace_paths_from_the_uuid() {
        let paths = AppPaths::new("/config/kodeck", "/data/kodeck");
        let id = WorkspaceId::from_uuid(
            uuid::Uuid::parse_str("f94d2d12-5acf-4370-bb04-41ec860bb205").unwrap(),
        );
        let workspace = paths.workspace("/projects/oryx", id);

        assert_eq!(
            workspace.shared_config_file(),
            Path::new("/projects/oryx/.kodeck/workspace.json")
        );
        assert_eq!(
            workspace.cards_file(),
            Path::new("/data/kodeck/workspaces/f94d2d12-5acf-4370-bb04-41ec860bb205/cards.json")
        );
        assert_eq!(
            workspace.sync_queue_file(),
            Path::new(
                "/data/kodeck/workspaces/f94d2d12-5acf-4370-bb04-41ec860bb205/sync-queue.json"
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_directories_with_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = TestDirectory::new();
        let paths = AppPaths::new(root.0.join("config"), root.0.join("data"));
        paths.ensure_private_directories().unwrap();
        let workspace = paths.workspace(root.0.join("workspace"), WorkspaceId::generate());
        workspace.ensure_private_directory().unwrap();

        for path in [
            paths.config_directory(),
            paths.data_directory(),
            paths.workspaces_directory().as_path(),
            workspace.private_directory(),
        ] {
            let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "unexpected permissions for {}", path.display());
        }
    }
}
