use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use kodeck::domain::{
    CardId, CardsFile, ColumnId, LocalCard, RepositoryConfig, RepositoryId, TaskPriority,
};
use kodeck::storage::{AppPaths, AtomicJsonStore};
use kodeck::workspace::{
    DiscoveryOutcome, WorkspaceError, WorkspaceInitializer, WorkspaceManager,
    suggested_workspace_root,
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kodeck-lifecycle-{}", uuid::Uuid::new_v4()));
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
fn initializes_registers_and_repairs_a_workspace_idempotently() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("projects/oryx");
    create_directory(root.join(".git"));
    create_directory(root.join("src"));
    let initializer = WorkspaceInitializer::new(temporary.app_paths());

    let first = initializer.initialize(&root, "Oryx Migration").unwrap();
    let workspace_id = first.config.id;

    assert_eq!(first.config.columns.len(), 5);
    assert_eq!(first.config.repositories.len(), 1);
    assert_eq!(first.config.repositories[0].path, ".");
    assert!(first.paths.shared_config_file().is_file());
    assert!(first.paths.cards_file().is_file());
    assert!(first.paths.state_file().is_file());
    assert!(first.paths.cache_file().is_file());
    assert!(first.paths.sync_queue_file().is_file());

    fs::remove_file(first.paths.state_file()).unwrap();
    let repaired = initializer.initialize(&root, "Ignored rename").unwrap();

    assert_eq!(repaired.config.id, workspace_id);
    assert_eq!(repaired.config.name, "Oryx Migration");
    assert!(repaired.paths.state_file().is_file());
    let global: kodeck::domain::GlobalConfig =
        AtomicJsonStore::private(temporary.app_paths().global_config_file())
            .read()
            .unwrap();
    assert_eq!(global.last_workspace_id, Some(workspace_id));
    assert_eq!(global.known_workspaces.len(), 1);
}

#[test]
fn discovers_from_a_child_and_restores_local_cards_after_restart() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("project");
    let child = root.join("src/deep");
    create_directory(&child);
    create_directory(root.join(".git"));
    let paths = temporary.app_paths();
    let initialized = WorkspaceInitializer::new(paths.clone())
        .initialize(&root, "Project")
        .unwrap();

    let card_id = CardId::generate();
    let column_id = ColumnId::from("todo");
    let mut cards = CardsFile::empty(initialized.config.id);
    cards.cards.push(LocalCard {
        id: card_id,
        title: "Persistent card".to_owned(),
        description: "Survives restart".to_owned(),
        priority: TaskPriority::HIGH,
        column_id: column_id.clone(),
        archived: false,
        archived_by_sync: false,
    });
    cards.ordering = BTreeMap::from([(column_id, vec![card_id])]);
    initialized.private_store().save_cards(&cards).unwrap();
    drop(initialized);

    let discovered = WorkspaceManager::new(paths).discover(&child).unwrap();
    let DiscoveryOutcome::Found(reloaded) = discovered else {
        panic!("workspace was not discovered");
    };

    assert_eq!(reloaded.private_state.cards.cards.len(), 1);
    assert_eq!(
        reloaded.private_state.cards.cards[0].title,
        "Persistent card"
    );
}

#[test]
fn moving_a_workspace_updates_its_registered_path_without_changing_identity() {
    let temporary = TestDirectory::new();
    let original = temporary.0.join("original");
    let moved = temporary.0.join("moved");
    create_directory(original.join("src"));
    let paths = temporary.app_paths();
    let context = WorkspaceInitializer::new(paths.clone())
        .initialize(&original, "Movable")
        .unwrap();
    let workspace_id = context.config.id;
    drop(context);

    fs::rename(&original, &moved).unwrap();
    let outcome = WorkspaceManager::new(paths.clone())
        .discover(&moved.join("src"))
        .unwrap();
    let DiscoveryOutcome::Found(context) = outcome else {
        panic!("moved workspace was not discovered");
    };

    assert_eq!(context.config.id, workspace_id);
    let registry = WorkspaceManager::new(paths).registry().load().unwrap();
    assert_eq!(
        registry.known_workspaces[0].path,
        fs::canonicalize(moved).unwrap()
    );
}

#[test]
fn detects_multiple_repositories_and_warns_for_a_missing_one() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("oryx");
    create_directory(root.join("migration/.git"));
    create_directory(root.join("reconciliation/.git"));
    let paths = temporary.app_paths();
    let mut context = WorkspaceInitializer::new(paths.clone())
        .initialize(&root, "Oryx")
        .unwrap();

    let repository_ids: Vec<_> = context
        .config
        .repositories
        .iter()
        .map(|repository| repository.id.as_str())
        .collect();
    assert_eq!(repository_ids, vec!["migration", "reconciliation"]);

    context.config.repositories.push(RepositoryConfig {
        id: RepositoryId::from("temporarily-missing"),
        path: "./missing".to_owned(),
    });
    context.shared_store().save(&context.config).unwrap();

    let reopened = WorkspaceManager::new(paths).open_root(&root).unwrap();
    assert_eq!(reopened.warnings.len(), 1);
    assert!(reopened.warnings[0].contains("temporarily-missing"));
}

