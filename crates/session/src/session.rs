//! Session domain model module.

use crate::agent::AgentHierarchy;
use chrono::{DateTime, Utc};
use neoco_core::ids::{AgentUlid, MessageId, SessionUlid};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

/// Allocates message IDs for a session.
#[derive(Debug, Clone)]
pub struct MessageIdAllocator {
    /// The current message ID.
    current: MessageId,
}

impl MessageIdAllocator {
    /// Creates a new allocator starting from the given value.
    #[must_use]
    pub fn new(start: u64) -> Self {
        Self {
            current: MessageId::from_u64(start.saturating_sub(1)).unwrap_or_default(),
        }
    }

    /// Returns the next message ID.
    pub fn next_id(&mut self) -> Option<MessageId> {
        self.current = self.current.increment();
        Some(self.current)
    }

    /// Returns the current message ID.
    #[must_use]
    pub fn current_id(&self) -> Option<MessageId> {
        Some(self.current)
    }
}

/// Session type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SessionType {
    /// Direct session with optional initial message.
    Direct {
        /// Initial message.
        initial_message: Option<String>,
    },
    /// TUI session.
    #[serde(alias = "repl")]
    Tui,
    /// Workflow session.
    Workflow {
        /// Workflow ID.
        workflow_id: String,
    },
}

impl Default for SessionType {
    fn default() -> Self {
        Self::Direct {
            initial_message: None,
        }
    }
}

/// Session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// User ID.
    pub user_id: Option<String>,
    /// Working directory.
    pub working_dir: PathBuf,
    /// Initial prompt.
    pub initial_prompt: Option<String>,
    /// Custom fields.
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            user_id: None,
            working_dir: PathBuf::new(),
            initial_prompt: None,
            custom: HashMap::new(),
        }
    }
}

/// Session domain model.
pub struct Session {
    /// Session ID.
    id: SessionUlid,
    /// Session type.
    ty: SessionType,
    /// Root agent ULID.
    root_agent_ulid: AgentUlid,
    /// Agent hierarchy.
    hierarchy: AgentHierarchy,
    /// Message ID allocator.
    id_allocator: RefCell<MessageIdAllocator>,
    /// Session metadata.
    metadata: SessionMetadata,
    /// Creation timestamp.
    created_at: DateTime<Utc>,
    /// Last update timestamp.
    updated_at: DateTime<Utc>,
}

