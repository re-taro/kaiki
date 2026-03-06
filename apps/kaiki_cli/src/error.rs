use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Core(#[from] kaiki_core::CoreError),

    #[error("plugin config error: {0}")]
    PluginConfig(#[from] serde_json::Error),

    #[error("validation error: {0}")]
    Validation(String),
}

impl From<kaiki_config::ConfigError> for CliError {
    fn from(e: kaiki_config::ConfigError) -> Self {
        Self::Core(kaiki_core::CoreError::from(e))
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        Self::Core(kaiki_core::CoreError::from(e))
    }
}

impl From<kaiki_git::GitError> for CliError {
    fn from(e: kaiki_git::GitError) -> Self {
        Self::Core(kaiki_core::CoreError::from(e))
    }
}

impl From<kaiki_storage::StorageError> for CliError {
    fn from(e: kaiki_storage::StorageError) -> Self {
        Self::Core(kaiki_core::CoreError::from(e))
    }
}

impl From<kaiki_notify::NotifyError> for CliError {
    fn from(e: kaiki_notify::NotifyError) -> Self {
        Self::Core(kaiki_core::CoreError::from(e))
    }
}

impl From<dialoguer::Error> for CliError {
    fn from(e: dialoguer::Error) -> Self {
        match e {
            dialoguer::Error::IO(io_err) => Self::Core(kaiki_core::CoreError::from(io_err)),
        }
    }
}
