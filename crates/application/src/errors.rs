//! # Application Errors
//!
//! Error types for application layer operations.

use thiserror::Error;

/// Result type alias for application operations
pub type ApplicationResult<T> = Result<T, ApplicationError>;

/// Errors that can occur in application operations
#[derive(Error, Debug, Clone)]
pub enum ApplicationError {
    /// Domain error wrapper
    #[error("Domain error: {0}")]
    Domain(String),

    /// Entity not found
    #[error("Entity not found: {0}")]
    NotFound(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Repository error
    #[error("Repository error: {0}")]
    Repository(String),

    /// Infrastructure error
    #[error("Infrastructure error: {0}")]
    Infrastructure(String),

    /// Unauthorized operation
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Conflict (e.g., duplicate)
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl ApplicationError {
    /// Creates a not found error
    #[must_use]
    pub fn not_found(entity: impl Into<String>) -> Self {
        Self::NotFound(entity.into())
    }

    /// Creates a validation error
    #[must_use]
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    /// Creates a repository error
    #[must_use]
    pub fn repository(msg: impl Into<String>) -> Self {
        Self::Repository(msg.into())
    }

    /// Creates an infrastructure error
    #[must_use]
    pub fn infrastructure(msg: impl Into<String>) -> Self {
        Self::Infrastructure(msg.into())
    }

    /// Creates a not implemented error
    #[must_use]
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::NotImplemented(msg.into())
    }
}

impl From<domain::DomainError> for ApplicationError {
    fn from(err: domain::DomainError) -> Self {
        Self::Domain(err.to_string())
    }
}