impl Session {
    /// Creates a new session.
    #[must_use]
    pub fn new(session_type: SessionType, metadata: SessionMetadata) -> Self {
        let id = SessionUlid::new();
        let root_agent_ulid = AgentUlid::new_root(&id);

        Self {
            id,
            ty: session_type,
            root_agent_ulid,
            hierarchy: AgentHierarchy::new(root_agent_ulid),
            id_allocator: RefCell::new(MessageIdAllocator::new(1)),
            metadata,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Returns the session ID.
    #[must_use]
    pub fn id(&self) -> &SessionUlid {
        &self.id
    }

    /// Returns the session type.
    #[must_use]
    pub fn session_type(&self) -> &SessionType {
        &self.ty
    }

    /// Returns the root agent ULID.
    #[must_use]
    pub fn root_agent_ulid(&self) -> &AgentUlid {
        &self.root_agent_ulid
    }

    /// Returns the agent hierarchy.
    #[must_use]
    pub fn hierarchy(&self) -> &AgentHierarchy {
        &self.hierarchy
    }

    /// Returns the message ID allocator.
    #[must_use]
    pub fn id_allocator(&self) -> &RefCell<MessageIdAllocator> {
        &self.id_allocator
    }

    /// Returns the session metadata.
    #[must_use]
    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    /// Returns the creation timestamp.
    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns the last update timestamp.
    #[must_use]
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Returns a mutable reference to the hierarchy.
    #[must_use]
    pub fn hierarchy_mut(&mut self) -> &mut AgentHierarchy {
        &mut self.hierarchy
    }

    /// Spawns a new agent in this session.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::AgentNotFound` if the parent agent does not exist.
    #[must_use = "the spawned agent ULID must be used"]
    pub fn spawn_agent(&mut self, parent_ulid: AgentUlid) -> Result<AgentUlid, SessionError> {
        if parent_ulid != self.root_agent_ulid && !self.hierarchy.has_agent(&parent_ulid) {
            return Err(SessionError::AgentNotFound(parent_ulid));
        }

        let agent_ulid = AgentUlid::new_child(&parent_ulid);
        self.hierarchy.add_child(parent_ulid, agent_ulid);
        self.updated_at = Utc::now();
        Ok(agent_ulid)
    }

    /// Allocates a new message ID.
    #[must_use]
    pub fn allocate_message_id(&self) -> Option<MessageId> {
        self.id_allocator.borrow_mut().next_id()
    }

    /// Restores a session from parameters.
    #[must_use]
    pub fn restore(params: SessionRestoreParams) -> Self {
        Self {
            id: params.id,
            ty: params.session_type,
            root_agent_ulid: params.root_agent_ulid,
            hierarchy: params.hierarchy,
            id_allocator: RefCell::new(params.id_allocator),
            metadata: params.metadata,
            created_at: params.created_at,
            updated_at: params.updated_at,
        }
    }
}

/// Parameters for restoring a session.
#[derive(Debug, Clone)]
pub struct SessionRestoreParams {
    /// Session ID.
    pub id: SessionUlid,
    /// Session type.
    pub session_type: SessionType,
    /// Root agent ULID.
    pub root_agent_ulid: AgentUlid,
    /// Agent hierarchy.
    pub hierarchy: AgentHierarchy,
    /// Message ID allocator.
    pub id_allocator: MessageIdAllocator,
    /// Session metadata.
    pub metadata: SessionMetadata,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Session metadata for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Session ID.
    pub id: SessionUlid,
    /// Session type.
    pub session_type: SessionType,
    /// Root agent ULID.
    pub root_agent_ulid: AgentUlid,
    /// Next message ID.
    pub next_message_id: u64,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Session metadata.
    pub metadata: SessionMetadata,
}

impl From<&Session> for SessionMeta {
    fn from(session: &Session) -> Self {
        let current_id = session.id_allocator().borrow().current_id();
        Self {
            id: *session.id(),
            session_type: session.session_type().clone(),
            root_agent_ulid: *session.root_agent_ulid(),
            next_message_id: current_id.map_or(1, |id| id.as_u64()),
            created_at: session.created_at(),
            updated_at: session.updated_at(),
            metadata: session.metadata().clone(),
        }
    }
}

/// Session errors.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Session not found.
    #[error("Session不存在: {0}")]
    NotFound(SessionUlid),

    /// Agent not found.
    #[error("Agent不存在: {0}")]
    AgentNotFound(AgentUlid),

    /// Storage error.
    #[error("存储错误: {0}")]
    Storage(String),

    /// Serialization error.
    #[error("序列化错误: {0}")]
    Serialization(String),

    /// Message ID overflow.
    #[error("消息ID分配失败")]
    MessageIdOverflow,
}

impl SessionError {
    /// Checks if the error is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Storage(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session_type = SessionType::Direct {
            initial_message: Some("Hello".to_string()),
        };
        let metadata = SessionMetadata {
            user_id: Some("user1".to_string()),
            working_dir: PathBuf::from("/test"),
            initial_prompt: None,
            custom: HashMap::new(),
        };

        let session = Session::new(session_type, metadata);

        assert!(!session.id().to_string().is_empty());
        assert!(session.root_agent_ulid().to_string().contains(':'));
    }

    #[test]
    fn test_session_spawn_agent() {
        let mut session = Session::new(SessionType::default(), SessionMetadata::default());

        let root_id = *session.root_agent_ulid();
        let child_id = session.spawn_agent(root_id).unwrap();
        assert!(session.hierarchy().has_agent(&child_id));
    }

    #[test]
    fn test_session_spawn_agent_invalid_parent() {
        let mut session = Session::new(SessionType::default(), SessionMetadata::default());

        let fake_parent = AgentUlid::new_root(&SessionUlid::new());
        let result = session.spawn_agent(fake_parent);
        result.unwrap_err();
    }

    #[test]
    fn test_allocate_message_id() {
        let session = Session::new(SessionType::default(), SessionMetadata::default());

        let id1 = session.allocate_message_id();
        assert!(id1.is_some());

        let id2 = session.allocate_message_id();
        assert!(id2.is_some());

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_session_meta_from_session() {
        let session = Session::new(
            SessionType::Tui,
            SessionMetadata {
                user_id: Some("user1".to_string()),
                working_dir: PathBuf::from("/home"),
                initial_prompt: Some("Test prompt".to_string()),
                custom: HashMap::new(),
            },
        );

        let meta = SessionMeta::from(&session);

        assert_eq!(meta.id, *session.id());
        assert_eq!(meta.metadata.user_id, session.metadata().user_id.clone());
    }
}