#[test]
fn scans_every_available_repository_and_skips_missing_ones() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("oryx");
    create_directory(root.join("migration/.git"));
    create_directory(root.join("reconciliation/.git"));
    fs::write(root.join("migration/main.rs"), "// TODO: migrate\n").unwrap();
    fs::write(root.join("reconciliation/main.rs"), "// FIXME: reconcile\n").unwrap();
    let paths = temporary.app_paths();
    let context = WorkspaceInitializer::new(paths.clone())
        .initialize(&root, "Oryx")
        .unwrap();

    let findings = kodeck::scan::run_workspace_scan(&context);
    assert_eq!(findings.len(), 2);
    assert_eq!(
        findings
            .iter()
            .map(|finding| finding.repository_id.as_str())
            .collect::<std::collections::HashSet<_>>(),
        std::collections::HashSet::from(["migration", "reconciliation"])
    );
    assert!(findings.iter().all(|finding| finding.path == "main.rs"));

    fs::remove_dir_all(root.join("reconciliation")).unwrap();
    let reopened = WorkspaceManager::new(paths).open_root(&root).unwrap();
    assert_eq!(reopened.warnings.len(), 1);
    assert_eq!(kodeck::scan::run_workspace_scan(&reopened).len(), 1);
}

#[test]
fn a_corrupt_private_file_is_never_replaced_during_repair() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("project");
    create_directory(&root);
    let paths = temporary.app_paths();
    let context = WorkspaceInitializer::new(paths.clone())
        .initialize(&root, "Project")
        .unwrap();
    let cards_path = context.paths.cards_file();
    let corrupt = b"{ definitely not valid JSON";
    fs::write(&cards_path, corrupt).unwrap();
    drop(context);

    let error = WorkspaceInitializer::new(paths)
        .initialize(&root, "Project")
        .unwrap_err();

    assert!(matches!(error, WorkspaceError::Storage(_)));
    assert_eq!(fs::read(cards_path).unwrap(), corrupt);
}

#[test]
fn detects_a_live_copy_with_the_same_workspace_identity() {
    let temporary = TestDirectory::new();
    let first_root = temporary.0.join("first");
    let copied_root = temporary.0.join("copied");
    create_directory(&first_root);
    create_directory(copied_root.join(".kodeck"));
    let paths = temporary.app_paths();
    let first = WorkspaceInitializer::new(paths.clone())
        .initialize(&first_root, "Original")
        .unwrap();
    fs::copy(
        first.paths.shared_config_file(),
        copied_root.join(".kodeck/workspace.json"),
    )
    .unwrap();

    let error = WorkspaceManager::new(paths)
        .open_root(&copied_root)
        .unwrap_err();

    assert!(matches!(error, WorkspaceError::IdentityConflict { .. }));
}

#[test]
fn suggests_the_enclosing_git_root_for_initialization() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("repository");
    let child = root.join("src/module");
    create_directory(root.join(".git"));
    create_directory(&child);

    assert_eq!(
        suggested_workspace_root(&child).unwrap(),
        fs::canonicalize(root).unwrap()
    );
}

#[test]
fn uninitialized_git_repository_is_suggested_even_when_another_workspace_was_last_used() {
    let temporary = TestDirectory::new();
    let known_root = temporary.0.join("known");
    let current_root = temporary.0.join("current");
    let current_child = current_root.join("src");
    create_directory(&known_root);
    create_directory(current_root.join(".git"));
    create_directory(&current_child);
    let paths = temporary.app_paths();
    WorkspaceInitializer::new(paths.clone())
        .initialize(&known_root, "Known")
        .unwrap();

    let outcome = WorkspaceManager::new(paths)
        .discover(&current_child)
        .unwrap();

    let DiscoveryOutcome::NotFound { suggested_root } = outcome else {
        panic!("the current repository should be proposed for initialization");
    };
    assert_eq!(suggested_root, fs::canonicalize(current_root).unwrap());
}

#[test]
fn outside_a_repository_discovery_offers_local_creation_and_known_workspaces() {
    let temporary = TestDirectory::new();
    let first_root = temporary.0.join("first");
    let last_root = temporary.0.join("last");
    let launch_root = temporary.0.join("launch");
    create_directory(&first_root);
    create_directory(&last_root);
    create_directory(&launch_root);
    let paths = temporary.app_paths();
    let first = WorkspaceInitializer::new(paths.clone())
        .initialize(&first_root, "First")
        .unwrap();
    let last = WorkspaceInitializer::new(paths.clone())
        .initialize(&last_root, "Last")
        .unwrap();

    let outcome = WorkspaceManager::new(paths).discover(&launch_root).unwrap();

    let DiscoveryOutcome::SelectionRequired {
        workspaces,
        suggested_root,
    } = outcome
    else {
        panic!("creation and known workspaces should be offered");
    };
    assert_eq!(suggested_root, fs::canonicalize(launch_root).unwrap());
    assert_eq!(
        workspaces
            .iter()
            .map(|workspace| workspace.id)
            .collect::<Vec<_>>(),
        vec![last.config.id, first.config.id]
    );
}

#[test]
fn outside_a_repository_without_available_workspaces_initializes_locally() {
    let temporary = TestDirectory::new();
    let launch_root = temporary.0.join("launch");
    create_directory(&launch_root);

    let outcome = WorkspaceManager::new(temporary.app_paths())
        .discover(&launch_root)
        .unwrap();

    let DiscoveryOutcome::NotFound { suggested_root } = outcome else {
        panic!("the current directory should be proposed for initialization");
    };
    assert_eq!(suggested_root, fs::canonicalize(launch_root).unwrap());
}

#[test]
fn stale_registered_workspaces_are_not_offered() {
    let temporary = TestDirectory::new();
    let stale_root = temporary.0.join("stale");
    let launch_root = temporary.0.join("launch");
    create_directory(&stale_root);
    create_directory(&launch_root);
    let paths = temporary.app_paths();
    let stale = WorkspaceInitializer::new(paths.clone())
        .initialize(&stale_root, "Stale")
        .unwrap();
    fs::remove_file(stale.paths.shared_config_file()).unwrap();

    let outcome = WorkspaceManager::new(paths).discover(&launch_root).unwrap();

    assert!(matches!(outcome, DiscoveryOutcome::NotFound { .. }));
}
