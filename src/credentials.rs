use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;

use crate::domain::{ProviderConfig, ProviderId, WorkspaceConfig, WorkspaceId};

const KEYRING_SERVICE: &str = "kodeck";
const FALLBACK_ENVIRONMENT_VARIABLE: &str = "KODECK_GITLAB_TOKEN";

pub trait CredentialStore {
    fn get_password(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<String>, CredentialStoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialStoreError;

pub trait CredentialEnvironment {
    fn value(&self, variable: &str) -> Option<OsString>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCredentialStore;

impl CredentialStore for SystemCredentialStore {
    fn get_password(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<String>, CredentialStoreError> {
        let entry = keyring::Entry::new(service, account).map_err(|_| CredentialStoreError)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(CredentialStoreError),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnvironment;

impl CredentialEnvironment for ProcessEnvironment {
    fn value(&self, variable: &str) -> Option<OsString> {
        env::var_os(variable).filter(|value| !value.is_empty())
    }
}

pub struct SecretValue(String);

impl SecretValue {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    CredentialStore,
    Environment,
}

#[derive(Debug)]
pub struct ResolvedCredential {
    pub secret: SecretValue,
    pub source: CredentialSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    Missing { provider_id: ProviderId },
    CredentialStoreUnavailable { provider_id: ProviderId },
}

impl fmt::Display for CredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { provider_id } => {
                write!(
                    formatter,
                    "no credential found for provider '{provider_id}'"
                )
            }
            Self::CredentialStoreUnavailable { provider_id } => write!(
                formatter,
                "credential store is unavailable and no environment token exists for provider '{provider_id}'"
            ),
        }
    }
}

impl Error for CredentialError {}

pub fn resolve_gitlab_token(
    workspace_id: WorkspaceId,
    provider_id: &ProviderId,
) -> Result<ResolvedCredential, CredentialError> {
    resolve_gitlab_token_with(
        &SystemCredentialStore,
        &ProcessEnvironment,
        workspace_id,
        provider_id,
    )
}

pub fn credential_warnings(workspace: &WorkspaceConfig) -> Vec<String> {
    workspace
        .providers
        .iter()
        .filter_map(|provider| match provider {
            ProviderConfig::Gitlab { id, .. } => resolve_gitlab_token(workspace.id, id)
                .err()
                .map(|error| error.to_string()),
        })
        .collect()
}

pub fn resolve_gitlab_token_with(
    store: &dyn CredentialStore,
    environment: &dyn CredentialEnvironment,
    workspace_id: WorkspaceId,
    provider_id: &ProviderId,
) -> Result<ResolvedCredential, CredentialError> {
    let account = format!("{workspace_id}:gitlab:{provider_id}");
    let store_result = store.get_password(KEYRING_SERVICE, &account);
    let store_unavailable = store_result.is_err();
    if let Ok(Some(secret)) = store_result
        && !secret.is_empty()
    {
        return Ok(ResolvedCredential {
            secret: SecretValue(secret),
            source: CredentialSource::CredentialStore,
        });
    }

    for variable in [
        provider_environment_variable(provider_id),
        FALLBACK_ENVIRONMENT_VARIABLE.to_owned(),
    ] {
        if let Some(value) = environment.value(&variable)
            && let Some(secret) = value.to_str()
            && !secret.is_empty()
        {
            return Ok(ResolvedCredential {
                secret: SecretValue(secret.to_owned()),
                source: CredentialSource::Environment,
            });
        }
    }

    match store_unavailable {
        true => Err(CredentialError::CredentialStoreUnavailable {
            provider_id: provider_id.clone(),
        }),
        false => Err(CredentialError::Missing {
            provider_id: provider_id.clone(),
        }),
    }
}

fn provider_environment_variable(provider_id: &ProviderId) -> String {
    let suffix: String = provider_id
        .as_str()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{FALLBACK_ENVIRONMENT_VARIABLE}_{suffix}")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct FakeStore(Result<Option<String>, CredentialStoreError>);

    impl CredentialStore for FakeStore {
        fn get_password(
            &self,
            _service: &str,
            _account: &str,
        ) -> Result<Option<String>, CredentialStoreError> {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct FakeEnvironment(HashMap<String, OsString>);

    impl CredentialEnvironment for FakeEnvironment {
        fn value(&self, variable: &str) -> Option<OsString> {
            self.0.get(variable).cloned()
        }
    }

    fn workspace_id() -> WorkspaceId {
        WorkspaceId::from_uuid(
            uuid::Uuid::parse_str("f94d2d12-5acf-4370-bb04-41ec860bb205").unwrap(),
        )
    }

    #[test]
    fn credential_store_has_priority_over_environment() {
        let environment = FakeEnvironment(HashMap::from([(
            FALLBACK_ENVIRONMENT_VARIABLE.to_owned(),
            OsString::from("environment-secret"),
        )]));

        let credential = resolve_gitlab_token_with(
            &FakeStore(Ok(Some("keyring-secret".to_owned()))),
            &environment,
            workspace_id(),
            &ProviderId::from("main-gitlab"),
        )
        .unwrap();

        assert_eq!(credential.source, CredentialSource::CredentialStore);
        assert_eq!(credential.secret.expose(), "keyring-secret");
    }

    #[test]
    fn provider_environment_variable_precedes_the_fallback() {
        let environment = FakeEnvironment(HashMap::from([
            (
                "KODECK_GITLAB_TOKEN_MAIN_GITLAB".to_owned(),
                OsString::from("provider-secret"),
            ),
            (
                FALLBACK_ENVIRONMENT_VARIABLE.to_owned(),
                OsString::from("fallback-secret"),
            ),
        ]));

        let credential = resolve_gitlab_token_with(
            &FakeStore(Ok(None)),
            &environment,
            workspace_id(),
            &ProviderId::from("main-gitlab"),
        )
        .unwrap();

        assert_eq!(credential.source, CredentialSource::Environment);
        assert_eq!(credential.secret.expose(), "provider-secret");
    }

    #[test]
    fn environment_is_used_when_the_credential_store_is_unavailable() {
        let environment = FakeEnvironment(HashMap::from([(
            FALLBACK_ENVIRONMENT_VARIABLE.to_owned(),
            OsString::from("environment-secret"),
        )]));

        let credential = resolve_gitlab_token_with(
            &FakeStore(Err(CredentialStoreError)),
            &environment,
            workspace_id(),
            &ProviderId::from("main-gitlab"),
        )
        .unwrap();

        assert_eq!(credential.source, CredentialSource::Environment);
    }

    #[test]
    fn secret_is_redacted_from_debug_and_errors() {
        let secret = "never-print-this-token";
        let credential = resolve_gitlab_token_with(
            &FakeStore(Ok(Some(secret.to_owned()))),
            &FakeEnvironment::default(),
            workspace_id(),
            &ProviderId::from("main-gitlab"),
        )
        .unwrap();

        assert!(!format!("{credential:?}").contains(secret));
        assert!(!format!("{:?}", credential.secret).contains(secret));
    }

    #[test]
    fn missing_credentials_only_identify_the_provider() {
        let error = resolve_gitlab_token_with(
            &FakeStore(Ok(None)),
            &FakeEnvironment::default(),
            workspace_id(),
            &ProviderId::from("main-gitlab"),
        )
        .unwrap_err();

        assert_eq!(
            error,
            CredentialError::Missing {
                provider_id: ProviderId::from("main-gitlab")
            }
        );
        assert_eq!(
            error.to_string(),
            "no credential found for provider 'main-gitlab'"
        );
    }
}
