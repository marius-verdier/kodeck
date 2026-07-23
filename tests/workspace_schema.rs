use kodeck::domain::{
    ColumnConfig, ColumnId, ProviderConfig, ProviderId, RepositoryConfig, RepositoryId,
    StatusMapping, WORKSPACE_SCHEMA_VERSION, WorkspaceConfig, WorkspaceId,
};
use uuid::Uuid;

const WORKSPACE_ID: &str = "f94d2d12-5acf-4370-bb04-41ec860bb205";

fn valid_workspace() -> WorkspaceConfig {
    WorkspaceConfig {
        schema_version: WORKSPACE_SCHEMA_VERSION,
        id: WorkspaceId::from_uuid(Uuid::parse_str(WORKSPACE_ID).unwrap()),
        name: "Oryx Migration".to_owned(),
        repositories: vec![RepositoryConfig {
            id: RepositoryId::from("migration"),
            path: "./migration".to_owned(),
        }],
        columns: ColumnConfig::defaults(),
        providers: vec![ProviderConfig::Gitlab {
            id: ProviderId::from("main-gitlab"),
            instance_url: "https://gitlab.example.com".to_owned(),
            projects: vec!["data/oryx-migration".to_owned()],
        }],
        status_mappings: vec![StatusMapping {
            provider_id: ProviderId::from("main-gitlab"),
            provider_status: "workflow::todo".to_owned(),
            column_id: ColumnId::from("todo"),
        }],
    }
}

#[test]
fn deserializes_the_workspace_specification_example() {
    let json = format!(
        r#"{{
            "schema_version": 1,
            "id": "{WORKSPACE_ID}",
            "name": "Oryx Migration",
            "repositories": [{{ "id": "migration", "path": "./migration" }}],
            "columns": [
                {{ "id": "inbox", "name": "Inbox" }},
                {{ "id": "todo", "name": "To do" }},
                {{ "id": "in-progress", "name": "In progress" }},
                {{ "id": "review", "name": "Review" }},
                {{ "id": "done", "name": "Done" }}
            ],
            "providers": [{{
                "id": "main-gitlab",
                "type": "gitlab",
                "instance_url": "https://gitlab.example.com",
                "projects": ["data/oryx-migration"]
            }}]
        }}"#
    );

    let workspace: WorkspaceConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(workspace.status_mappings, Vec::new());
    assert_eq!(workspace.id.to_string(), WORKSPACE_ID);
    assert!(workspace.validate().is_ok());
}

#[test]
fn round_trips_the_complete_v1_schema() {
    let workspace = valid_workspace();

    let serialized = serde_json::to_string_pretty(&workspace).unwrap();
    let deserialized: WorkspaceConfig = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized, workspace);
    assert!(deserialized.validate().is_ok());
}

#[test]
fn rejects_an_invalid_workspace_uuid_during_deserialization() {
    let json = r#"{
        "schema_version": 1,
        "id": "not-a-uuid",
        "name": "Oryx Migration",
        "columns": [{ "id": "todo", "name": "To do" }]
    }"#;

    assert!(serde_json::from_str::<WorkspaceConfig>(json).is_err());
}

#[test]
fn generates_non_nil_unique_workspace_ids() {
    let first = WorkspaceId::generate();
    let second = WorkspaceId::generate();

    assert!(!first.as_uuid().is_nil());
    assert!(!second.as_uuid().is_nil());
    assert_ne!(first, second);
}

#[test]
fn constructor_uses_schema_version_and_default_columns() {
    let workspace = WorkspaceConfig::new(
        WorkspaceId::from_uuid(Uuid::parse_str(WORKSPACE_ID).unwrap()),
        "Oryx Migration",
    );

    let columns: Vec<_> = workspace
        .columns
        .iter()
        .map(|column| (column.id.as_str(), column.name.as_str()))
        .collect();

    assert_eq!(workspace.schema_version, 1);
    assert_eq!(
        columns,
        vec![
            ("inbox", "Inbox"),
            ("todo", "To do"),
            ("in-progress", "In progress"),
            ("review", "Review"),
            ("done", "Done"),
        ]
    );
}

