//! Storage abstraction module.
//!
//! This module provides storage backend traits and implementations.

pub mod file;
pub mod memory;

pub use file::FileStorage;
pub use memory::InMemoryStorage;

use crate::agent::Agent;
use crate::message::Message;
use crate::session::{Session, SessionMeta};
use async_trait::async_trait;
use neoco_core::ids::{AgentUlid, MessageId, SessionUlid};
use thiserror::Error;

/// Storage backend trait for persisting session data.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Saves session metadata and hierarchy.
    async fn save_session(
        &self,
        session_meta: &SessionMeta,
        hierarchy_meta: &crate::agent::HierarchyMeta,
    ) -> Result<(), StorageError>;
    /// Loads a session by ID.
    async fn load_session(&self, id: &SessionUlid) -> Result<Session, StorageError>;
    /// Deletes a session by ID.
    async fn delete_session(&self, id: &SessionUlid) -> Result<(), StorageError>;
    /// Lists all sessions.
    async fn list_sessions(&self) -> Result<Vec<SessionUlid>, StorageError>;

    /// Saves an agent.
    async fn save_agent(&self, agent: &Agent) -> Result<(), StorageError>;
    /// Loads an agent by ID.
    async fn load_agent(&self, id: &AgentUlid) -> Result<Agent, StorageError>;
    /// Lists all agents in a session.
    async fn list_agents(&self, session_ulid: &SessionUlid)
    -> Result<Vec<AgentUlid>, StorageError>;

    /// Appends a message to an agent.
    async fn append_message(
        &self,
        agent_ulid: &AgentUlid,
        message: &Message,
    ) -> Result<(), StorageError>;
    /// Loads all messages for an agent.
    async fn load_messages(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, StorageError>;
    /// Deletes messages before a given ID.
    async fn delete_prefix(
        &self,
        agent_ulid: &AgentUlid,
        before_id: MessageId,
    ) -> Result<(), StorageError>;
    /// Deletes messages after a given ID.
    async fn delete_suffix(
        &self,
        agent_ulid: &AgentUlid,
        after_id: MessageId,
    ) -> Result<(), StorageError>;
}

/// Storage errors.
#[derive(Debug, Error)]
pub enum StorageError {
    /// I/O error.
    #[error("IO错误: {0}")]
    Io(#[source] std::io::Error),

    /// Not found error.
    #[error("文件不存在: {0}")]
    NotFound(String),

    /// Serialization error.
    #[error("序列化错误: {0}")]
    Serialization(String),

    /// Data corruption error.
    #[error("文件损坏: {0}")]
    Corruption(String),
}

impl StorageError {
    /// Checks if the error is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Io(e) if e.kind() == std::io::ErrorKind::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_error_is_retryable() {
        let io_error = StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        ));
        assert!(io_error.is_retryable());

        let other_error = StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        ));
        assert!(!other_error.is_retryable());
    }

    #[test]
    fn test_storage_error_display() {
        let error = StorageError::NotFound("/test/path".to_string());
        assert_eq!(error.to_string(), "文件不存在: /test/path");

        let error = StorageError::Serialization("invalid json".to_string());
        assert_eq!(error.to_string(), "序列化错误: invalid json");
    }
}
