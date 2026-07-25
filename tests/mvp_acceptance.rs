use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kodeck::domain::{
    CardId, CardsFile, ColumnId, LocalCard, ProviderConfig, ProviderId, RepositoryConfig,
    RepositoryId, TaskPriority,
};
use kodeck::storage::AppPaths;
use kodeck::workspace::{DiscoveryOutcome, WorkspaceInitializer, WorkspaceManager};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("kodeck-mvp-acceptance-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn app_paths(&self) -> AppPaths {
        AppPaths::new(self.0.join("config"), self.0.join("data"))
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn create_directory(path: impl AsRef<Path>) {
    fs::create_dir_all(path).unwrap();
}

#[test]
fn workspace_mvp_keeps_shared_identity_and_private_state_across_its_lifecycle() {
    let temporary = TestDirectory::new();
    let original_root = temporary.0.join("projects/oryx");
    create_directory(original_root.join("migration/.git"));
    create_directory(original_root.join("migration/src/deep"));
    create_directory(original_root.join("reconciliation/.git"));
    let app_paths = temporary.app_paths();

    let mut initialized = WorkspaceInitializer::new(app_paths.clone())
        .initialize(&original_root, "Oryx Migration")
        .unwrap();
    let workspace_id = initialized.config.id;
    assert_eq!(initialized.config.repositories.len(), 2);

    initialized.config.providers.push(ProviderConfig::Gitlab {
        id: ProviderId::from("main-gitlab"),
        instance_url: "https://gitlab.example.com".to_owned(),
        projects: vec!["data/oryx-migration".to_owned()],
    });
    initialized.config.repositories.push(RepositoryConfig {
        id: RepositoryId::from("temporarily-missing"),
        path: "./missing".to_owned(),
    });
    initialized
        .shared_store()
        .save(&initialized.config)
        .unwrap();

    let card_id = CardId::generate();
    let todo = ColumnId::from("todo");
    let mut cards = CardsFile::empty(workspace_id);
    cards.cards.push(LocalCard {
        id: card_id,
        title: "Persist me".to_owned(),
        description: "Private workspace data".to_owned(),
        priority: TaskPriority::MEDIUM,
        column_id: todo.clone(),
        archived: false,
    });
    cards.ordering = BTreeMap::from([(todo.clone(), vec![card_id])]);
    initialized.private_store().save_cards(&cards).unwrap();

    let mut ui_state = initialized.private_state.state.clone();
    ui_state.selected_column_id = Some(todo);
    ui_state.selected_card_id = Some(card_id);
    initialized
        .private_store()
        .save_ui_state(&ui_state)
        .unwrap();

    let shared_file = initialized.paths.shared_config_file();
    let private_directory = initialized.paths.private_directory().to_owned();
    assert!(!private_directory.starts_with(&original_root));
    let shared_json = fs::read_to_string(&shared_file).unwrap();
    assert!(!shared_json.to_ascii_lowercase().contains("token"));
    assert!(!shared_json.contains("Persist me"));
    let committed_files: Vec<_> = fs::read_dir(original_root.join(".kodeck"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        committed_files,
        vec![std::ffi::OsString::from("workspace.json")]
    );
    drop(initialized);

    let child = original_root.join("migration/src/deep");
    let DiscoveryOutcome::Found(restarted) = WorkspaceManager::new(app_paths.clone())
        .discover(&child)
        .unwrap()
    else {
        panic!("workspace was not discovered from a child directory");
    };
    assert_eq!(restarted.config.id, workspace_id);
    assert_eq!(restarted.private_state.cards.cards[0].id, card_id);
    assert_eq!(
        restarted.private_state.state.selected_card_id,
        Some(card_id)
    );
    assert!(
        restarted
            .warnings
            .iter()
            .any(|warning| warning.contains("temporarily-missing"))
    );
    drop(restarted);

    let moved_root = temporary.0.join("archive/oryx");
    create_directory(moved_root.parent().unwrap());
    fs::rename(&original_root, &moved_root).unwrap();
    let DiscoveryOutcome::Found(moved) = WorkspaceManager::new(app_paths.clone())
        .discover(&moved_root.join("migration/src/deep"))
        .unwrap()
    else {
        panic!("moved workspace was not discovered");
    };
    assert_eq!(moved.config.id, workspace_id);
    assert_eq!(moved.private_state.cards.cards[0].id, card_id);

    let registry = WorkspaceManager::new(app_paths).registry().load().unwrap();
    let registered = registry
        .known_workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
        .unwrap();
    assert_eq!(registered.path, fs::canonicalize(moved_root).unwrap());
}