#[test]
fn rejects_unknown_fields_including_tokens() {
    let json = format!(
        r#"{{
            "schema_version": 1,
            "id": "{WORKSPACE_ID}",
            "name": "Oryx Migration",
            "columns": [{{ "id": "todo", "name": "To do" }}],
            "providers": [{{
                "id": "main-gitlab",
                "type": "gitlab",
                "instance_url": "https://gitlab.example.com",
                "token": "must-not-be-stored"
            }}]
        }}"#
    );

    let error = serde_json::from_str::<WorkspaceConfig>(&json).unwrap_err();

    assert!(error.to_string().contains("unknown field `token`"));
}

#[test]
fn reports_all_top_level_validation_errors() {
    let mut workspace = valid_workspace();
    workspace.schema_version = 2;
    workspace.id = WorkspaceId::from_uuid(Uuid::nil());
    workspace.name = " ".to_owned();
    workspace.columns.clear();

    let errors = workspace.validate().unwrap_err();
    let paths: Vec<_> = errors
        .issues()
        .iter()
        .map(|issue| issue.path.as_str())
        .collect();

    assert!(paths.contains(&"schema_version"));
    assert!(paths.contains(&"id"));
    assert!(paths.contains(&"name"));
    assert!(paths.contains(&"columns"));
}

#[test]
fn validates_repository_ids_and_lexical_paths_without_accessing_disk() {
    let mut workspace = valid_workspace();
    workspace.repositories = vec![
        RepositoryConfig {
            id: RepositoryId::from("migration"),
            path: "./migration".to_owned(),
        },
        RepositoryConfig {
            id: RepositoryId::from("migration"),
            path: "migration".to_owned(),
        },
        RepositoryConfig {
            id: RepositoryId::from("outside"),
            path: "../outside".to_owned(),
        },
        RepositoryConfig {
            id: RepositoryId::from("absolute"),
            path: "/tmp/project".to_owned(),
        },
    ];

    let errors = workspace.validate().unwrap_err();
    let messages: Vec<_> = errors
        .issues()
        .iter()
        .map(|issue| issue.message.as_str())
        .collect();

    assert!(
        messages
            .iter()
            .any(|message| message.contains("duplicate repository id"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("duplicate repository path"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("must not escape"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("must be relative"))
    );
}

#[test]
fn validates_provider_configuration_and_status_mapping_references() {
    let mut workspace = valid_workspace();
    workspace.providers = vec![ProviderConfig::Gitlab {
        id: ProviderId::from("main-gitlab"),
        instance_url: "https://token@gitlab.example.com?secret=value".to_owned(),
        projects: vec!["data/oryx".to_owned(), "data/oryx".to_owned()],
    }];
    workspace.status_mappings = vec![StatusMapping {
        provider_id: ProviderId::from("unknown-provider"),
        provider_status: "workflow::todo".to_owned(),
        column_id: ColumnId::from("unknown-column"),
    }];

    let errors = workspace.validate().unwrap_err();
    let paths: Vec<_> = errors
        .issues()
        .iter()
        .map(|issue| issue.path.as_str())
        .collect();

    assert!(paths.contains(&"providers[0].instance_url"));
    assert!(paths.contains(&"providers[0].projects[1]"));
    assert!(paths.contains(&"status_mappings[0].provider_id"));
    assert!(paths.contains(&"status_mappings[0].column_id"));
}

#[test]
fn rejects_invalid_ids_and_duplicate_shared_configuration() {
    let mut workspace = valid_workspace();
    workspace.repositories[0].id = RepositoryId::from("invalid id");
    workspace.columns.push(ColumnConfig {
        id: ColumnId::from("todo"),
        name: "Duplicate todo".to_owned(),
    });
    workspace.providers.push(ProviderConfig::Gitlab {
        id: ProviderId::from("main-gitlab"),
        instance_url: "https://gitlab.example.com".to_owned(),
        projects: Vec::new(),
    });
    workspace.status_mappings.push(StatusMapping {
        provider_id: ProviderId::from("main-gitlab"),
        provider_status: "workflow::todo".to_owned(),
        column_id: ColumnId::from("inbox"),
    });

    let errors = workspace.validate().unwrap_err();
    let messages: Vec<_> = errors
        .issues()
        .iter()
        .map(|issue| issue.message.as_str())
        .collect();

    assert!(
        messages
            .iter()
            .any(|message| message.contains("repository id must start"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("duplicate column id"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("duplicate provider id"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("mapped more than once"))
    );
}

#[test]
fn permits_a_temporarily_missing_repository() {
    let workspace = valid_workspace();

    assert!(workspace.validate().is_ok());
}
