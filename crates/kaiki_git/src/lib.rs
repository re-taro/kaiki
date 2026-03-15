pub mod commit_explorer;

use thiserror::Error;

/// Errors that can occur in git operations.
#[derive(Debug, Error)]
pub enum GitError {
    #[error("git error: {0}")]
    Git(String),
    #[error("repository not found at: {0}")]
    RepoNotFound(String),
    #[error("no suitable base commit found")]
    NoBaseCommit,
}

/// Trait for generating storage keys.
pub trait KeyGenerator: Send + Sync {
    /// Get the key for the expected (baseline) images.
    fn get_expected_key(&self) -> Result<Option<String>, GitError>;

    /// Get the key for the actual (current) images.
    fn get_actual_key(&self) -> Result<String, GitError>;
}

/// Simple key generator that returns a static key.
pub struct SimpleKeygen {
    pub expected_key: String,
}

impl KeyGenerator for SimpleKeygen {
    fn get_expected_key(&self) -> Result<Option<String>, GitError> {
        if self.expected_key.is_empty() { Ok(None) } else { Ok(Some(self.expected_key.clone())) }
    }

    fn get_actual_key(&self) -> Result<String, GitError> {
        Ok(self.expected_key.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_keygen_expected_key() {
        let keygen = SimpleKeygen { expected_key: "abc123".to_string() };
        assert_eq!(keygen.get_expected_key().unwrap(), Some("abc123".to_string()));
    }

    #[test]
    fn test_simple_keygen_actual_key() {
        let keygen = SimpleKeygen { expected_key: "abc123".to_string() };
        assert_eq!(keygen.get_actual_key().unwrap(), "abc123");
    }

    #[test]
    fn test_simple_keygen_empty_expected() {
        let keygen = SimpleKeygen { expected_key: String::new() };
        assert_eq!(keygen.get_expected_key().unwrap(), None);
    }

    #[test]
    fn test_simple_keygen_actual_key_empty() {
        let keygen = SimpleKeygen { expected_key: String::new() };
        assert_eq!(keygen.get_actual_key().unwrap(), "");
    }
}
